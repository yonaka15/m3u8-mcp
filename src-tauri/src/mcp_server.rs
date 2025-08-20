use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, sse::{Event, Sse}},
    routing::post,
    Json, Router,
};
use futures::stream::{self};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// Import mcp-schema for type validation
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::CorsLayer;

// MCP Protocol Version
const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

// Session data structure
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub initialized: bool,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub last_event_id: u64,
    pub tools: Vec<Tool>,
    pub resources: Vec<Resource>,
}

// Tool definition - matches MCP schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

// Resource definition - matches MCP schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

// JSON-RPC Request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

// JSON-RPC Response
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

// JSON-RPC Error
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// Server state
pub struct McpServerState {
    pub sessions: Arc<RwLock<HashMap<String, Session>>>,
    pub port: u16,
    pub running: Arc<Mutex<bool>>,
    pub enabled_tools: Arc<RwLock<Vec<String>>>,
}

impl McpServerState {
    pub fn new(port: u16) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            port,
            running: Arc::new(Mutex::new(false)),
            enabled_tools: Arc::new(RwLock::new(vec![
                "m3u8_parse".to_string(),
                "m3u8_download".to_string(),
                "m3u8_convert".to_string(),
                "m3u8_probe".to_string(),
                "m3u8_extract_segments".to_string(),
            ])),
        }
    }

    pub fn new_with_tools(port: u16, tools: Vec<String>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            port,
            running: Arc::new(Mutex::new(false)),
            enabled_tools: Arc::new(RwLock::new(tools)),
        }
    }
}

// Generate session ID
fn generate_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

// Start MCP server
pub async fn start_mcp_server(state: Arc<McpServerState>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = format!("0.0.0.0:{}", state.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    println!("MCP Server starting on {}", addr);
    *state.running.lock().await = true;
    
    let app = Router::new()
        .route("/mcp", post(handle_sse_endpoint))
        .route("/sse", post(handle_sse_endpoint))  // Keep for backward compatibility
        .layer(CorsLayer::permissive())
        .with_state(state.clone());
    
    axum::serve(listener, app)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    
    Ok(())
}

// SSE endpoint handler - handles the MCP protocol over SSE
async fn handle_sse_endpoint(
    State(state): State<Arc<McpServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Parse the incoming JSON-RPC request
    let request: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            let error_response = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {}", e),
                    data: None,
                }),
            };
            return Json(error_response).into_response();
        }
    };

    // Handle the request
    let response = handle_jsonrpc_request(state, request).await;
    
    // Return as JSON response for now
    // Full SSE implementation would stream responses
    Json(response).into_response()
}

// Handle JSON-RPC request
async fn handle_jsonrpc_request(
    state: Arc<McpServerState>,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    match request.method.as_str() {
        "initialize" => handle_initialize(state, request.id, request.params).await,
        "initialized" => handle_initialized(state, request.id).await,
        "tools/list" => handle_tools_list(state, request.id).await,
        "tools/call" => handle_tools_call(state, request.id, request.params).await,
        "resources/list" => handle_resources_list(state, request.id).await,
        "resources/read" => handle_resources_read(state, request.id, request.params).await,
        "ping" => handle_ping(request.id).await,
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        },
    }
}

// Handle initialize request
async fn handle_initialize(
    state: Arc<McpServerState>,
    request_id: Option<Value>,
    _params: Option<Value>,
) -> JsonRpcResponse {
    let session_id = generate_session_id();
    let enabled_tools = state.enabled_tools.read().await;
    let tools = get_available_tools(&enabled_tools);
    let resources = get_available_resources();
    
    let session = Session {
        id: session_id.clone(),
        initialized: false,
        created_at: SystemTime::now(),
        last_activity: SystemTime::now(),
        last_event_id: 0,
        tools: tools.clone(),
        resources: resources.clone(),
    };
    
    let mut sessions = state.sessions.write().await;
    sessions.insert(session_id.clone(), session);
    
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {},
                "logging": {}
            },
            "serverInfo": {
                "name": "m3u8-mcp",
                "version": "0.1.0"
            },
            "sessionId": session_id
        })),
        error: None,
    }
}

// Handle initialized notification
async fn handle_initialized(
    state: Arc<McpServerState>,
    request_id: Option<Value>,
) -> JsonRpcResponse {
    // Mark all sessions as initialized
    let mut sessions = state.sessions.write().await;
    for session in sessions.values_mut() {
        session.initialized = true;
    }
    
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({})),
        error: None,
    }
}

// Get available tools based on enabled list
fn get_available_tools(enabled_tools: &[String]) -> Vec<Tool> {
    let enabled_tools_set: HashSet<_> = enabled_tools.iter().cloned().collect();
    
    // Define all available tools
    let all_tools = vec![
        // m3u8 URL management
        Tool {
            name: "m3u8_set_url".to_string(),
            description: Some("Set the current m3u8 URL in the UI".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL of the m3u8 stream to set in the UI"
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "m3u8_get_url".to_string(),
            description: Some("Get the current m3u8 URL from the UI".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        // m3u8 parsing and analysis
        Tool {
            name: "m3u8_parse".to_string(),
            description: Some("Parse an m3u8 playlist from URL or content".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL of the m3u8 playlist"
                    },
                    "content": {
                        "type": "string",
                        "description": "Raw m3u8 content (if URL not provided)"
                    }
                }
            }),
        },
        Tool {
            name: "m3u8_download".to_string(),
            description: Some("Download m3u8 stream using FFmpeg".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL of the m3u8 stream"
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Output file path (required)"
                    },
                    "format": {
                        "type": "string",
                        "description": "Output format (mp4, mkv, ts)",
                        "default": "mp4"
                    }
                },
                "required": ["url", "output_path"]
            }),
        },
        Tool {
            name: "m3u8_convert".to_string(),
            description: Some("Convert video to HLS format".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "input_path": {
                        "type": "string",
                        "description": "Input video file path"
                    },
                    "output_dir": {
                        "type": "string",
                        "description": "Output directory for HLS files"
                    },
                    "segment_duration": {
                        "type": "number",
                        "description": "Segment duration in seconds",
                        "default": 10
                    },
                    "playlist_type": {
                        "type": "string",
                        "description": "Playlist type (vod or event)",
                        "default": "vod"
                    }
                },
                "required": ["input_path", "output_dir"]
            }),
        },
        Tool {
            name: "m3u8_probe".to_string(),
            description: Some("Probe m3u8 stream for information".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL of the m3u8 stream"
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "m3u8_extract_segments".to_string(),
            description: Some("Extract segment URLs from m3u8 playlist".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL of the m3u8 playlist"
                    },
                    "base_url": {
                        "type": "string",
                        "description": "Base URL for relative segment URLs"
                    }
                }
            }),
        },
    ];
    
    // Filter tools based on enabled list
    all_tools.into_iter()
        .filter(|tool| enabled_tools_set.contains(&tool.name))
        .collect()
}

// Get available resources
fn get_available_resources() -> Vec<Resource> {
    vec![
        Resource {
            uri: "m3u8://config".to_string(),
            name: "Configuration".to_string(),
            description: Some("m3u8 MCP server configuration".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        Resource {
            uri: "m3u8://cache/stats".to_string(),
            name: "Cache Statistics".to_string(),
            description: Some("Statistics about cached m3u8 data".to_string()),
            mime_type: Some("application/json".to_string()),
        },
    ]
}

// Handle tools/list request
async fn handle_tools_list(
    state: Arc<McpServerState>,
    request_id: Option<Value>,
) -> JsonRpcResponse {
    let enabled_tools = state.enabled_tools.read().await;
    let tools = get_available_tools(&enabled_tools);
    
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({
            "tools": tools
        })),
        error: None,
    }
}

// Handle resources/list request
async fn handle_resources_list(
    _state: Arc<McpServerState>,
    request_id: Option<Value>,
) -> JsonRpcResponse {
    let resources = get_available_resources();
    
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({
            "resources": resources
        })),
        error: None,
    }
}

// Handle resources/read request
async fn handle_resources_read(
    _state: Arc<McpServerState>,
    request_id: Option<Value>,
    params: Option<Value>,
) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            };
        }
    };
    
    let uri = match params.get("uri").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Missing required parameter: uri".to_string(),
                    data: None,
                }),
            };
        }
    };
    
    let result = match uri {
        "m3u8://config" => {
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": json!({
                        "ffmpeg_path": "ffmpeg",
                        "output_dir": "~/Downloads/m3u8-mcp",
                        "cache_enabled": true
                    }).to_string()
                }]
            })
        }
        "m3u8://cache/stats" => {
            // Get cache stats from database
            let db_guard = crate::database::GLOBAL_DB.read().await;
            if let Some(ref db) = *db_guard {
                match db.get_cache_stats() {
                    Ok(stats) => {
                        json!({
                            "contents": [{
                                "uri": uri,
                                "mimeType": "application/json",
                                "text": stats.to_string()
                            }]
                        })
                    }
                    Err(e) => {
                        return JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request_id,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32603,
                                message: format!("Failed to get cache stats: {}", e),
                                data: None,
                            }),
                        };
                    }
                }
            } else {
                json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": json!({
                            "error": "Database not initialized"
                        }).to_string()
                    }]
                })
            }
        }
        _ => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: format!("Unknown resource URI: {}", uri),
                    data: None,
                }),
            };
        }
    };
    
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(result),
        error: None,
    }
}

// Handle ping request
async fn handle_ping(request_id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({})),
        error: None,
    }
}

// Handle tools/call request
async fn handle_tools_call(
    _state: Arc<McpServerState>,
    request_id: Option<Value>,
    params: Option<Value>,
) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            };
        }
    };
    
    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Missing required parameter: name".to_string(),
                    data: None,
                }),
            };
        }
    };
    
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    
    // Execute tool based on name
    let result = match tool_name {
        "m3u8_set_url" => {
            let url = match arguments.get("url").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing required parameter: url".to_string(),
                            data: None,
                        }),
                    };
                }
            };
            
            // Store the URL in a global state
            let mut url_state = crate::CURRENT_M3U8_URL.write().await;
            *url_state = Some(url.to_string());
            
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("URL set to: {}", url)
                }]
            })
        }
        "m3u8_get_url" => {
            let url_state = crate::CURRENT_M3U8_URL.read().await;
            let url = url_state.as_ref().map(|s| s.as_str()).unwrap_or("No URL set");
            
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("Current URL: {}", url)
                }]
            })
        }
        "m3u8_parse" => {
            let url = arguments.get("url").and_then(|v| v.as_str());
            let content = arguments.get("content").and_then(|v| v.as_str());
            
            if url.is_none() && content.is_none() {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request_id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Either 'url' or 'content' parameter is required".to_string(),
                        data: None,
                    }),
                };
            }
            
            // Parse m3u8 using the parser module
            if let Some(url) = url {
                let parser = crate::m3u8_parser::M3u8Parser::new();
                match parser.parse_url(url).await {
                    Ok(playlist) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&playlist).unwrap_or_else(|_| "Failed to serialize".to_string())
                        }]
                    }),
                    Err(e) => {
                        return JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request_id,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32603,
                                message: format!("Failed to parse m3u8: {}", e),
                                data: None,
                            }),
                        };
                    }
                }
            } else {
                // Parse from content
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Parsing from content not yet implemented"
                    }]
                })
            }
        }
        "m3u8_download" => {
            let url = match arguments.get("url").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing required parameter: url".to_string(),
                            data: None,
                        }),
                    };
                }
            };
            
            let output_path = match arguments.get("output_path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing required parameter: output_path".to_string(),
                            data: None,
                        }),
                    };
                }
            };
            
            // Use FFmpeg wrapper to download
            let config = crate::ffmpeg_wrapper::FFmpegConfig::default();
            let wrapper = crate::ffmpeg_wrapper::FFmpegWrapper::new(config);
            
            let output = Some(std::path::Path::new(output_path));
            
            match wrapper.download_stream(url, output).await {
                Ok(path) => json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Downloaded to: {}", path.display())
                    }]
                }),
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Failed to download m3u8: {}", e),
                            data: None,
                        }),
                    };
                }
            }
        }
        "m3u8_probe" => {
            let url = match arguments.get("url").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing required parameter: url".to_string(),
                            data: None,
                        }),
                    };
                }
            };
            
            // Use FFmpeg wrapper to probe
            let config = crate::ffmpeg_wrapper::FFmpegConfig::default();
            let wrapper = crate::ffmpeg_wrapper::FFmpegWrapper::new(config);
            
            match wrapper.probe_stream(url).await {
                Ok(info) => json!({
                    "content": [{
                        "type": "text",
                        "text": info
                    }]
                }),
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Failed to probe stream: {}", e),
                            data: None,
                        }),
                    };
                }
            }
        }
        "m3u8_extract_segments" => {
            let url = match arguments.get("url").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing 'url' parameter".to_string(),
                            data: None,
                        }),
                    };
                }
            };
            
            let base_url = arguments.get("base_url").and_then(|v| v.as_str());
            
            // Use m3u8 parser to extract segments
            let parser = crate::m3u8_parser::M3u8Parser::new();
            
            match parser.extract_segments(url, base_url).await {
                Ok(segments) => json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&segments).unwrap_or_else(|_| "[]".to_string())
                    }]
                }),
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Failed to extract segments: {}", e),
                            data: None,
                        }),
                    };
                }
            }
        }
        _ => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Unknown tool: {}", tool_name),
                    data: None,
                }),
            };
        }
    };
    
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({
            "content": result["content"],
            "isError": false
        })),
        error: None,
    }
}