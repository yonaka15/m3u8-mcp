mod mcp_server;
mod redmine_client;

use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;
use std::collections::HashMap;

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
async fn start_mcp_server(
    state: State<'_, Arc<Mutex<ServerHandle>>>, 
    port: u16,
    enabled_tools: Vec<String>
) -> Result<String, String> {
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
    
    // Create new server state with specified port and enabled tools
    let new_state = Arc::new(mcp_server::McpServerState::new_with_tools(port, enabled_tools));
    
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

// Redmine configuration commands
#[tauri::command]
async fn configure_redmine(host: String, api_key: String) -> Result<String, String> {
    // Initialize Redmine client
    redmine_client::init_client(host.clone(), api_key.clone())
        .await
        .map_err(|e| format!("Failed to configure Redmine: {}", e))?;
    
    // Save configuration to local storage
    save_redmine_config(host.clone(), api_key)
        .await
        .map_err(|e| format!("Failed to save configuration: {}", e))?;
    
    Ok(format!("Redmine configured: {}", host))
}

// Save Redmine configuration to local storage
async fn save_redmine_config(host: String, api_key: String) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    
    // Get app data directory using home directory
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let config_dir = home_dir.join(".redmine-mcp");
    
    // Create directory if it doesn't exist
    fs::create_dir_all(&config_dir)?;
    
    // Save configuration as JSON
    let config_path = config_dir.join("config.json");
    let config = serde_json::json!({
        "host": host,
        "api_key": api_key
    });
    
    fs::write(config_path, config.to_string())?;
    Ok(())
}

// Load Redmine configuration from local storage
#[tauri::command]
async fn load_redmine_config() -> Result<serde_json::Value, String> {
    use std::fs;
    
    // Get app data directory using home directory
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let config_dir = home_dir.join(".redmine-mcp");
    
    let config_path = config_dir.join("config.json");
    
    // Check if config file exists
    if !config_path.exists() {
        return Ok(serde_json::json!(null));
    }
    
    // Read and parse configuration
    let config_str = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read configuration: {}", e))?;
    
    let config: serde_json::Value = serde_json::from_str(&config_str)
        .map_err(|e| format!("Failed to parse configuration: {}", e))?;
    
    // Initialize Redmine client with saved configuration
    if let (Some(host), Some(api_key)) = (config.get("host"), config.get("api_key")) {
        if let (Some(host_str), Some(api_key_str)) = (host.as_str(), api_key.as_str()) {
            redmine_client::init_client(host_str.to_string(), api_key_str.to_string())
                .await
                .map_err(|e| format!("Failed to initialize Redmine client: {}", e))?;
        }
    }
    
    Ok(config)
}

#[tauri::command]
async fn test_redmine_connection() -> Result<serde_json::Value, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        // Test connection by getting current user
        match client.get_current_user().await {
            Ok(user) => Ok(user),
            Err(e) => Err(format!("Connection test failed: {}", e))
        }
    } else {
        Err("Redmine client not configured".to_string())
    }
}

// Redmine Issue commands
#[tauri::command]
async fn list_redmine_issues(params: HashMap<String, String>) -> Result<serde_json::Value, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        client.list_issues(params)
            .await
            .map_err(|e| format!("Failed to list issues: {}", e))
    } else {
        Err("Redmine client not configured".to_string())
    }
}

#[tauri::command]
async fn get_redmine_issue(id: u32) -> Result<serde_json::Value, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        client.get_issue(id)
            .await
            .map_err(|e| format!("Failed to get issue: {}", e))
    } else {
        Err("Redmine client not configured".to_string())
    }
}

#[tauri::command]
async fn create_redmine_issue(issue: serde_json::Value) -> Result<serde_json::Value, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        client.create_issue(issue)
            .await
            .map_err(|e| format!("Failed to create issue: {}", e))
    } else {
        Err("Redmine client not configured".to_string())
    }
}

#[tauri::command]
async fn update_redmine_issue(id: u32, issue: serde_json::Value) -> Result<serde_json::Value, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        client.update_issue(id, issue)
            .await
            .map_err(|e| format!("Failed to update issue: {}", e))
    } else {
        Err("Redmine client not configured".to_string())
    }
}

// Redmine Project commands
#[tauri::command]
async fn list_redmine_projects(params: HashMap<String, String>) -> Result<serde_json::Value, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        client.list_projects(params)
            .await
            .map_err(|e| format!("Failed to list projects: {}", e))
    } else {
        Err("Redmine client not configured".to_string())
    }
}

#[tauri::command]
async fn get_redmine_project(id: String) -> Result<serde_json::Value, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        client.get_project(&id)
            .await
            .map_err(|e| format!("Failed to get project: {}", e))
    } else {
        Err("Redmine client not configured".to_string())
    }
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
            configure_redmine,
            load_redmine_config,
            test_redmine_connection,
            list_redmine_issues,
            get_redmine_issue,
            create_redmine_issue,
            update_redmine_issue,
            list_redmine_projects,
            get_redmine_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}