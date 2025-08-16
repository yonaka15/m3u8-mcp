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
// We use custom types for simpler implementation but can validate against mcp-schema types
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
            enabled_tools: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub fn new_with_tools(port: u16, enabled_tools: Vec<String>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            port,
            running: Arc::new(Mutex::new(false)),
            enabled_tools: Arc::new(RwLock::new(enabled_tools)),
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
    
    // Parse JSON-RPC request
    let request: JsonRpcRequest = match serde_json::from_str(&body_str) {
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

    // Route to appropriate handler based on method
    let response = match request.method.as_str() {
        "initialize" => {
            let (_session_id, response) = handle_initialize(state.clone(), request).await;
            // Store session for this connection
            response
        }
        "initialized" => handle_initialized(state.clone(), "default".to_string(), request.id).await,
        "tools/list" => handle_tools_list(state.clone(), "default".to_string(), request.id).await,
        "resources/list" => handle_resources_list(state.clone(), "default".to_string(), request.id).await,
        "tools/call" => handle_tool_call(state.clone(), "default".to_string(), request).await,
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
    };

    Json(response).into_response()
}

// Handle GET /mcp (SSE stream)
async fn handle_get(
    State(state): State<Arc<McpServerState>>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    // For streamable HTTP, use default session
    let session_id = "default".to_string();
    
    // Create or get session
    let mut sessions = state.sessions.write().await;
    if !sessions.contains_key(&session_id) {
        let session = Session {
            id: session_id.clone(),
            initialized: false,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            last_event_id: 0,
            tools: vec![],
            resources: vec![],
        };
        sessions.insert(session_id.clone(), session);
    }
    drop(sessions);

    // Create SSE stream
    let stream = stream::repeat_with(move || {
        Event::default()
            .data("ping")
            .event("ping")
    })
    .map(Ok::<_, Infallible>)
    .take(1); // Just send one ping for now

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("ping")
        )
}

// Handle DELETE /mcp (session cleanup)
async fn handle_delete(
    State(state): State<Arc<McpServerState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Get session ID from header
    let session_id = headers
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("default")
        .to_string();

    // Remove session
    let mut sessions = state.sessions.write().await;
    sessions.remove(&session_id);

    StatusCode::ACCEPTED.into_response()
}

// Handle initialize method
async fn handle_initialize(
    state: Arc<McpServerState>,
    request: JsonRpcRequest,
) -> (String, JsonRpcResponse) {
    // For streamable HTTP, use default session
    let session_id = "default".to_string();

    // Get enabled tools list
    let enabled_tools = state.enabled_tools.read().await;
    let enabled_tools_set: std::collections::HashSet<_> = enabled_tools.iter().cloned().collect();
    
    // Define all available tools
    let all_tools = vec![
            // Redmine configuration
            Tool {
                name: "redmine_configure".to_string(),
                description: Some("Configure Redmine connection".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "host": {
                            "type": "string",
                            "description": "Redmine server URL (e.g., https://redmine.example.com)"
                        },
                        "api_key": {
                            "type": "string",
                            "description": "Redmine API key"
                        }
                    },
                    "required": ["host", "api_key"]
                }),
            },
            Tool {
                name: "redmine_test_connection".to_string(),
                description: Some("Test Redmine connection".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            // Issue management
            Tool {
                name: "redmine_list_issues".to_string(),
                description: Some("List Redmine issues".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "project_id": {
                            "type": "string",
                            "description": "Project ID or identifier"
                        },
                        "assigned_to_id": {
                            "type": "string",
                            "description": "User ID of assignee"
                        },
                        "status_id": {
                            "type": "string",
                            "description": "Status ID"
                        },
                        "tracker_id": {
                            "type": "string",
                            "description": "Tracker ID"
                        },
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of issues to return",
                            "default": 25
                        },
                        "offset": {
                            "type": "number",
                            "description": "Offset for pagination",
                            "default": 0
                        }
                    }
                }),
            },
            Tool {
                name: "redmine_get_issue".to_string(),
                description: Some("Get a specific Redmine issue".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "number",
                            "description": "Issue ID"
                        }
                    },
                    "required": ["id"]
                }),
            },
            Tool {
                name: "redmine_create_issue".to_string(),
                description: Some("Create a new Redmine issue".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "project_id": {
                            "type": "string",
                            "description": "Project ID or identifier"
                        },
                        "subject": {
                            "type": "string",
                            "description": "Issue subject"
                        },
                        "description": {
                            "type": "string",
                            "description": "Issue description"
                        },
                        "tracker_id": {
                            "type": "number",
                            "description": "Tracker ID"
                        },
                        "status_id": {
                            "type": "number",
                            "description": "Status ID"
                        },
                        "priority_id": {
                            "type": "number",
                            "description": "Priority ID"
                        },
                        "assigned_to_id": {
                            "type": "number",
                            "description": "User ID of assignee"
                        },
                        "parent_issue_id": {
                            "type": "number",
                            "description": "Parent issue ID"
                        },
                        "start_date": {
                            "type": "string",
                            "description": "Start date (YYYY-MM-DD)"
                        },
                        "due_date": {
                            "type": "string",
                            "description": "Due date (YYYY-MM-DD)"
                        },
                        "estimated_hours": {
                            "type": "number",
                            "description": "Estimated hours"
                        }
                    },
                    "required": ["project_id", "subject"]
                }),
            },
            Tool {
                name: "redmine_update_issue".to_string(),
                description: Some("Update an existing Redmine issue".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "number",
                            "description": "Issue ID"
                        },
                        "subject": {
                            "type": "string",
                            "description": "Issue subject"
                        },
                        "description": {
                            "type": "string",
                            "description": "Issue description"
                        },
                        "status_id": {
                            "type": "number",
                            "description": "Status ID"
                        },
                        "priority_id": {
                            "type": "number",
                            "description": "Priority ID"
                        },
                        "assigned_to_id": {
                            "type": "number",
                            "description": "User ID of assignee"
                        },
                        "done_ratio": {
                            "type": "number",
                            "description": "Progress percentage (0-100)"
                        },
                        "notes": {
                            "type": "string",
                            "description": "Update notes/comment"
                        }
                    },
                    "required": ["id"]
                }),
            },
            Tool {
                name: "redmine_delete_issue".to_string(),
                description: Some("Delete a Redmine issue".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "issueNumber": {
                            "type": "number",
                            "description": "Issue ID"
                        }
                    },
                    "required": ["issueNumber"]
                }),
            },
            // Project management
            Tool {
                name: "redmine_list_projects".to_string(),
                description: Some("List Redmine projects".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of projects to return",
                            "default": 25
                        },
                        "offset": {
                            "type": "number",
                            "description": "Offset for pagination",
                            "default": 0
                        }
                    }
                }),
            },
            Tool {
                name: "redmine_get_project".to_string(),
                description: Some("Get a specific Redmine project".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Project ID or identifier"
                        }
                    },
                    "required": ["id"]
                }),
            },
            Tool {
                name: "redmine_create_project".to_string(),
                description: Some("Create a new Redmine project".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Project name"
                        },
                        "identifier": {
                            "type": "string",
                            "description": "Project identifier (unique)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Project description"
                        },
                        "parent_id": {
                            "type": "number",
                            "description": "Parent project ID"
                        },
                        "is_public": {
                            "type": "boolean",
                            "description": "Whether the project is public",
                            "default": false
                        }
                    },
                    "required": ["name", "identifier"]
                }),
            },
            // User management
            Tool {
                name: "redmine_list_users".to_string(),
                description: Some("List Redmine users".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "number",
                            "description": "User status (1=active, 2=registered, 3=locked)"
                        },
                        "name": {
                            "type": "string",
                            "description": "Filter by name"
                        },
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of users to return",
                            "default": 25
                        },
                        "offset": {
                            "type": "number",
                            "description": "Offset for pagination",
                            "default": 0
                        }
                    }
                }),
            },
            Tool {
                name: "redmine_get_current_user".to_string(),
                description: Some("Get current Redmine user".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            // Time tracking
            Tool {
                name: "redmine_list_time_entries".to_string(),
                description: Some("List time entries".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "issue_id": {
                            "type": "number",
                            "description": "Issue ID"
                        },
                        "project_id": {
                            "type": "string",
                            "description": "Project ID or identifier"
                        },
                        "user_id": {
                            "type": "number",
                            "description": "User ID"
                        },
                        "from": {
                            "type": "string",
                            "description": "From date (YYYY-MM-DD)"
                        },
                        "to": {
                            "type": "string",
                            "description": "To date (YYYY-MM-DD)"
                        },
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of entries to return",
                            "default": 25
                        }
                    }
                }),
            },
            Tool {
                name: "redmine_create_time_entry".to_string(),
                description: Some("Create a time entry".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "issue_id": {
                            "type": "number",
                            "description": "Issue ID"
                        },
                        "project_id": {
                            "type": "string",
                            "description": "Project ID (required if issue_id is not provided)"
                        },
                        "hours": {
                            "type": "number",
                            "description": "Time spent in hours"
                        },
                        "activity_id": {
                            "type": "number",
                            "description": "Activity ID"
                        },
                        "comments": {
                            "type": "string",
                            "description": "Comments for the time entry"
                        },
                        "spent_on": {
                            "type": "string",
                            "description": "Date the time was spent (YYYY-MM-DD)"
                        }
                    },
                    "required": ["hours"]
                }),
            },
        ];
    
    // Filter tools based on enabled list
    let filtered_tools: Vec<Tool> = if enabled_tools.is_empty() {
        // If no tools specified, include all tools
        all_tools
    } else {
        // Include only enabled tools
        all_tools.into_iter()
            .filter(|tool| enabled_tools_set.contains(&tool.name))
            .collect()
    };
    
    let session = Session {
        id: session_id.clone(),
        initialized: true,
        created_at: SystemTime::now(),
        last_activity: SystemTime::now(),
        last_event_id: 0,
        tools: filtered_tools,
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
                "name": "redmine-mcp",
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

// Handle tool execution
async fn handle_tool_call(
    _state: Arc<McpServerState>,
    _session_id: String,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let params = match request.params {
        Some(p) => p,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            };
        }
    };

    let tool_name = params["name"].as_str().unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    // Execute tool based on name
    let result = match tool_name {
        "redmine_configure" => {
            let host = arguments["host"].as_str().unwrap_or("");
            let api_key = arguments["api_key"].as_str().unwrap_or("");
            
            match crate::redmine_client::init_client(host.to_string(), api_key.to_string()).await {
                Ok(_) => json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Redmine configured successfully for {}", host)
                    }]
                }),
                Err(e) => json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Failed to configure Redmine: {}", e)
                    }]
                })
            }
        }
        "redmine_test_connection" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                match client.get_current_user().await {
                    Ok(user) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Connection successful. Current user: {}", 
                                user["user"]["login"].as_str().unwrap_or("unknown"))
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Connection test failed: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_list_issues" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let mut params = HashMap::new();
                if let Some(project_id) = arguments["project_id"].as_str() {
                    params.insert("project_id".to_string(), project_id.to_string());
                }
                if let Some(assigned_to_id) = arguments["assigned_to_id"].as_str() {
                    params.insert("assigned_to_id".to_string(), assigned_to_id.to_string());
                }
                if let Some(status_id) = arguments["status_id"].as_str() {
                    params.insert("status_id".to_string(), status_id.to_string());
                }
                if let Some(limit) = arguments["limit"].as_u64() {
                    params.insert("limit".to_string(), limit.to_string());
                }
                if let Some(offset) = arguments["offset"].as_u64() {
                    params.insert("offset".to_string(), offset.to_string());
                }
                
                match client.list_issues(params).await {
                    Ok(issues) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&issues).unwrap_or("Failed to format issues".to_string())
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to list issues: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_get_issue" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let id = arguments["id"].as_u64().unwrap_or(0) as u32;
                
                match client.get_issue(id).await {
                    Ok(issue) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&issue).unwrap_or("Failed to format issue".to_string())
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to get issue: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_create_issue" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                match client.create_issue(arguments).await {
                    Ok(issue) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Issue created successfully: {}", 
                                serde_json::to_string_pretty(&issue).unwrap_or("Failed to format issue".to_string()))
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to create issue: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_update_issue" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let id = arguments["id"].as_u64().unwrap_or(0) as u32;
                let mut update_data = arguments.clone();
                update_data.as_object_mut().map(|obj| obj.remove("id"));
                
                match client.update_issue(id, update_data).await {
                    Ok(_result) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Issue {} updated successfully", id)
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to update issue: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_delete_issue" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let id = arguments["issueNumber"].as_u64().unwrap_or(0) as u32;
                
                match client.delete_issue(id).await {
                    Ok(_) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Issue {} deleted successfully", id)
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to delete issue: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_list_projects" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let mut params = std::collections::HashMap::new();
                
                // Add optional parameters
                if let Some(limit) = arguments["limit"].as_u64() {
                    params.insert("limit".to_string(), limit.to_string());
                }
                if let Some(offset) = arguments["offset"].as_u64() {
                    params.insert("offset".to_string(), offset.to_string());
                }
                
                match client.list_projects(params).await {
                    Ok(projects) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&projects).unwrap_or("Failed to format projects".to_string())
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to list projects: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_get_project" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let id = arguments["id"].as_str().unwrap_or("");
                
                match client.get_project(id).await {
                    Ok(project) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&project).unwrap_or("Failed to format project".to_string())
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to get project: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_create_project" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                match client.create_project(arguments).await {
                    Ok(project) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Project created successfully: {}", 
                                serde_json::to_string_pretty(&project).unwrap_or("Failed to format project".to_string()))
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to create project: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_list_users" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let mut params = std::collections::HashMap::new();
                
                // Add optional parameters
                if let Some(status) = arguments["status"].as_u64() {
                    params.insert("status".to_string(), status.to_string());
                }
                if let Some(name) = arguments["name"].as_str() {
                    params.insert("name".to_string(), name.to_string());
                }
                if let Some(limit) = arguments["limit"].as_u64() {
                    params.insert("limit".to_string(), limit.to_string());
                }
                if let Some(offset) = arguments["offset"].as_u64() {
                    params.insert("offset".to_string(), offset.to_string());
                }
                
                match client.list_users(params).await {
                    Ok(users) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&users).unwrap_or("Failed to format users".to_string())
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to list users: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_get_current_user" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                match client.get_current_user().await {
                    Ok(user) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&user).unwrap_or("Failed to format user".to_string())
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to get current user: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_list_time_entries" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                let mut params = std::collections::HashMap::new();
                
                // Add optional parameters
                if let Some(issue_id) = arguments["issue_id"].as_u64() {
                    params.insert("issue_id".to_string(), issue_id.to_string());
                }
                if let Some(project_id) = arguments["project_id"].as_str() {
                    params.insert("project_id".to_string(), project_id.to_string());
                }
                if let Some(user_id) = arguments["user_id"].as_u64() {
                    params.insert("user_id".to_string(), user_id.to_string());
                }
                if let Some(from) = arguments["from"].as_str() {
                    params.insert("from".to_string(), from.to_string());
                }
                if let Some(to) = arguments["to"].as_str() {
                    params.insert("to".to_string(), to.to_string());
                }
                if let Some(limit) = arguments["limit"].as_u64() {
                    params.insert("limit".to_string(), limit.to_string());
                }
                
                match client.list_time_entries(params).await {
                    Ok(entries) => json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&entries).unwrap_or("Failed to format time entries".to_string())
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to list time entries: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
        }
        "redmine_create_time_entry" => {
            let guard = match crate::redmine_client::get_client().await {
                Ok(g) => g,
                Err(e) => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Failed to get Redmine client: {}", e)
                            }]
                        })),
                        error: None,
                    };
                }
            };
            
            if let Some(client) = guard.as_ref() {
                match client.create_time_entry(arguments).await {
                    Ok(entry) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Time entry created successfully: {}", 
                                serde_json::to_string_pretty(&entry).unwrap_or("Failed to format time entry".to_string()))
                        }]
                    }),
                    Err(e) => json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Failed to create time entry: {}", e)
                        }]
                    })
                }
            } else {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "Redmine client not configured. Please run redmine_configure first."
                    }]
                })
            }
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
    axum::serve(listener, router)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    
    Ok(())
}