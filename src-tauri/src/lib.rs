mod mcp_server;
mod simple_browser;

use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

// Server state for Tauri
struct ServerHandle {
    state: Arc<mcp_server::McpServerState>,
    handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn start_mcp_server(state: State<'_, Arc<Mutex<ServerHandle>>>) -> Result<String, String> {
    let server_handle = state.lock().await;
    
    // Check if server is already running
    if *server_handle.state.running.lock().await {
        return Err("Server is already running".to_string());
    }
    
    let server_state = server_handle.state.clone();
    
    // Start server in background task
    let handle = tokio::spawn(async move {
        if let Err(e) = mcp_server::start_mcp_server(server_state).await {
            eprintln!("MCP Server error: {}", e);
        }
    });
    
    // Store the handle
    let mut stored_handle = server_handle.handle.lock().await;
    *stored_handle = Some(handle);
    
    Ok(format!("MCP Server started on port {}", server_handle.state.port))
}

#[tauri::command]
async fn stop_mcp_server(state: State<'_, Arc<Mutex<ServerHandle>>>) -> Result<String, String> {
    let server_handle = state.lock().await;
    
    // Check if server is running
    if !*server_handle.state.running.lock().await {
        return Err("Server is not running".to_string());
    }
    
    // Set running to false
    *server_handle.state.running.lock().await = false;
    
    // Cancel the server task
    let mut handle = server_handle.handle.lock().await;
    if let Some(h) = handle.take() {
        h.abort();
    }
    
    Ok("MCP Server stopped".to_string())
}

#[tauri::command]
async fn get_mcp_server_status(state: State<'_, Arc<Mutex<ServerHandle>>>) -> Result<bool, String> {
    let server_handle = state.lock().await;
    let running = *server_handle.state.running.lock().await;
    Ok(running)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize MCP server state
    let mcp_state = Arc::new(mcp_server::McpServerState::new(37650));
    let server_handle = Arc::new(Mutex::new(ServerHandle {
        state: mcp_state,
        handle: Arc::new(Mutex::new(None)),
    }));
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(server_handle)
        .invoke_handler(tauri::generate_handler![
            greet,
            start_mcp_server,
            stop_mcp_server,
            get_mcp_server_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
