use axum::{
    body::Bytes,
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
#[axum::debug_handler]
async fn handle_post(
    State(state): State<Arc<McpServerState>>,
    body: Bytes,
) -> Response {
    // Convert body to string
    let body_str = String::from_utf8_lossy(&body);
    // Parse JSON body
    let body_value: Value = match serde_json::from_str(&body_str) {
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
    if body_value.is_array() {
        handle_batch_request(state, session_id, body_value.as_array().unwrap().clone()).await
    } else {
        let request: JsonRpcRequest = match serde_json::from_value(body_value) {
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
                tools: vec![],
                resources: vec![],
            };
            sessions.insert(session_id.clone(), session);
        }
    }

    // Process request based on method
    let request_id = request.id.clone();
    let response = match method {
        "initialize" => {
            let (_new_session_id, response) = handle_initialize(state.clone(), request).await;
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
                name: "browser_open".to_string(),
                description: Some("Open a new browser instance".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "headless": {
                            "type": "boolean",
                            "description": "Run in headless mode (no visible window). Default: false",
                            "default": false
                        }
                    }
                }),
            },
            Tool {
                name: "browser_navigate".to_string(),
                description: Some("Navigate to a URL in the browser (requires browser_open first)".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to navigate to"
                        }
                    },
                    "required": ["url"]
                }),
            },
            Tool {
                name: "browser_click".to_string(),
                description: Some("Click an element on the page".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "string",
                            "description": "CSS selector for the element to click"
                        }
                    },
                    "required": ["selector"]
                }),
            },
            Tool {
                name: "browser_type".to_string(),
                description: Some("Type text into an input field".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "string",
                            "description": "CSS selector for the input field"
                        },
                        "text": {
                            "type": "string",
                            "description": "Text to type"
                        }
                    },
                    "required": ["selector", "text"]
                }),
            },
            Tool {
                name: "browser_screenshot".to_string(),
                description: Some("Take a screenshot of the current page".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "full_page": {
                            "type": "boolean",
                            "description": "Whether to capture the full page",
                            "default": false
                        }
                    }
                }),
            },
            Tool {
                name: "browser_evaluate".to_string(),
                description: Some("Execute JavaScript in the browser".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "script": {
                            "type": "string",
                            "description": "JavaScript code to execute"
                        }
                    },
                    "required": ["script"]
                }),
            },
            Tool {
                name: "browser_wait_for".to_string(),
                description: Some("Wait for an element to appear".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "string",
                            "description": "CSS selector to wait for"
                        },
                        "timeout": {
                            "type": "number",
                            "description": "Timeout in milliseconds",
                            "default": 30000
                        }
                    },
                    "required": ["selector"]
                }),
            },
            Tool {
                name: "browser_get_content".to_string(),
                description: Some("Get the HTML content of the current page".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "browser_go_back".to_string(),
                description: Some("Navigate back in browser history".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "browser_go_forward".to_string(),
                description: Some("Navigate forward in browser history".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "browser_reload".to_string(),
                description: Some("Reload the current page".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "browser_close".to_string(),
                description: Some("Close the browser".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "browser_snapshot".to_string(),
                description: Some("Get a snapshot of the current page state".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "browser_tab_list".to_string(),
                description: Some("List all open browser tabs".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            Tool {
                name: "browser_tab_new".to_string(),
                description: Some("Open a new browser tab".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to navigate to in the new tab. If not provided, the new tab will be blank."
                        }
                    }
                }),
            },
            Tool {
                name: "browser_tab_switch".to_string(),
                description: Some("Switch to a different browser tab".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "index": {
                            "type": "number",
                            "description": "Tab index to switch to"
                        }
                    },
                    "required": ["index"]
                }),
            },
            Tool {
                name: "browser_tab_close".to_string(),
                description: Some("Close a specific browser tab".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "index": {
                            "type": "number",
                            "description": "Tab index to close"
                        }
                    },
                    "required": ["index"]
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
        "browser_open" => {
            let headless = arguments.get("headless").and_then(|v| v.as_bool()).unwrap_or(false);
            let result = crate::cdp_browser::handle_browser_open(headless).await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("Browser opened successfully")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to open browser")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
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
                let result = crate::cdp_browser::handle_browser_navigate(url).await;
                let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let message = if success {
                    result.get("message").and_then(|v| v.as_str()).unwrap_or("Navigated successfully")
                } else {
                    result.get("error").and_then(|v| v.as_str()).unwrap_or("Navigation failed")
                };
                
                json!({
                    "content": [{
                        "type": "text",
                        "text": message
                    }]
                })
            }
        }
        "browser_click" => {
            let selector = arguments.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            if selector.is_empty() {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Error: Selector is required"
                    }]
                })
            } else {
                let result = crate::cdp_browser::handle_browser_click(selector).await;
                let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let message = if success {
                    result.get("message").and_then(|v| v.as_str()).unwrap_or("Clicked successfully")
                } else {
                    result.get("error").and_then(|v| v.as_str()).unwrap_or("Click failed")
                };
                
                json!({
                    "content": [{
                        "type": "text",
                        "text": message
                    }]
                })
            }
        }
        "browser_type" => {
            let selector = arguments.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let text = arguments.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if selector.is_empty() || text.is_empty() {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Error: Both selector and text are required"
                    }]
                })
            } else {
                let result = crate::cdp_browser::handle_browser_type(selector, text).await;
                let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let message = if success {
                    result.get("message").and_then(|v| v.as_str()).unwrap_or("Typed successfully")
                } else {
                    result.get("error").and_then(|v| v.as_str()).unwrap_or("Type failed")
                };
                
                json!({
                    "content": [{
                        "type": "text",
                        "text": message
                    }]
                })
            }
        }
        "browser_screenshot" => {
            let full_page = arguments.get("full_page").and_then(|v| v.as_bool()).unwrap_or(false);
            let result = crate::cdp_browser::handle_browser_screenshot(full_page).await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            
            if success {
                if let Some(screenshot) = result.get("screenshot").and_then(|v| v.as_str()) {
                    // Extract base64 data from data URL and determine mime type
                    let (base64_data, mime_type) = if screenshot.starts_with("data:image/jpeg;base64,") {
                        (&screenshot[23..], "image/jpeg")
                    } else if screenshot.starts_with("data:image/png;base64,") {
                        (&screenshot[22..], "image/png")
                    } else {
                        // Assume it's already raw base64 data (JPEG from our processor)
                        (screenshot, "image/jpeg")
                    };
                    
                    json!({
                        "content": [{
                            "type": "image",
                            "data": base64_data,
                            "mimeType": mime_type
                        }]
                    })
                } else {
                    json!({
                        "content": [{
                            "type": "text",
                            "text": "Screenshot captured but data not available"
                        }]
                    })
                }
            } else {
                let error = result.get("error").and_then(|v| v.as_str()).unwrap_or("Screenshot failed");
                json!({
                    "content": [{
                        "type": "text",
                        "text": error
                    }]
                })
            }
        }
        "browser_evaluate" => {
            let script = arguments.get("script").and_then(|v| v.as_str()).unwrap_or("");
            if script.is_empty() {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Error: Script is required"
                    }]
                })
            } else {
                let result = crate::cdp_browser::handle_browser_evaluate(script).await;
                let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                
                if success {
                    let value = result.get("result")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "undefined".to_string());
                    json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Result: {}", value)
                        }]
                    })
                } else {
                    let error = result.get("error").and_then(|v| v.as_str()).unwrap_or("Evaluation failed");
                    json!({
                        "content": [{
                            "type": "text",
                            "text": error
                        }]
                    })
                }
            }
        }
        "browser_wait_for" => {
            let selector = arguments.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let timeout = arguments.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30000);
            if selector.is_empty() {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Error: Selector is required"
                    }]
                })
            } else {
                let result = crate::cdp_browser::handle_browser_wait_for(selector, timeout).await;
                let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let message = if success {
                    result.get("message").and_then(|v| v.as_str()).unwrap_or("Element found")
                } else {
                    result.get("error").and_then(|v| v.as_str()).unwrap_or("Element not found")
                };
                
                json!({
                    "content": [{
                        "type": "text",
                        "text": message
                    }]
                })
            }
        }
        "browser_get_content" => {
            let result = crate::cdp_browser::handle_browser_get_content().await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            
            if success {
                let content = result.get("content").and_then(|v| v.as_str()).unwrap_or("No content available");
                json!({
                    "content": [{
                        "type": "text",
                        "text": content
                    }]
                })
            } else {
                let error = result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to get content");
                json!({
                    "content": [{
                        "type": "text",
                        "text": error
                    }]
                })
            }
        }
        "browser_go_back" => {
            let result = crate::cdp_browser::handle_browser_go_back().await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("Navigated back")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to go back")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
        "browser_go_forward" => {
            let result = crate::cdp_browser::handle_browser_go_forward().await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("Navigated forward")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to go forward")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
        "browser_reload" => {
            let result = crate::cdp_browser::handle_browser_reload().await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("Page reloaded")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to reload")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
        "browser_close" => {
            let result = crate::cdp_browser::handle_browser_close().await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("Browser closed")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to close browser")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
        "browser_snapshot" => {
            let result = crate::cdp_browser::handle_browser_snapshot().await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            
            if success {
                if let Some(snapshot) = result.get("snapshot") {
                    let formatted = format!(
                        "### Page Snapshot\n\n**URL**: {}\n**Title**: {}\n\n**Tabs**:\n{}\n\n**Recent Console Messages**:\n{}",
                        snapshot.get("url").and_then(|v| v.as_str()).unwrap_or("N/A"),
                        snapshot.get("title").and_then(|v| v.as_str()).unwrap_or("N/A"),
                        snapshot.get("tabs").and_then(|v| v.as_array())
                            .map(|tabs| tabs.iter()
                                .map(|tab| format!("- [{}] {} - {} {}",
                                    tab.get("index").and_then(|v| v.as_u64()).unwrap_or(0),
                                    tab.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                                    tab.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                                    if tab.get("current").and_then(|v| v.as_bool()).unwrap_or(false) { "(current)" } else { "" }
                                ))
                                .collect::<Vec<_>>()
                                .join("\n"))
                            .unwrap_or_else(|| "None".to_string()),
                        snapshot.get("console_messages").and_then(|v| v.as_array())
                            .map(|msgs| if msgs.is_empty() {
                                "None".to_string()
                            } else {
                                msgs.iter()
                                    .map(|msg| format!("- [{}] {}",
                                        msg.get("level").and_then(|v| v.as_str()).unwrap_or(""),
                                        msg.get("text").and_then(|v| v.as_str()).unwrap_or("")
                                    ))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .unwrap_or_else(|| "None".to_string())
                    );
                    
                    json!({
                        "content": [{
                            "type": "text",
                            "text": formatted
                        }]
                    })
                } else {
                    json!({
                        "content": [{
                            "type": "text",
                            "text": "Snapshot captured but no data available"
                        }]
                    })
                }
            } else {
                let error = result.get("error").and_then(|v| v.as_str()).unwrap_or("Snapshot failed");
                json!({
                    "content": [{
                        "type": "text",
                        "text": error
                    }]
                })
            }
        }
        "browser_tab_list" => {
            let result = crate::cdp_browser::handle_browser_tab_list().await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            
            if success {
                if let Some(tabs) = result.get("tabs").and_then(|v| v.as_array()) {
                    let formatted = if tabs.is_empty() {
                        "No open tabs".to_string()
                    } else {
                        let tab_list = tabs.iter()
                            .map(|tab| format!("[{}] {} - {} {}",
                                tab.get("index").and_then(|v| v.as_u64()).unwrap_or(0),
                                tab.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                                tab.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                                if tab.get("current").and_then(|v| v.as_bool()).unwrap_or(false) { "(current)" } else { "" }
                            ))
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!("Open tabs:\n{}", tab_list)
                    };
                    
                    json!({
                        "content": [{
                            "type": "text",
                            "text": formatted
                        }]
                    })
                } else {
                    json!({
                        "content": [{
                            "type": "text",
                            "text": "No tabs information available"
                        }]
                    })
                }
            } else {
                let error = result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to list tabs");
                json!({
                    "content": [{
                        "type": "text",
                        "text": error
                    }]
                })
            }
        }
        "browser_tab_switch" => {
            let index = arguments.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let result = crate::cdp_browser::handle_browser_tab_switch(index).await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("Tab switched")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to switch tab")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
        "browser_tab_new" => {
            let url = arguments.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());
            let result = crate::cdp_browser::handle_browser_tab_new(url).await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("New tab created")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to create new tab")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
        "browser_tab_close" => {
            let index = arguments.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let result = crate::cdp_browser::handle_browser_tab_close(index).await;
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let message = if success {
                result.get("message").and_then(|v| v.as_str()).unwrap_or("Tab closed")
            } else {
                result.get("error").and_then(|v| v.as_str()).unwrap_or("Failed to close tab")
            };
            
            json!({
                "content": [{
                    "type": "text",
                    "text": message
                }]
            })
        }
        _ => {
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("Tool '{}' is not yet implemented", tool_name)
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