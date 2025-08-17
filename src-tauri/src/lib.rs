mod mcp_server;
mod redmine_client;
mod database;

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

// Database state for Tauri
struct DatabaseHandle {
    db: Arc<Mutex<Option<Arc<database::Database>>>>,
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

// Database commands
#[tauri::command]
async fn init_database(db_state: State<'_, Arc<Mutex<DatabaseHandle>>>) -> Result<String, String> {
    let db_handle = db_state.lock().await;
    
    // Get app data directory using home directory
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let db_dir = home_dir.join(".redmine-mcp");
    let db_path = db_dir.join("cache.db");
    
    // Create and initialize database
    match database::Database::new(db_path.clone()) {
        Ok(database) => {
            *db_handle.db.lock().await = Some(Arc::new(database));
            Ok(format!("Database initialized at: {}", db_path.display()))
        }
        Err(e) => Err(format!("Failed to initialize database: {}", e))
    }
}

#[tauri::command]
async fn get_cache_stats(db_state: State<'_, Arc<Mutex<DatabaseHandle>>>) -> Result<serde_json::Value, String> {
    let db_handle = db_state.lock().await;
    let db_lock = db_handle.db.lock().await;
    
    if let Some(ref db) = *db_lock {
        db.get_cache_stats()
            .map_err(|e| format!("Failed to get cache stats: {}", e))
    } else {
        Err("Database not initialized".to_string())
    }
}

#[tauri::command]
async fn get_cached_issues(
    db_state: State<'_, Arc<Mutex<DatabaseHandle>>>,
    project_id: Option<i32>,
    limit: Option<usize>
) -> Result<Vec<database::CachedIssue>, String> {
    let db_handle = db_state.lock().await;
    let db_lock = db_handle.db.lock().await;
    
    if let Some(ref db) = *db_lock {
        db.get_cached_issues(project_id, limit.unwrap_or(100))
            .map_err(|e| format!("Failed to get cached issues: {}", e))
    } else {
        Err("Database not initialized".to_string())
    }
}

#[tauri::command]
async fn get_cached_projects(
    db_state: State<'_, Arc<Mutex<DatabaseHandle>>>,
    limit: Option<usize>
) -> Result<Vec<database::CachedProject>, String> {
    let db_handle = db_state.lock().await;
    let db_lock = db_handle.db.lock().await;
    
    if let Some(ref db) = *db_lock {
        db.get_cached_projects(limit.unwrap_or(100))
            .map_err(|e| format!("Failed to get cached projects: {}", e))
    } else {
        Err("Database not initialized".to_string())
    }
}

#[tauri::command]
async fn clear_cache(db_state: State<'_, Arc<Mutex<DatabaseHandle>>>) -> Result<String, String> {
    let db_handle = db_state.lock().await;
    let db_lock = db_handle.db.lock().await;
    
    if let Some(ref db) = *db_lock {
        db.clear_all_cache()
            .map_err(|e| format!("Failed to clear cache: {}", e))?;
        Ok("Cache cleared successfully".to_string())
    } else {
        Err("Database not initialized".to_string())
    }
}

#[tauri::command]
async fn clear_old_cache(
    db_state: State<'_, Arc<Mutex<DatabaseHandle>>>,
    days: i64
) -> Result<String, String> {
    let db_handle = db_state.lock().await;
    let db_lock = db_handle.db.lock().await;
    
    if let Some(ref db) = *db_lock {
        db.clear_old_cache(days)
            .map_err(|e| format!("Failed to clear old cache: {}", e))?;
        Ok(format!("Cleared cache older than {} days", days))
    } else {
        Err("Database not initialized".to_string())
    }
}

// Download ALL issues from Redmine with pagination
#[tauri::command]
async fn download_all_issues(
    db_state: State<'_, Arc<Mutex<DatabaseHandle>>>
) -> Result<String, String> {
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        let db_handle = db_state.lock().await;
        let db_lock = db_handle.db.lock().await;
        
        if let Some(ref db) = *db_lock {
            let mut total_cached = 0;
            let mut offset = 0;
            const LIMIT: usize = 100;
            let now = chrono::Utc::now();
            
            loop {
                // Fetch issues with pagination
                let mut params = HashMap::new();
                params.insert("limit".to_string(), LIMIT.to_string());
                params.insert("offset".to_string(), offset.to_string());
                params.insert("status_id".to_string(), "*".to_string()); // All statuses
                
                let issues_json = client.list_issues(params)
                    .await
                    .map_err(|e| format!("Failed to list issues at offset {}: {}", offset, e))?;
                
                if let Some(issues_array) = issues_json.get("issues").and_then(|v| v.as_array()) {
                    if issues_array.is_empty() {
                        break; // No more issues
                    }
                    
                    for issue in issues_array {
                        // Convert JSON to CachedIssue
                        let cached_issue = database::CachedIssue {
                            id: issue.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                            project_id: issue.get("project")
                                .and_then(|p| p.get("id"))
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32,
                            project_name: issue.get("project")
                                .and_then(|p| p.get("name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            subject: issue.get("subject")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            description: issue.get("description")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            status_id: issue.get("status")
                                .and_then(|s| s.get("id"))
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32,
                            status_name: issue.get("status")
                                .and_then(|s| s.get("name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            priority_id: issue.get("priority")
                                .and_then(|p| p.get("id"))
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32,
                            priority_name: issue.get("priority")
                                .and_then(|p| p.get("name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            assigned_to_id: issue.get("assigned_to")
                                .and_then(|a| a.get("id"))
                                .and_then(|v| v.as_i64())
                                .map(|id| id as i32),
                            assigned_to_name: issue.get("assigned_to")
                                .and_then(|a| a.get("name"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            created_on: issue.get("created_on")
                                .and_then(|v| v.as_str())
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                                .unwrap_or(now),
                            updated_on: issue.get("updated_on")
                                .and_then(|v| v.as_str())
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                                .unwrap_or(now),
                            cached_at: now,
                        };
                        
                        if let Ok(_) = db.cache_issue(&cached_issue) {
                            total_cached += 1;
                        }
                    }
                    
                    offset += issues_array.len();
                } else {
                    break;
                }
            }
            
            Ok(format!("Downloaded and cached {} issues", total_cached))
        } else {
            Err("Database not initialized".to_string())
        }
    } else {
        Err("Redmine client not configured".to_string())
    }
}

// Search cached issues
#[tauri::command]
async fn search_cached_issues(
    db_state: State<'_, Arc<Mutex<DatabaseHandle>>>,
    query: String,
    project_id: Option<i32>,
    status_id: Option<i32>,
    assigned_to_id: Option<i32>,
    limit: Option<usize>
) -> Result<Vec<database::CachedIssue>, String> {
    let db_handle = db_state.lock().await;
    let db_lock = db_handle.db.lock().await;
    
    if let Some(ref db) = *db_lock {
        db.search_issues(&query, project_id, status_id, assigned_to_id, limit.unwrap_or(100), 0)
            .map_err(|e| format!("Failed to search issues: {}", e))
    } else {
        Err("Database not initialized".to_string())
    }
}

// Cache Redmine data to database
#[tauri::command]
async fn cache_redmine_issues(
    db_state: State<'_, Arc<Mutex<DatabaseHandle>>>,
    params: HashMap<String, String>
) -> Result<String, String> {
    // First, fetch issues from Redmine
    let guard = redmine_client::get_client()
        .await
        .map_err(|e| format!("Failed to get client: {}", e))?;
    
    if let Some(client) = guard.as_ref() {
        let issues_json = client.list_issues(params)
            .await
            .map_err(|e| format!("Failed to list issues: {}", e))?;
        
        // Parse and cache the issues
        let db_handle = db_state.lock().await;
        let db_lock = db_handle.db.lock().await;
        
        if let Some(ref db) = *db_lock {
            if let Some(issues_array) = issues_json.get("issues").and_then(|v| v.as_array()) {
                let mut cached_count = 0;
                let now = chrono::Utc::now();
                
                for issue in issues_array {
                    // Convert JSON to CachedIssue
                    let cached_issue = database::CachedIssue {
                        id: issue.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                        project_id: issue.get("project")
                            .and_then(|p| p.get("id"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32,
                        project_name: issue.get("project")
                            .and_then(|p| p.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        subject: issue.get("subject")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        description: issue.get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        status_id: issue.get("status")
                            .and_then(|s| s.get("id"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32,
                        status_name: issue.get("status")
                            .and_then(|s| s.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        priority_id: issue.get("priority")
                            .and_then(|p| p.get("id"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32,
                        priority_name: issue.get("priority")
                            .and_then(|p| p.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        assigned_to_id: issue.get("assigned_to")
                            .and_then(|a| a.get("id"))
                            .and_then(|v| v.as_i64())
                            .map(|id| id as i32),
                        assigned_to_name: issue.get("assigned_to")
                            .and_then(|a| a.get("name"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        created_on: issue.get("created_on")
                            .and_then(|v| v.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or(now),
                        updated_on: issue.get("updated_on")
                            .and_then(|v| v.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or(now),
                        cached_at: now,
                    };
                    
                    if let Ok(_) = db.cache_issue(&cached_issue) {
                        cached_count += 1;
                    }
                }
                
                Ok(format!("Cached {} issues", cached_count))
            } else {
                Ok("No issues to cache".to_string())
            }
        } else {
            Err("Database not initialized".to_string())
        }
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
    
    // Initialize database state
    let database_handle = Arc::new(Mutex::new(DatabaseHandle {
        db: Arc::new(Mutex::new(None)),
    }));
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(server_handle)
        .manage(database_handle)
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
            get_redmine_project,
            // Database commands
            init_database,
            get_cache_stats,
            get_cached_issues,
            get_cached_projects,
            clear_cache,
            clear_old_cache,
            cache_redmine_issues,
            download_all_issues,
            search_cached_issues
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}