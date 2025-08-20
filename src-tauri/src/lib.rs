mod mcp_server;
mod m3u8_parser;
mod ffmpeg_wrapper;
mod database;

use std::sync::Arc;
use std::path::PathBuf;
use tauri::{State, Emitter};
use tokio::sync::{Mutex, RwLock};

// Global state for current m3u8 URL
lazy_static::lazy_static! {
    pub static ref CURRENT_M3U8_URL: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));
}

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

// m3u8 parser state for Tauri
struct M3u8ParserHandle {
    parser: Arc<m3u8_parser::M3u8Parser>,
}

// FFmpeg state for Tauri  
struct FFmpegHandle {
    wrapper: Arc<Mutex<ffmpeg_wrapper::FFmpegWrapper>>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to m3u8 MCP!", name)
}

// m3u8 URL management commands
#[tauri::command]
async fn set_current_m3u8_url(url: String) -> Result<(), String> {
    let mut url_state = CURRENT_M3U8_URL.write().await;
    // Set to None if the URL is empty, otherwise Some(url)
    *url_state = if url.trim().is_empty() {
        None
    } else {
        // Save to history if not empty
        if let Err(e) = save_url_to_history(&url).await {
            eprintln!("Failed to save URL to history: {}", e);
        }
        Some(url)
    };
    Ok(())
}

#[tauri::command]
async fn get_current_m3u8_url() -> Result<Option<String>, String> {
    let url_state = CURRENT_M3U8_URL.read().await;
    Ok(url_state.clone())
}

// m3u8 parsing commands
#[tauri::command]
async fn parse_m3u8_url(
    parser_state: State<'_, M3u8ParserHandle>,
    url: String
) -> Result<m3u8_parser::ParsedPlaylist, String> {
    parser_state.parser
        .parse_url(&url)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn extract_m3u8_segments(
    parser_state: State<'_, M3u8ParserHandle>,
    url: String,
    base_url: Option<String>
) -> Result<Vec<String>, String> {
    parser_state.parser
        .extract_segments(&url, base_url.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// FFmpeg commands
#[tauri::command]
async fn check_ffmpeg_installation(
    ffmpeg_state: State<'_, Arc<Mutex<FFmpegHandle>>>
) -> Result<String, String> {
    let handle = ffmpeg_state.lock().await;
    let wrapper = handle.wrapper.lock().await;
    wrapper.check_installation()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn cancel_download(
    _app: tauri::AppHandle,
    ffmpeg_state: State<'_, Arc<Mutex<FFmpegHandle>>>
) -> Result<String, String> {
    println!("cancel_download command called");
    let handle = ffmpeg_state.lock().await;
    let wrapper = handle.wrapper.lock().await;
    
    println!("Calling FFmpegWrapper::cancel_download");
    wrapper.cancel_download()
        .await
        .map_err(|e| {
            let error_msg = e.to_string();
            eprintln!("Cancel failed: {}", error_msg);
            error_msg
        })?;
    
    println!("Download cancelled successfully");
    Ok("Download cancelled".to_string())
}

#[tauri::command]
async fn download_m3u8_stream(
    app: tauri::AppHandle,
    ffmpeg_state: State<'_, Arc<Mutex<FFmpegHandle>>>,
    url: String,
    output_path: Option<String>
) -> Result<String, String> {
    println!("Download requested for URL: {}", url);
    
    // Emit start event
    app.emit("download-progress", serde_json::json!({
        "status": "starting",
        "message": "Initializing download..."
    })).ok();
    
    let handle = ffmpeg_state.lock().await;
    let mut wrapper = handle.wrapper.lock().await;
    
    let output = if let Some(path) = output_path {
        println!("Using provided output path: {}", path);
        Some(PathBuf::from(path))
    } else {
        println!("No output path provided, will generate default");
        None
    };
    
    // Set the app handle for event emission
    wrapper.set_app_handle(Some(app.clone()));
    
    println!("Starting FFmpeg download...");
    let result_path = wrapper
        .download_stream(&url, output.as_deref())
        .await
        .map_err(|e| {
            let error_msg = format!("FFmpeg download failed: {}", e);
            eprintln!("{}", error_msg);
            // Emit error event
            app.emit("download-progress", serde_json::json!({
                "status": "error",
                "message": error_msg.clone()
            })).ok();
            error_msg
        })?;
    
    let path_str = result_path.to_string_lossy().to_string();
    println!("Download completed successfully: {}", path_str);
    
    // Emit completion event
    app.emit("download-progress", serde_json::json!({
        "status": "completed",
        "message": format!("Download completed: {}", path_str)
    })).ok();
    
    Ok(path_str)
}

#[tauri::command]
async fn convert_to_hls(
    ffmpeg_state: State<'_, Arc<Mutex<FFmpegHandle>>>,
    input_path: String,
    output_dir: String,
    segment_duration: u32
) -> Result<String, String> {
    let handle = ffmpeg_state.lock().await;
    let wrapper = handle.wrapper.lock().await;
    
    let result_path = wrapper
        .convert_to_hls(
            &PathBuf::from(input_path),
            &PathBuf::from(output_dir),
            segment_duration
        )
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(result_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn probe_stream(
    ffmpeg_state: State<'_, Arc<Mutex<FFmpegHandle>>>,
    url: String
) -> Result<String, String> {
    let handle = ffmpeg_state.lock().await;
    let wrapper = handle.wrapper.lock().await;
    
    wrapper.probe_stream(&url)
        .await
        .map_err(|e| e.to_string())
}

// URL history management
async fn save_url_to_history(url: &str) -> Result<(), String> {
    use std::fs;
    use serde_json::json;
    
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let config_dir = home_dir.join(".m3u8-mcp");
    
    fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;
    
    let history_path = config_dir.join("url_history.json");
    
    // Read existing history
    let mut history: Vec<serde_json::Value> = if history_path.exists() {
        let content = fs::read_to_string(&history_path)
            .unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| Vec::new())
    } else {
        Vec::new()
    };
    
    // Create new entry
    let entry = json!({
        "url": url,
        "timestamp": chrono::Local::now().to_rfc3339(),
    });
    
    // Check if URL already exists and remove it
    history.retain(|item| {
        item.get("url")
            .and_then(|v| v.as_str())
            .map(|u| u != url)
            .unwrap_or(true)
    });
    
    // Add new entry at the beginning
    history.insert(0, entry);
    
    // Keep only last 20 URLs
    history.truncate(20);
    
    // Save to file
    let json_str = serde_json::to_string_pretty(&history)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;
    
    fs::write(history_path, json_str)
        .map_err(|e| format!("Failed to save history: {}", e))?;
    
    Ok(())
}

#[tauri::command]
async fn get_url_history() -> Result<Vec<serde_json::Value>, String> {
    use std::fs;
    
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let history_path = home_dir.join(".m3u8-mcp").join("url_history.json");
    
    if !history_path.exists() {
        return Ok(Vec::new());
    }
    
    let content = fs::read_to_string(history_path)
        .map_err(|e| format!("Failed to read history: {}", e))?;
    
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse history: {}", e))
}

#[tauri::command]
async fn get_last_used_url() -> Result<Option<String>, String> {
    let history = get_url_history().await?;
    
    if history.is_empty() {
        return Ok(None);
    }
    
    Ok(history[0].get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

#[tauri::command]
async fn clear_url_history() -> Result<(), String> {
    use std::fs;
    
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let history_path = home_dir.join(".m3u8-mcp").join("url_history.json");
    
    if history_path.exists() {
        fs::write(history_path, "[]")
            .map_err(|e| format!("Failed to clear history: {}", e))?;
    }
    
    Ok(())
}

// Configuration management
#[tauri::command]
async fn save_m3u8_config(
    ffmpeg_path: Option<String>,
    output_dir: String
) -> Result<(), String> {
    use std::fs;
    
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let config_dir = home_dir.join(".m3u8-mcp");
    
    fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;
    
    let config_path = config_dir.join("config.json");
    let config = serde_json::json!({
        "ffmpeg_path": ffmpeg_path,
        "output_dir": output_dir
    });
    
    fs::write(config_path, config.to_string())
        .map_err(|e| format!("Failed to save configuration: {}", e))?;
    
    Ok(())
}

#[tauri::command]
async fn load_m3u8_config() -> Result<serde_json::Value, String> {
    use std::fs;
    
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let config_dir = home_dir.join(".m3u8-mcp");
    let config_path = config_dir.join("config.json");
    
    if !config_path.exists() {
        return Ok(serde_json::json!({
            "ffmpeg_path": null,
            "output_dir": home_dir.join("Downloads").join("m3u8-mcp").to_string_lossy()
        }));
    }
    
    let config_str = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read configuration: {}", e))?;
    
    serde_json::from_str(&config_str)
        .map_err(|e| format!("Failed to parse configuration: {}", e))
}

// MCP Server commands (unchanged)
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

// Database commands
#[tauri::command]
async fn init_database(db_state: State<'_, Arc<Mutex<DatabaseHandle>>>) -> Result<String, String> {
    let db_handle = db_state.lock().await;
    
    // Get app data directory using home directory
    let home_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?;
    let db_dir = home_dir.join(".m3u8-mcp");
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
    
    // Initialize m3u8 parser
    let parser_handle = M3u8ParserHandle {
        parser: Arc::new(m3u8_parser::M3u8Parser::new()),
    };
    
    // Initialize FFmpeg wrapper with default config
    let ffmpeg_config = ffmpeg_wrapper::FFmpegConfig::default();
    let ffmpeg_handle = Arc::new(Mutex::new(FFmpegHandle {
        wrapper: Arc::new(Mutex::new(ffmpeg_wrapper::FFmpegWrapper::new(ffmpeg_config))),
    }));
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(server_handle)
        .manage(database_handle)
        .manage(parser_handle)
        .manage(ffmpeg_handle)
        .invoke_handler(tauri::generate_handler![
            greet,
            // MCP Server
            start_mcp_server,
            stop_mcp_server,
            get_mcp_server_status,
            check_port_availability,
            // m3u8 URL management
            set_current_m3u8_url,
            get_current_m3u8_url,
            get_last_used_url,
            get_url_history,
            clear_url_history,
            // m3u8 operations
            parse_m3u8_url,
            extract_m3u8_segments,
            check_ffmpeg_installation,
            download_m3u8_stream,
            cancel_download,
            convert_to_hls,
            probe_stream,
            // Configuration
            save_m3u8_config,
            load_m3u8_config,
            // Database
            init_database,
            get_cache_stats,
            clear_cache
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}