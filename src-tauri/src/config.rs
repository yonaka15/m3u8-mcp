use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub headless: bool,
    pub timeout_seconds: u64,
    pub retry_attempts: u32,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub user_agent: Option<String>,
    pub proxy: Option<String>,
    pub max_tabs: usize,
    pub console_message_limit: usize,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: false,
            timeout_seconds: 30,
            retry_attempts: 3,
            viewport_width: 1280,
            viewport_height: 1024,
            user_agent: None,
            proxy: None,
            max_tabs: 10,
            console_message_limit: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub port: u16,
    pub host: String,
    pub max_sessions: usize,
    pub session_timeout_minutes: u64,
    pub cors_enabled: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            port: 37650,
            host: "127.0.0.1".to_string(),
            max_sessions: 10,
            session_timeout_minutes: 60,
            cors_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub browser: BrowserConfig,
    pub mcp: McpConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            browser: BrowserConfig::default(),
            mcp: McpConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        
        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)?;
            let config: AppConfig = serde_json::from_str(&contents)?;
            Ok(config)
        } else {
            // Create default config
            let config = AppConfig::default();
            config.save()?;
            Ok(config)
        }
    }
    
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        
        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, contents)?;
        
        Ok(())
    }
    
    fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?;
        
        Ok(config_dir.join("browser-automation").join("config.json"))
    }
}