use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, sse::{Event, Sse}},
    routing::post,
    Json, Router,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// Import mcp-schema for type validation
// We use custom types for simpler implementation but can validate against mcp-schema types
#[allow(unused_imports)]
use mcp_schema;
use std::{
    collections::HashMap,
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
}

impl McpServerState {
    pub fn new(port: u16) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            port,
            running: Arc::new(Mutex::new(false)),
        }
    }
}

// Create MCP router
pub fn create_mcp_router(state: Arc<McpServerState>) -> Router {
    Router::new()
        .route("/mcp", 
            post(handle_post)
            .get(handle_get)
            .delete(handle_delete)
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}





// Handle POST /mcp
async fn handle_post(
    State(state): State<Arc<McpServerState>>,
    _headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    // Parse JSON body
    let body: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid JSON" })),
            )
                .into_response();
        }
    };


    // For streamable HTTP, we don't require session ID
    let session_id = None;

    // Handle single or batch requests
    if body.is_array() {
        handle_batch_request(state, session_id, body.as_array().unwrap().clone()).await
    } else {
        let request: JsonRpcRequest = match serde_json::from_value(body) {
            Ok(req) => req,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "Invalid JSON-RPC request" })),
                )
                    .into_response();
            }
        };

        handle_single_request(state, session_id, request).await
    }
}

// Handle single JSON-RPC request
async fn handle_single_request(
    state: Arc<McpServerState>,
    _session_id: Option<String>,
    request: JsonRpcRequest,
) -> Response {
    let method = request.method.as_str();

    // Handle initialize method
    if method == "initialize" {
        let (_session_id, response) = handle_initialize(state.clone(), request).await;
        
        return (StatusCode::OK, Json(response)).into_response();
    }

    // For streamable HTTP, we use a default session
    let session_id = "default".to_string();
    
    // Ensure default session exists
    {
        let mut sessions = state.sessions.write().await;
        if !sessions.contains_key(&session_id) {
            let session = Session {
                id: session_id.clone(),
                initialized: true,
                created_at: SystemTime::now(),
                last_activity: SystemTime::now(),
                last_event_id: 0,
                tools: vec![
                    Tool {
                        name: "browser_navigate".to_string(),
                        description: Some("Open a URL in the default browser".to_string()),
                        input_schema: json!({
                            "type": "object",
                            "properties": {
                                "url": {
                                    "type": "string",
                                    "description": "The URL to open in the browser"
                                }
                            },
                            "required": ["url"]
                        }),
                    },
                ],
                resources: vec![],
            };
            sessions.insert(session_id.clone(), session);
        }
    }

    // Process request based on method
    let request_id = request.id.clone();
    let response = match method {
        "initialize" => {
            let (new_session_id, response) = handle_initialize(state.clone(), request).await;
            // Update session_id if it changed
            response
        },
        "initialized" => handle_initialized(state.clone(), session_id.clone(), request_id.clone()).await,
        "tools/list" => handle_tools_list(state.clone(), session_id.clone(), request_id.clone()).await,
        "resources/list" => handle_resources_list(state.clone(), session_id.clone(), request_id.clone()).await,
        "tools/call" => handle_tools_call(state.clone(), session_id.clone(), request).await,
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request_id.clone(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }),
        },
    };

    // Return response
    if request_id.is_some() {
        (StatusCode::OK, Json(response)).into_response()
    } else {
        StatusCode::ACCEPTED.into_response()
    }
}

// Handle batch request
async fn handle_batch_request(
    _state: Arc<McpServerState>,
    _session_id: Option<String>,
    requests: Vec<Value>,
) -> Response {
    let mut has_requests = false;

    for req_value in requests {
        if let Ok(request) = serde_json::from_value::<JsonRpcRequest>(req_value) {
            if request.id.is_some() {
                has_requests = true;
            }
            // Note: In production, batch processing would be more sophisticated
            // Here we're simplifying for the initial implementation
        }
    }

    if has_requests {
        // Return SSE stream for batch responses
        StatusCode::ACCEPTED.into_response()
    } else {
        StatusCode::ACCEPTED.into_response()
    }
}

// Handle GET /mcp (SSE stream)
async fn handle_get(
    State(state): State<Arc<McpServerState>>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    // For streamable HTTP, use default session
    let session_id = "default".to_string();

    // Create SSE stream
    let stream = create_sse_stream(state, session_id);
    Sse::new(stream).into_response()
}

// Create SSE stream
fn create_sse_stream(
    _state: Arc<McpServerState>,
    _session_id: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    stream::unfold(
        0u64,
        |counter| async move {
            // Send heartbeat every 30 seconds
            tokio::time::sleep(Duration::from_secs(30)).await;
            
            let event = Event::default()
                .event("ping")
                .data("{}");
            
            Some((Ok(event), counter + 1))
        },
    )
}

// Handle DELETE /mcp
async fn handle_delete(
    State(_state): State<Arc<McpServerState>>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    // For streamable HTTP without auth, just return OK
    StatusCode::OK
}

// Handle initialize method
async fn handle_initialize(
    state: Arc<McpServerState>,
    request: JsonRpcRequest,
) -> (String, JsonRpcResponse) {
    // For streamable HTTP, use a default session
    let session_id = "default".to_string();
    
    let session = Session {
        id: session_id.clone(),
        initialized: true,
        created_at: SystemTime::now(),
        last_activity: SystemTime::now(),
        last_event_id: 0,
        tools: vec![
            Tool {
                name: "browser_navigate".to_string(),
                description: Some("Open a URL in the default browser".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to open in the browser"
                        }
                    },
                    "required": ["url"]
                }),
            },
        ],
        resources: vec![],
    };

    let mut sessions = state.sessions.write().await;
    sessions.insert(session_id.clone(), session);

    let response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id,
        result: Some(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "serverInfo": {
                "name": "browser-automation-mcp",
                "version": "0.1.0"
            }
        })),
        error: None,
    };

    (session_id, response)
}

// Handle initialized notification
async fn handle_initialized(
    state: Arc<McpServerState>,
    session_id: String,
    request_id: Option<Value>,
) -> JsonRpcResponse {
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(&session_id) {
        session.last_activity = SystemTime::now();
    }

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({})),
        error: None,
    }
}

// Handle tools/list method
async fn handle_tools_list(
    state: Arc<McpServerState>,
    session_id: String,
    request_id: Option<Value>,
) -> JsonRpcResponse {
    let sessions = state.sessions.read().await;
    let tools = sessions
        .get(&session_id)
        .map(|s| s.tools.clone())
        .unwrap_or_default();

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({
            "tools": tools
        })),
        error: None,
    }
}

// Handle resources/list method
async fn handle_resources_list(
    state: Arc<McpServerState>,
    session_id: String,
    request_id: Option<Value>,
) -> JsonRpcResponse {
    let sessions = state.sessions.read().await;
    let resources = sessions
        .get(&session_id)
        .map(|s| s.resources.clone())
        .unwrap_or_default();

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request_id,
        result: Some(json!({
            "resources": resources
        })),
        error: None,
    }
}

// Handle tools/call method
async fn handle_tools_call(
    _state: Arc<McpServerState>,
    _session_id: String,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    // Parse the tool call parameters
    let params = request.params.unwrap_or(json!({}));
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let empty_args = json!({});
    let arguments = params.get("arguments").unwrap_or(&empty_args);
    
    let result = match tool_name {
        "browser_navigate" => {
            let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Error: URL is required"
                    }]
                })
            } else {
                // Simple browser open using OS command
                let browser_result = crate::simple_browser::open_browser(url);
                let success = browser_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let message = browser_result.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error").to_string();
                
                let text = if success {
                    format!("✅ {}", message)
                } else {
                    format!("❌ {}", message)
                };
                
                json!({
                    "content": [{
                        "type": "text",
                        "text": text
                    }]
                })
            }
        }
        _ => {
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("Tool '{}' is not yet implemented in MVP", tool_name)
                }]
            })
        }
    };
    
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id,
        result: Some(result),
        error: None,
    }
}



// Start MCP server
pub async fn start_mcp_server(state: Arc<McpServerState>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let router = create_mcp_router(state.clone());
    
    let addr = format!("127.0.0.1:{}", state.port);
    
    // Try to bind to the port, this will fail if port is already in use
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            println!("MCP Server successfully started on http://{}", addr);
            listener
        },
        Err(e) => {
            eprintln!("Failed to bind to port {}: {}", state.port, e);
            *state.running.lock().await = false;
            return Err(Box::new(e));
        }
    };
    
    *state.running.lock().await = true;
    
    // This will block until the server is stopped
    let result = axum::serve(listener, router).await;
    
    // Mark server as stopped when serve() returns
    *state.running.lock().await = false;
    
    result.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}