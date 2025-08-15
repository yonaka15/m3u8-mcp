use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams
};
use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams
};
use chromiumoxide::page::Page;
use serde_json::{json, Value};
use std::sync::Arc;
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use futures::StreamExt;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

// Console message structure
#[derive(Debug, Clone)]
pub struct ConsoleMessage {
    pub level: String,
    pub text: String,
    pub timestamp: std::time::SystemTime,
}

// Tab structure
#[derive(Clone)]
pub struct Tab {
    pub page: Arc<Page>,
    pub url: String,
    pub title: String,
    pub index: usize,
}

// Browser instance manager
pub struct BrowserManager {
    browser: Option<Browser>,
    tabs: Vec<Tab>,
    current_tab_index: Option<usize>,
    console_messages: VecDeque<ConsoleMessage>,
    max_console_messages: usize,
}

impl BrowserManager {
    pub fn new() -> Self {
        Self {
            browser: None,
            tabs: Vec::new(),
            current_tab_index: None,
            console_messages: VecDeque::new(),
            max_console_messages: 100, // Keep last 100 console messages
        }
    }
    
    // Helper to get current page
    fn current_page(&self) -> Option<Arc<Page>> {
        self.current_tab_index
            .and_then(|idx| self.tabs.get(idx))
            .map(|tab| tab.page.clone())
    }
    
    // Add console message with size limit
    fn add_console_message(&mut self, level: String, text: String) {
        // Trim text to max 200 characters
        let trimmed_text = if text.len() > 200 {
            format!("{}...", &text[..197])
        } else {
            text
        };
        
        let message = ConsoleMessage {
            level,
            text: trimmed_text,
            timestamp: std::time::SystemTime::now(),
        };
        
        self.console_messages.push_back(message);
        
        // Remove oldest messages if we exceed the limit
        while self.console_messages.len() > self.max_console_messages {
            self.console_messages.pop_front();
        }
    }

    // Connect to existing Chrome instance or launch new one
    // headless: true for headless mode (default), false for visible browser window
    pub async fn connect(&mut self, debug_port: Option<u16>, headless: Option<bool>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let port = debug_port.unwrap_or(9222);
        let headless_mode = headless.unwrap_or(false); // デフォルトはheadfulモード
        
        // Try to connect to existing Chrome instance
        let ws_url = format!("ws://localhost:{}/devtools/browser", port);
        
        match Browser::connect(&ws_url).await {
            Ok((browser, mut handler)) => {
                // Spawn handler in background
                tokio::spawn(async move {
                    while let Some(_) = handler.next().await {
                        // Handle browser events
                    }
                });
                
                self.browser = Some(browser);
                Ok(())
            }
            Err(_) => {
                // Launch new Chrome instance with debugging enabled
                // ユニークなユーザーデータディレクトリを使用してプロセスの競合を避ける
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                let user_data_dir = format!("/tmp/chromiumoxide-{}", timestamp);
                
                let mut builder = BrowserConfig::builder();
                
                // headlessモードの設定
                if !headless_mode {
                    builder = builder.with_head(); // 可視ブラウザウィンドウを表示
                }
                // headless_mode == trueの場合は何も追加しない（デフォルトがheadless）
                
                let config = builder
                    .port(port)
                    .no_sandbox()  // サンドボックスを無効化（必要に応じて）
                    .window_size(800, 600)  // ウィンドウサイズを設定（デフォルトビューポートに合わせる）
                    .user_data_dir(&user_data_dir)  // ユニークなプロファイルディレクトリ
                    .build()
                    .map_err(|e| format!("Failed to build browser config: {}", e))?;
                
                let (browser, mut handler) = Browser::launch(config).await?;
                
                // Spawn handler in background
                tokio::spawn(async move {
                    while let Some(_) = handler.next().await {
                        // Handle browser events
                    }
                });
                
                self.browser = Some(browser);
                Ok(())
            }
        }
    }

    // Navigate to URL with retry logic
    pub async fn navigate(&mut self, url: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        // Check browser connection first
        if self.browser.is_none() {
            return Err("Browser not connected".into());
        }
        
        // Retry navigation up to 3 times
        let mut attempts = 0;
        let max_attempts = 3;
        let mut last_error = None;
        
        while attempts < max_attempts {
            attempts += 1;
            
            match self.navigate_internal(url).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempts < max_attempts {
                        // Wait before retry with exponential backoff
                        sleep(Duration::from_millis(500 * attempts)).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| "Navigation failed after retries".into()))
    }
    
    // Internal navigation logic
    async fn navigate_internal(&mut self, url: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let browser = self.browser.as_ref().ok_or("Browser not connected")?;
        let page = browser.new_page(url).await?;
        
        // Set viewport size for the new page using CDP command
        let device_metrics = SetDeviceMetricsOverrideParams::builder()
            .width(800)
            .height(600)
            .device_scale_factor(1.0)
            .mobile(false)
            .build()
            .map_err(|e| format!("Failed to build device metrics: {}", e))?;
        page.execute(device_metrics).await?;
        
        let page_arc = Arc::new(page);
        
        // Wait for page to load with timeout
        let navigation_result = tokio::time::timeout(
            Duration::from_secs(30),
            page_arc.wait_for_navigation()
        ).await;
        
        if let Err(_) = navigation_result {
            // Continue even if navigation times out - page might still be usable
            eprintln!("Navigation timeout for {}, continuing anyway", url);
        }
        
        // Get page title with fallback
        let title = page_arc.get_title()
            .await
            .unwrap_or(Some("Untitled".to_string()))
            .unwrap_or_else(|| "Untitled".to_string());
        
        // Create new tab
        let tab = Tab {
            page: page_arc.clone(),
            url: url.to_string(),
            title,
            index: self.tabs.len(),
        };
        
        self.tabs.push(tab);
        self.current_tab_index = Some(self.tabs.len() - 1);
        
        Ok(json!({
            "success": true,
            "message": format!("Navigated to {}", url),
            "tab_index": self.current_tab_index
        }))
    }

    // Click element with retry and wait
    pub async fn click(&self, selector: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        // Wait for element to be present (up to 10 seconds)
        let mut found = false;
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(10);
        
        while !found && start.elapsed() < timeout {
            match page.find_element(selector).await {
                Ok(element) => {
                    // Try to click the element
                    match element.click().await {
                        Ok(_) => {
                            found = true;
                            break;
                        }
                        Err(e) => {
                            // Element might not be clickable yet
                            if start.elapsed() + Duration::from_millis(500) < timeout {
                                sleep(Duration::from_millis(500)).await;
                            } else {
                                return Err(format!("Failed to click element: {}", e).into());
                            }
                        }
                    }
                }
                Err(_) => {
                    // Element not found yet
                    if start.elapsed() + Duration::from_millis(500) < timeout {
                        sleep(Duration::from_millis(500)).await;
                    } else {
                        return Err(format!("Element not found: {}", selector).into());
                    }
                }
            }
        }
        
        if found {
            Ok(json!({
                "success": true,
                "message": format!("Clicked element: {}", selector)
            }))
        } else {
            Err(format!("Failed to click element: {} (timeout)", selector).into())
        }
    }

    // Type text into element
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        // Find element
        let element = page.find_element(selector).await?;
        
        // Focus on element
        element.focus().await?;
        
        // Type text
        element.type_str(text).await?;
        
        Ok(json!({
            "success": true,
            "message": format!("Typed text into element: {}", selector)
        }))
    }

    // Take screenshot
    pub async fn screenshot(&self, full_page: bool) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        let format = CaptureScreenshotFormat::Png;
        
        let screenshot_data = if full_page {
            page.screenshot(
                CaptureScreenshotParams::builder()
                    .format(format)
                    .capture_beyond_viewport(true)
                    .build()
            ).await?
        } else {
            page.screenshot(
                CaptureScreenshotParams::builder()
                    .format(format)
                    .build()
            ).await?
        };
        
        // Process screenshot with Rust image processor
        // Default quality: 85, max width: 1200px
        match crate::image_processor::process_screenshot(screenshot_data.clone(), 85, 1200) {
            Ok(jpeg_base64) => {
                Ok(json!({
                    "success": true,
                    "screenshot": jpeg_base64  // Return raw base64 without data URL prefix
                }))
            }
            Err(_) => {
                // Fallback to original PNG if processing fails
                let base64_data = BASE64.encode(&screenshot_data);
                Ok(json!({
                    "success": true,
                    "screenshot": base64_data  // Return raw base64 without data URL prefix
                }))
            }
        }
    }

    // Evaluate JavaScript
    pub async fn evaluate(&self, script: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        // Execute JavaScript and get the raw result
        let result = page.evaluate_expression(script).await?;
        
        Ok(json!({
            "success": true,
            "result": result.value().clone()
        }))
    }

    // Wait for selector
    pub async fn wait_for(&self, selector: &str, _timeout_ms: u64) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        // Try to find element
        match page.find_element(selector).await {
            Ok(_) => {
                Ok(json!({
                    "success": true,
                    "message": format!("Element found: {}", selector)
                }))
            }
            Err(e) => {
                Ok(json!({
                    "success": false,
                    "error": format!("Element not found: {}", e)
                }))
            }
        }
    }

    // Get page content
    pub async fn get_content(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        let content = page.content().await?;
        
        Ok(json!({
            "success": true,
            "content": content
        }))
    }

    // Go back
    pub async fn go_back(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        page.evaluate_expression("window.history.back()").await?;
        
        Ok(json!({
            "success": true,
            "message": "Navigated back"
        }))
    }

    // Go forward
    pub async fn go_forward(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        page.evaluate_expression("window.history.forward()").await?;
        
        Ok(json!({
            "success": true,
            "message": "Navigated forward"
        }))
    }

    // Reload page
    pub async fn reload(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        page.reload().await?;
        
        Ok(json!({
            "success": true,
            "message": "Page reloaded"
        }))
    }

    // Close browser
    pub async fn close(&mut self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(mut browser) = self.browser.take() {
            browser.close().await?;
        }
        self.tabs.clear();
        self.current_tab_index = None;
        self.console_messages.clear();
        
        Ok(json!({
            "success": true,
            "message": "Browser closed"
        }))
    }

    // Get current URL
    pub async fn get_url(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        let url = page.url().await?;
        
        Ok(json!({
            "success": true,
            "url": url
        }))
    }

    // Set viewport size
    pub async fn set_viewport(&self, width: u32, height: u32) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        // Execute JavaScript to resize viewport
        let script = format!(
            "window.resizeTo({}, {}); '{}x{}'",
            width, height, width, height
        );
        
        page.evaluate_expression(script.as_str()).await?;
        
        Ok(json!({
            "success": true,
            "message": format!("Viewport set to {}x{}", width, height)
        }))
    }
    
    // Get accessibility snapshot (simplified version)
    pub async fn get_snapshot(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let page = self.current_page()
            .ok_or("No active page")?;
        
        // Get page title and URL
        let title = page.get_title().await.unwrap_or(Some("Untitled".to_string())).unwrap_or("Untitled".to_string());
        let url = page.url().await?;
        
        // Get console messages (last 10 for snapshot)
        let recent_messages: Vec<_> = self.console_messages
            .iter()
            .rev()
            .take(10)
            .map(|msg| json!({
                "level": msg.level,
                "text": msg.text
            }))
            .collect();
        
        // Get list of tabs
        let tabs: Vec<_> = self.tabs
            .iter()
            .enumerate()
            .map(|(idx, tab)| {
                let is_current = Some(idx) == self.current_tab_index;
                json!({
                    "index": idx,
                    "title": tab.title,
                    "url": tab.url,
                    "current": is_current
                })
            })
            .collect();
        
        Ok(json!({
            "success": true,
            "snapshot": {
                "url": url,
                "title": title,
                "tabs": tabs,
                "console_messages": recent_messages
            }
        }))
    }
    
    // Switch to a different tab
    pub async fn switch_tab(&mut self, index: usize) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        if index >= self.tabs.len() {
            return Ok(json!({
                "success": false,
                "error": format!("Tab index {} out of range (0-{})", index, self.tabs.len() - 1)
            }));
        }
        
        self.current_tab_index = Some(index);
        
        Ok(json!({
            "success": true,
            "message": format!("Switched to tab {}", index),
            "tab": {
                "index": index,
                "title": self.tabs[index].title.clone(),
                "url": self.tabs[index].url.clone()
            }
        }))
    }
    
    // Create a new tab and navigate to URL (if provided)
    pub async fn new_tab(&mut self, url: Option<&str>) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let browser = self.browser.as_ref().ok_or("Browser not connected")?;
        
        // Create a new page (tab) using CDP
        let new_page = browser.new_page("about:blank").await?;
        
        // Set viewport size for the new tab using CDP command
        let device_metrics = SetDeviceMetricsOverrideParams::builder()
            .width(800)
            .height(600)
            .device_scale_factor(1.0)
            .mobile(false)
            .build()
            .map_err(|e| format!("Failed to build device metrics: {}", e))?;
        new_page.execute(device_metrics).await?;
        
        // Navigate to URL if provided
        let final_url = if let Some(target_url) = url {
            new_page.goto(target_url).await?;
            target_url.to_string()
        } else {
            "about:blank".to_string()
        };
        
        // Get page title
        let title = match new_page.get_title().await {
            Ok(Some(t)) => t,
            Ok(None) => "New Tab".to_string(),
            Err(_) => "New Tab".to_string(),
        };
        
        // Add to tabs list
        let index = self.tabs.len();
        self.tabs.push(Tab {
            page: Arc::new(new_page),
            url: final_url.clone(),
            title: title.clone(),
            index,
        });
        
        // Set as current tab
        self.current_tab_index = Some(index);
        
        Ok(json!({
            "success": true,
            "message": format!("New tab created at index {}", index),
            "tab": {
                "index": index,
                "title": title,
                "url": final_url
            }
        }))
    }
    
    // Close a specific tab
    pub async fn close_tab(&mut self, index: usize) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        if index >= self.tabs.len() {
            return Ok(json!({
                "success": false,
                "error": format!("Tab index {} out of range", index)
            }));
        }
        
        // Remove the tab
        self.tabs.remove(index);
        
        // Update indices for remaining tabs
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            tab.index = i;
        }
        
        // Update current tab index
        if let Some(current) = self.current_tab_index {
            if current == index {
                // If we closed the current tab, switch to the previous one or none
                self.current_tab_index = if !self.tabs.is_empty() {
                    Some(index.min(self.tabs.len() - 1))
                } else {
                    None
                };
            } else if current > index {
                // Adjust index if current tab was after the closed one
                self.current_tab_index = Some(current - 1);
            }
        }
        
        Ok(json!({
            "success": true,
            "message": format!("Closed tab {}", index),
            "current_tab": self.current_tab_index
        }))
    }
    
    // List all tabs
    pub fn list_tabs(&self) -> Value {
        let tabs: Vec<_> = self.tabs
            .iter()
            .enumerate()
            .map(|(idx, tab)| {
                let is_current = Some(idx) == self.current_tab_index;
                json!({
                    "index": idx,
                    "title": tab.title,
                    "url": tab.url,
                    "current": is_current
                })
            })
            .collect();
        
        json!({
            "success": true,
            "tabs": tabs,
            "current_index": self.current_tab_index
        })
    }
    
    // Export current session state for debugging
    pub fn export_session(&self) -> Value {
        json!({
            "browser_connected": self.browser.is_some(),
            "tabs": self.tabs.iter().map(|tab| {
                json!({
                    "url": tab.url,
                    "title": tab.title,
                    "index": tab.index
                })
            }).collect::<Vec<_>>(),
            "current_tab": self.current_tab_index,
            "console_messages_count": self.console_messages.len(),
            "max_console_messages": self.max_console_messages,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        })
    }
    
    // Clear console messages
    pub fn clear_console(&mut self) {
        self.console_messages.clear();
    }
    
    // Get recent console messages
    pub fn get_console_messages(&self, limit: usize) -> Vec<Value> {
        self.console_messages
            .iter()
            .rev()
            .take(limit)
            .map(|msg| json!({
                "level": msg.level,
                "text": msg.text,
                "timestamp": msg.timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            }))
            .collect()
    }
}

// Global browser manager instance
lazy_static::lazy_static! {
    pub static ref BROWSER_MANAGER: Arc<RwLock<BrowserManager>> = Arc::new(RwLock::new(BrowserManager::new()));
}

// Helper functions for MCP integration

// Open a new browser instance with specified mode
pub async fn handle_browser_open(headless: bool) -> Value {
    let mut manager = BROWSER_MANAGER.write().await;
    
    // Close existing browser if any
    if manager.browser.is_some() {
        let _ = manager.close().await;
    }
    
    // Open new browser with specified mode
    if let Err(e) = manager.connect(None, Some(headless)).await {
        return json!({
            "success": false,
            "error": format!("Failed to open browser: {}", e)
        });
    }
    
    json!({
        "success": true,
        "message": format!("Browser opened in {} mode", if headless { "headless" } else { "headful" })
    })
}

// Navigate to URL (requires browser to be opened first)
pub async fn handle_browser_navigate(url: &str) -> Value {
    let mut manager = BROWSER_MANAGER.write().await;
    
    // Check if browser is opened
    if manager.browser.is_none() {
        return json!({
            "success": false,
            "error": "Browser not opened. Please call browser_open first with desired mode (headless: true/false)"
        });
    }
    
    match manager.navigate(url).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Navigation failed: {}", e)
        })
    }
}

pub async fn handle_browser_click(selector: &str) -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.click(selector).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Click failed: {}", e)
        })
    }
}

pub async fn handle_browser_type(selector: &str, text: &str) -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.type_text(selector, text).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Type failed: {}", e)
        })
    }
}

pub async fn handle_browser_screenshot(full_page: bool) -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.screenshot(full_page).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Screenshot failed: {}", e)
        })
    }
}

pub async fn handle_browser_evaluate(script: &str) -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.evaluate(script).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Evaluate failed: {}", e)
        })
    }
}

pub async fn handle_browser_wait_for(selector: &str, timeout_ms: u64) -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.wait_for(selector, timeout_ms).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Wait failed: {}", e)
        })
    }
}

pub async fn handle_browser_get_content() -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.get_content().await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Get content failed: {}", e)
        })
    }
}

pub async fn handle_browser_go_back() -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.go_back().await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Go back failed: {}", e)
        })
    }
}

pub async fn handle_browser_go_forward() -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.go_forward().await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Go forward failed: {}", e)
        })
    }
}

pub async fn handle_browser_reload() -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.reload().await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Reload failed: {}", e)
        })
    }
}

pub async fn handle_browser_close() -> Value {
    let mut manager = BROWSER_MANAGER.write().await;
    
    match manager.close().await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Close failed: {}", e)
        })
    }
}

// Handle browser snapshot
pub async fn handle_browser_snapshot() -> Value {
    let manager = BROWSER_MANAGER.read().await;
    
    match manager.get_snapshot().await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Snapshot failed: {}", e)
        })
    }
}

// Handle tab list
pub async fn handle_browser_tab_list() -> Value {
    let manager = BROWSER_MANAGER.read().await;
    manager.list_tabs()
}

// Handle tab switch
pub async fn handle_browser_tab_switch(index: usize) -> Value {
    let mut manager = BROWSER_MANAGER.write().await;
    
    match manager.switch_tab(index).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Tab switch failed: {}", e)
        })
    }
}

// Handle tab close
pub async fn handle_browser_tab_close(index: usize) -> Value {
    let mut manager = BROWSER_MANAGER.write().await;
    
    match manager.close_tab(index).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Tab close failed: {}", e)
        })
    }
}

// Handle new tab creation
pub async fn handle_browser_tab_new(url: Option<String>) -> Value {
    let mut manager = BROWSER_MANAGER.write().await;
    
    // Check if browser is opened
    if manager.browser.is_none() {
        return json!({
            "success": false,
            "error": "Browser not opened. Please call browser_open first"
        });
    }
    
    match manager.new_tab(url.as_deref()).await {
        Ok(result) => result,
        Err(e) => json!({
            "success": false,
            "error": format!("Tab creation failed: {}", e)
        })
    }
}