use std::process::Command;
use serde_json::json;

pub fn open_browser(url: &str) -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        match Command::new("open").arg(url).spawn() {
            Ok(_) => json!({
                "success": true,
                "message": format!("Opened browser with URL: {}", url)
            }),
            Err(e) => json!({
                "success": false,
                "message": format!("Failed to open browser: {}", e)
            })
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        match Command::new("cmd").args(&["/C", "start", url]).spawn() {
            Ok(_) => json!({
                "success": true,
                "message": format!("Opened browser with URL: {}", url)
            }),
            Err(e) => json!({
                "success": false,
                "message": format!("Failed to open browser: {}", e)
            })
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        match Command::new("xdg-open").arg(url).spawn() {
            Ok(_) => json!({
                "success": true,
                "message": format!("Opened browser with URL: {}", url)
            }),
            Err(e) => json!({
                "success": false,
                "message": format!("Failed to open browser: {}", e)
            })
        }
    }
}