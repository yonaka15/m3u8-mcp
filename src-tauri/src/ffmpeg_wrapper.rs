use std::path::{Path, PathBuf};
use std::process::Command;
use std::error::Error;
use std::fmt;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum FFmpegError {
    NotInstalled,
    CommandFailed(String),
    InvalidInput(String),
    OutputError(String),
}

impl fmt::Display for FFmpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FFmpegError::NotInstalled => write!(f, "FFmpeg is not installed or not in PATH"),
            FFmpegError::CommandFailed(msg) => write!(f, "FFmpeg command failed: {}", msg),
            FFmpegError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            FFmpegError::OutputError(msg) => write!(f, "Output error: {}", msg),
        }
    }
}

impl Error for FFmpegError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FFmpegConfig {
    pub ffmpeg_path: Option<String>,
    pub default_output_dir: PathBuf,
    pub timeout_seconds: u64,
}

impl Default for FFmpegConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            ffmpeg_path: None,
            default_output_dir: home_dir.join("Downloads").join("m3u8-mcp"),
            timeout_seconds: 3600, // 1 hour default timeout
        }
    }
}

pub struct FFmpegWrapper {
    config: FFmpegConfig,
    app_handle: Option<tauri::AppHandle>,
    current_download: Arc<Mutex<Option<tokio::process::Child>>>,
}

impl FFmpegWrapper {
    pub fn new(config: FFmpegConfig) -> Self {
        Self { 
            config,
            app_handle: None,
            current_download: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_app_handle(&mut self, handle: Option<tauri::AppHandle>) {
        self.app_handle = handle;
    }

    pub fn check_installation(&self) -> Result<String, FFmpegError> {
        let ffmpeg_cmd = self.get_ffmpeg_command();
        
        let output = Command::new(&ffmpeg_cmd)
            .arg("-version")
            .output()
            .map_err(|_| FFmpegError::NotInstalled)?;
        
        if !output.status.success() {
            return Err(FFmpegError::NotInstalled);
        }
        
        let version = String::from_utf8_lossy(&output.stdout);
        Ok(version.lines().next().unwrap_or("Unknown version").to_string())
    }

    pub async fn cancel_download(&self) -> Result<(), FFmpegError> {
        println!("FFmpegWrapper::cancel_download called");
        let mut download = self.current_download.lock().await;
        if let Some(mut child) = download.take() {
            println!("Found active download process, attempting to kill...");
            // Try to kill the process gracefully
            child.kill().await
                .map_err(|e| {
                    eprintln!("Failed to kill process: {}", e);
                    FFmpegError::CommandFailed(format!("Failed to cancel download: {}", e))
                })?;
            
            println!("Process killed successfully");
            
            // Emit cancellation event
            if let Some(ref app) = self.app_handle {
                app.emit("download-progress", serde_json::json!({
                    "status": "cancelled",
                    "message": "Download cancelled by user"
                })).ok();
            }
            
            Ok(())
        } else {
            eprintln!("No active download found to cancel");
            Err(FFmpegError::CommandFailed("No download in progress".to_string()))
        }
    }

    pub async fn download_stream(
        &self,
        url: &str,
        output_path: Option<&Path>,
    ) -> Result<PathBuf, FFmpegError> {
        use std::process::Stdio;
        
        println!("FFmpegWrapper::download_stream called with URL: {}", url);
        
        // Validate input URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            eprintln!("Invalid URL format: {}", url);
            return Err(FFmpegError::InvalidInput("URL must be HTTP or HTTPS".to_string()));
        }

        // Determine output path
        let output = if let Some(path) = output_path {
            println!("Using provided output path: {:?}", path);
            path.to_path_buf()
        } else {
            println!("Generating default output path...");
            let generated_path = self.generate_output_path(url)?;
            println!("Generated output path: {:?}", generated_path);
            generated_path
        };

        // Ensure output directory exists
        if let Some(parent) = output.parent() {
            println!("Creating output directory: {:?}", parent);
            std::fs::create_dir_all(parent)
                .map_err(|e| {
                    eprintln!("Failed to create output directory: {}", e);
                    FFmpegError::OutputError(e.to_string())
                })?;
        }

        // Build FFmpeg command
        let ffmpeg_cmd = self.get_ffmpeg_command();
        println!("Using FFmpeg command: {}", ffmpeg_cmd);
        
        let mut command = tokio::process::Command::new(&ffmpeg_cmd);
        
        // Use stderr for progress (FFmpeg outputs progress to stderr by default)
        command
            .arg("-i")
            .arg(url)
            .arg("-c:v")
            .arg("copy")
            .arg("-c:a")
            .arg("copy")
            .arg("-map")
            .arg("0:v:0")  // Select first video stream
            .arg("-map")
            .arg("0:a?")   // Select all audio streams (optional)
            .arg("-stats")  // Show progress statistics
            .arg("-y") // Overwrite output file if exists
            .arg(&output)
            .stdout(Stdio::null())  // Ignore stdout
            .stderr(Stdio::piped()); // Capture stderr for progress

        println!("Starting FFmpeg download with real-time progress...");
        
        // Emit progress event to UI
        if let Some(ref app) = self.app_handle {
            app.emit("download-progress", serde_json::json!({
                "status": "progress",
                "message": "Starting download..."
            })).ok();
        }
        
        // Spawn the command
        let child = command.spawn()
            .map_err(|e| {
                eprintln!("Failed to spawn FFmpeg command: {}", e);
                FFmpegError::CommandFailed(format!("Failed to spawn FFmpeg: {}", e))
            })?;
        
        // Store the child process for potential cancellation
        {
            let mut download = self.current_download.lock().await;
            *download = Some(child);
        }
        
        // Clone the Arc for async processing
        let download_arc = self.current_download.clone();

        // Read progress from stderr
        let stderr = {
            let mut download = download_arc.lock().await;
            if let Some(ref mut child) = *download {
                child.stderr.take()
            } else {
                None
            }
        };
        
        if let Some(stderr) = stderr {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut last_progress_time = std::time::Instant::now();
            
            while let Ok(Some(line)) = lines.next_line().await {
                    // FFmpeg outputs progress like: "frame= 1234 fps=123 q=-1.0 size=   12345kB time=00:01:23.45 bitrate= 123.4kbits/s speed=1.23x"
                    if line.contains("time=") && line.contains("speed=") {
                        // Extract time
                        let time_part = line.split("time=").nth(1)
                            .and_then(|s| s.split_whitespace().next());
                        
                        // Extract speed
                        let speed_part = line.split("speed=").nth(1)
                            .and_then(|s| s.split_whitespace().next());
                        
                        // Extract size
                        let size_part = line.split("size=").nth(1)
                            .and_then(|s| s.split_whitespace().next());
                        
                        // Throttle updates to once per second
                        if last_progress_time.elapsed() >= std::time::Duration::from_secs(1) {
                            let progress_msg = format!(
                                "Time: {} | Size: {} | Speed: {}",
                                time_part.unwrap_or("--:--:--"),
                                size_part.unwrap_or("--"),
                                speed_part.unwrap_or("--")
                            );
                            
                            println!("Progress: {}", progress_msg);
                            
                            // Emit progress event to UI
                            if let Some(ref app) = self.app_handle {
                                app.emit("download-progress", serde_json::json!({
                                    "status": "progress",
                                    "message": progress_msg,
                                    "time": time_part,
                                    "size": size_part,
                                    "speed": speed_part
                                })).ok();
                            }
                            
                            last_progress_time = std::time::Instant::now();
                        }
                    }
            }
        }

        // Monitor the process - don't take it out!
        // Create a separate task to monitor the process
        let monitor_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                let mut download = download_arc.lock().await;
                if let Some(ref mut child) = *download {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            // Process has finished
                            println!("FFmpeg process finished with status: {:?}", status);
                            return Ok(status);
                        }
                        Ok(None) => {
                            // Process is still running
                            continue;
                        }
                        Err(e) => {
                            eprintln!("Error checking process status: {}", e);
                            return Err(e);
                        }
                    }
                } else {
                    // Process was cancelled
                    println!("Process was cancelled or removed");
                    return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Download cancelled"));
                }
            }
        });
        
        // Wait for the monitoring task to complete
        let status = monitor_handle.await
            .map_err(|e| FFmpegError::CommandFailed(format!("Monitor task failed: {}", e)))?
            .map_err(|e| FFmpegError::CommandFailed(format!("Process error: {}", e)))?;

        // Clear the download reference after completion
        {
            let mut download = self.current_download.lock().await;
            *download = None;
        }
        
        if !status.success() {
            // Check if it was cancelled (killed signal)
            if status.code() == Some(255) || status.code().is_none() {
                return Err(FFmpegError::CommandFailed("Download cancelled".to_string()));
            }
            
            return Err(FFmpegError::CommandFailed(format!("FFmpeg exited with status: {:?}", status)));
        }

        println!("FFmpeg download completed successfully");
        println!("Output file: {}", output.display());
        Ok(output)
    }

    pub async fn convert_to_hls(
        &self,
        input_path: &Path,
        output_dir: &Path,
        segment_duration: u32,
    ) -> Result<PathBuf, FFmpegError> {
        // Validate input file exists
        if !input_path.exists() {
            return Err(FFmpegError::InvalidInput("Input file does not exist".to_string()));
        }

        // Create output directory
        std::fs::create_dir_all(output_dir)
            .map_err(|e| FFmpegError::OutputError(e.to_string()))?;

        let playlist_path = output_dir.join("playlist.m3u8");
        let segment_pattern = output_dir.join("segment%03d.ts");

        let ffmpeg_cmd = self.get_ffmpeg_command();
        let mut command = Command::new(&ffmpeg_cmd);
        
        command
            .arg("-i")
            .arg(input_path)
            .arg("-c:v")
            .arg("copy")
            .arg("-c:a")
            .arg("copy")
            .arg("-f")
            .arg("hls")
            .arg("-hls_time")
            .arg(segment_duration.to_string())
            .arg("-hls_list_size")
            .arg("0")
            .arg("-hls_segment_filename")
            .arg(&segment_pattern)
            .arg(&playlist_path);

        let output = command.output()
            .map_err(|e| FFmpegError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(FFmpegError::CommandFailed(error_msg.to_string()));
        }

        Ok(playlist_path)
    }

    pub async fn merge_segments(
        &self,
        segment_list: &[PathBuf],
        output_path: &Path,
    ) -> Result<PathBuf, FFmpegError> {
        if segment_list.is_empty() {
            return Err(FFmpegError::InvalidInput("No segments provided".to_string()));
        }

        // Create a temporary file list for FFmpeg concat
        let temp_dir = std::env::temp_dir();
        let list_file = temp_dir.join(format!("m3u8_mcp_segments_{}.txt", 
            std::process::id()));
        
        // Write segment list to file
        let mut list_content = String::new();
        for segment in segment_list {
            list_content.push_str(&format!("file '{}'\n", segment.display()));
        }
        
        std::fs::write(&list_file, list_content)
            .map_err(|e| FFmpegError::OutputError(e.to_string()))?;

        // Run FFmpeg concat
        let ffmpeg_cmd = self.get_ffmpeg_command();
        let mut command = Command::new(&ffmpeg_cmd);
        
        command
            .arg("-f")
            .arg("concat")
            .arg("-safe")
            .arg("0")
            .arg("-i")
            .arg(&list_file)
            .arg("-c")
            .arg("copy")
            .arg(output_path);

        let output = command.output()
            .map_err(|e| FFmpegError::CommandFailed(e.to_string()))?;

        // Clean up temp file
        let _ = std::fs::remove_file(&list_file);

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(FFmpegError::CommandFailed(error_msg.to_string()));
        }

        Ok(output_path.to_path_buf())
    }

    pub async fn probe_stream(&self, url: &str) -> Result<String, FFmpegError> {
        let ffprobe_cmd = self.get_ffprobe_command();
        
        let output = Command::new(&ffprobe_cmd)
            .arg("-v")
            .arg("quiet")
            .arg("-print_format")
            .arg("json")
            .arg("-show_format")
            .arg("-show_streams")
            .arg(url)
            .output()
            .map_err(|e| FFmpegError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(FFmpegError::CommandFailed(error_msg.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn get_ffmpeg_command(&self) -> String {
        self.config.ffmpeg_path.clone()
            .unwrap_or_else(|| "ffmpeg".to_string())
    }

    fn get_ffprobe_command(&self) -> String {
        if let Some(ffmpeg_path) = &self.config.ffmpeg_path {
            // If custom FFmpeg path is provided, derive ffprobe path
            ffmpeg_path.replace("ffmpeg", "ffprobe")
        } else {
            "ffprobe".to_string()
        }
    }

    fn generate_output_path(&self, url: &str) -> Result<PathBuf, FFmpegError> {
        // Extract filename from URL or generate one
        let filename = if let Some(pos) = url.rfind('/') {
            let name = &url[pos + 1..];
            if name.ends_with(".m3u8") {
                name.replace(".m3u8", ".mp4")
            } else {
                format!("{}.mp4", name)
            }
        } else {
            format!("stream_{}.mp4", chrono::Local::now().format("%Y%m%d_%H%M%S"))
        };

        // Sanitize filename
        let safe_filename: String = filename
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            })
            .collect();

        Ok(self.config.default_output_dir.join(safe_filename))
    }
}