use reqwest::{Client, RequestBuilder};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::time::Duration;

// Redmine API Error
#[derive(Debug)]
pub struct RedmineApiError {
    pub status: u16,
    pub message: String,
    pub errors: Vec<String>,
}

impl fmt::Display for RedmineApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Redmine API error: {} - {} {}",
            self.status,
            self.message,
            self.errors.join(", ")
        )
    }
}

impl Error for RedmineApiError {}

// Redmine configuration
#[derive(Debug, Clone)]
pub struct RedmineConfig {
    pub host: String,
    pub api_key: String,
}

// Redmine client
pub struct RedmineClient {
    client: Client,
    config: RedmineConfig,
}

impl RedmineClient {
    pub fn new(host: String, api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))  // 10 second timeout
            .connect_timeout(Duration::from_secs(5))  // 5 second connection timeout
            .build()
            .unwrap_or_else(|_| Client::new());
            
        Self {
            client,
            config: RedmineConfig { host, api_key },
        }
    }

    // Build request with authentication
    fn build_request(&self, method: reqwest::Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.config.host, path);
        self.client
            .request(method, url)
            .header("X-Redmine-API-Key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
    }

    // Perform GET request
    pub async fn get(&self, path: &str) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let response = self.build_request(reqwest::Method::GET, path).send().await?;
        
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await?;
            return Err(Box::new(RedmineApiError {
                status,
                message: format!("Request failed"),
                errors: vec![text],
            }));
        }

        Ok(response.json().await?)
    }

    // Perform POST request
    pub async fn post(&self, path: &str, body: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let response = self
            .build_request(reqwest::Method::POST, path)
            .json(&body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await?;
            return Err(Box::new(RedmineApiError {
                status,
                message: format!("Request failed"),
                errors: vec![text],
            }));
        }

        if response.status().as_u16() == 204 {
            return Ok(json!({}));
        }

        Ok(response.json().await?)
    }

    // Perform PUT request
    pub async fn put(&self, path: &str, body: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let response = self
            .build_request(reqwest::Method::PUT, path)
            .json(&body)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await?;
            return Err(Box::new(RedmineApiError {
                status,
                message: format!("Request failed"),
                errors: vec![text],
            }));
        }

        if response.status().as_u16() == 204 {
            return Ok(json!({}));
        }

        Ok(response.json().await?)
    }

    // Perform DELETE request
    pub async fn delete(&self, path: &str) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let response = self
            .build_request(reqwest::Method::DELETE, path)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await?;
            return Err(Box::new(RedmineApiError {
                status,
                message: format!("Request failed"),
                errors: vec![text],
            }));
        }

        Ok(json!({"success": true}))
    }
}

// Issue operations
impl RedmineClient {
    // List issues
    pub async fn list_issues(&self, params: HashMap<String, String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let mut path = "/issues.json".to_string();
        if !params.is_empty() {
            let query = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            path = format!("{}?{}", path, query);
        }
        self.get(&path).await
    }

    // Get issue by ID
    pub async fn get_issue(&self, id: u32) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/issues/{}.json?include=attachments,journals", id);
        self.get(&path).await
    }

    // Create issue
    pub async fn create_issue(&self, issue: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        self.post("/issues.json", json!({ "issue": issue })).await
    }

    // Update issue
    pub async fn update_issue(&self, id: u32, issue: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/issues/{}.json", id);
        self.put(&path, json!({ "issue": issue })).await
    }

    // Delete issue
    pub async fn delete_issue(&self, id: u32) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/issues/{}.json", id);
        self.delete(&path).await
    }
}

// Project operations
impl RedmineClient {
    // List projects
    pub async fn list_projects(&self, params: HashMap<String, String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let mut path = "/projects.json".to_string();
        if !params.is_empty() {
            let query = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            path = format!("{}?{}", path, query);
        }
        self.get(&path).await
    }

    // Get project by ID
    pub async fn get_project(&self, id: &str) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/projects/{}.json?include=trackers,issue_categories,enabled_modules", id);
        self.get(&path).await
    }

    // Create project
    pub async fn create_project(&self, project: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        self.post("/projects.json", json!({ "project": project })).await
    }

    // Update project
    pub async fn update_project(&self, id: &str, project: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/projects/{}.json", id);
        self.put(&path, json!({ "project": project })).await
    }

    // Delete project
    pub async fn delete_project(&self, id: &str) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/projects/{}.json", id);
        self.delete(&path).await
    }
}

// User operations
impl RedmineClient {
    // List users
    pub async fn list_users(&self, params: HashMap<String, String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let mut path = "/users.json".to_string();
        if !params.is_empty() {
            let query = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            path = format!("{}?{}", path, query);
        }
        self.get(&path).await
    }

    // Get current user
    pub async fn get_current_user(&self) -> Result<Value, Box<dyn Error + Send + Sync>> {
        self.get("/users/current.json").await
    }

    // Get user by ID
    pub async fn get_user(&self, id: u32) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/users/{}.json?include=memberships,groups", id);
        self.get(&path).await
    }
}

// Time entry operations
impl RedmineClient {
    // List time entries
    pub async fn list_time_entries(&self, params: HashMap<String, String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let mut path = "/time_entries.json".to_string();
        if !params.is_empty() {
            let query = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            path = format!("{}?{}", path, query);
        }
        self.get(&path).await
    }

    // Create time entry
    pub async fn create_time_entry(&self, time_entry: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        self.post("/time_entries.json", json!({ "time_entry": time_entry })).await
    }

    // Update time entry
    pub async fn update_time_entry(&self, id: u32, time_entry: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/time_entries/{}.json", id);
        self.put(&path, json!({ "time_entry": time_entry })).await
    }

    // Delete time entry
    pub async fn delete_time_entry(&self, id: u32) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let path = format!("/time_entries/{}.json", id);
        self.delete(&path).await
    }
}

// Global Redmine client instance
lazy_static::lazy_static! {
    pub static ref REDMINE_CLIENT: tokio::sync::RwLock<Option<RedmineClient>> = tokio::sync::RwLock::new(None);
}

// Initialize Redmine client
pub async fn init_client(host: String, api_key: String) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = RedmineClient::new(host, api_key);
    let mut guard = REDMINE_CLIENT.write().await;
    *guard = Some(client);
    Ok(())
}

// Get Redmine client
pub async fn get_client() -> Result<tokio::sync::RwLockReadGuard<'static, Option<RedmineClient>>, Box<dyn Error + Send + Sync>> {
    Ok(REDMINE_CLIENT.read().await)
}