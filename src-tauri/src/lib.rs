mod mcp_server;
mod cdp_browser;
mod config;

use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

// Server state for Tauri
struct ServerHandle {
    state: Arc<Mutex<Option<Arc<mcp_server::McpServerState>>>>,
    handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    port: Arc<Mutex<Option<u16>>>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn start_mcp_server(state: State<'_, Arc<Mutex<ServerHandle>>>, port: u16) -> Result<String, String> {
    // Validate port number (port 0 is not allowed for explicit binding)
    if port == 0 {
        return Err("Port number must be greater than 0".to_string());
    }
    
    if port < 1024 {
        return Err("Port number must be 1024 or higher (lower ports require root privileges)".to_string());
    }
    
    let server_handle = state.lock().await;
    
    // Check if server is already running
    let state_lock = server_handle.state.lock().await;
    if let Some(ref current_state) = *state_lock {
        if *current_state.running.lock().await {
            return Err("Server is already running".to_string());
        }
    }
    drop(state_lock);
    
    // First, check if the port is available by trying to connect to it
    // If we can connect, it means something is already listening on that port
    let addr = format!("127.0.0.1:{}", port);
    match tokio::net::TcpStream::connect(&addr).await {
        Ok(_) => {
            // Port is already in use
            return Err(format!("Port {} is already in use", port));
        }
        Err(_) => {
            // Port is free (connection failed means nothing is listening)
        }
    }
    
    // Create new server state with specified port
    let new_state = Arc::new(mcp_server::McpServerState::new(port));
    
    // Update the stored state
    let mut state_lock = server_handle.state.lock().await;
    *state_lock = Some(new_state.clone());
    drop(state_lock);
    
    // Update the port
    let mut port_lock = server_handle.port.lock().await;
    *port_lock = Some(port);
    drop(port_lock);
    
    // Clone state for the spawn task
    let task_state = new_state.clone();
    let task_port = port;
    
    // Start server in background task
    let handle = tokio::spawn(async move {
        if let Err(e) = mcp_server::start_mcp_server(task_state.clone()).await {
            eprintln!("MCP Server error: {}", e);
            // Mark server as not running on error
            *task_state.running.lock().await = false;
        }
    });
    
    // Store the handle
    let mut stored_handle = server_handle.handle.lock().await;
    *stored_handle = Some(handle);
    
    // Wait longer to ensure server starts or fails
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Double-check if server is actually running
    if !*new_state.running.lock().await {
        // Clean up state if server failed to start
        let mut state_lock = server_handle.state.lock().await;
        *state_lock = None;
        drop(state_lock);
        
        let mut port_lock = server_handle.port.lock().await;
        *port_lock = None;
        drop(port_lock);
        
        return Err(format!("Failed to start MCP Server on port {}. An unexpected error occurred.", task_port));
    }
    
    Ok(format!("MCP Server started on port {}", port))
}

#[tauri::command]
async fn stop_mcp_server(state: State<'_, Arc<Mutex<ServerHandle>>>) -> Result<String, String> {
    let server_handle = state.lock().await;
    
    // Check if server is running
    let state_lock = server_handle.state.lock().await;
    if let Some(ref current_state) = *state_lock {
        if !*current_state.running.lock().await {
            return Err("Server is not running".to_string());
        }
        // Set running to false
        *current_state.running.lock().await = false;
    } else {
        return Err("Server is not running".to_string());
    }
    drop(state_lock);
    
    // Clear the port
    let mut port_lock = server_handle.port.lock().await;
    *port_lock = None;
    drop(port_lock);
    
    // Cancel the server task
    let mut handle = server_handle.handle.lock().await;
    if let Some(h) = handle.take() {
        h.abort();
    }
    
    Ok("MCP Server stopped".to_string())
}

#[tauri::command]
async fn get_mcp_server_status(state: State<'_, Arc<Mutex<ServerHandle>>>) -> Result<serde_json::Value, String> {
    use serde_json::json;
    
    let server_handle = state.lock().await;
    
    let state_lock = server_handle.state.lock().await;
    let running = if let Some(ref current_state) = *state_lock {
        *current_state.running.lock().await
    } else {
        false
    };
    drop(state_lock);
    
    let port_lock = server_handle.port.lock().await;
    let port = *port_lock;
    drop(port_lock);
    
    Ok(json!({
        "running": running,
        "port": port
    }))
}

#[tauri::command]
async fn check_port_availability(port: u16) -> Result<bool, String> {
    // Validate port number
    if port == 0 {
        return Ok(false); // Invalid port
    }
    
    // Check if the port is available by trying to connect to it
    let addr = format!("127.0.0.1:{}", port);
    match tokio::net::TcpStream::connect(&addr).await {
        Ok(_) => {
            // Port is already in use
            Ok(false)
        }
        Err(_) => {
            // Port is free (connection failed means nothing is listening)
            Ok(true)
        }
    }
}

#[tauri::command]
async fn get_browser_status() -> Result<serde_json::Value, String> {
    use serde_json::json;
    
    let manager = crate::cdp_browser::BROWSER_MANAGER.read().await;
    let session_data = manager.export_session();
    
    Ok(json!({
        "connected": session_data.get("browser_connected").and_then(|v| v.as_bool()).unwrap_or(false),
        "tabs": session_data.get("tabs").cloned().unwrap_or(json!([])),
        "console_messages": manager.get_console_messages(20)
    }))
}

#[tauri::command]
async fn clear_browser_console() -> Result<String, String> {
    let mut manager = crate::cdp_browser::BROWSER_MANAGER.write().await;
    manager.clear_console();
    Ok("Console cleared".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize MCP server state
    let server_handle = Arc::new(Mutex::new(ServerHandle {
        state: Arc::new(Mutex::new(None)),
        handle: Arc::new(Mutex::new(None)),
        port: Arc::new(Mutex::new(None)),
    }));
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(server_handle)
        .invoke_handler(tauri::generate_handler![
            greet,
            start_mcp_server,
            stop_mcp_server,
            get_mcp_server_status,
            check_port_availability,
            get_browser_status,
            clear_browser_console
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
