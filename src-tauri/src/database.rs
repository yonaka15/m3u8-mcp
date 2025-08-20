use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedPlaylist {
    pub id: i32,
    pub url: String,
    pub playlist_type: String,  // "master" or "media"
    pub version: Option<i32>,
    pub target_duration: Option<i32>,
    pub media_sequence: Option<i32>,
    pub segments_count: Option<i32>,
    pub total_duration: Option<f64>,
    pub data: String,  // JSON serialized playlist data
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadedStream {
    pub id: i32,
    pub url: String,
    pub output_path: String,
    pub file_size: Option<i64>,
    pub duration: Option<f64>,
    pub format: Option<String>,
    pub resolution: Option<String>,
    pub bitrate: Option<i32>,
    pub downloaded_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProbeResult {
    pub id: i32,
    pub url: String,
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub duration: Option<f64>,
    pub size: Option<i64>,
    pub bit_rate: Option<i32>,
    pub probe_score: Option<i32>,
    pub streams_info: Option<String>,  // JSON serialized stream info
    pub metadata: Option<String>,      // JSON serialized metadata
    pub probed_at: DateTime<Utc>,
}

impl Database {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        // Create directory if it doesn't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                    Some(format!("Failed to create database directory: {}", e)),
                )
            })?;
        }
        
        let conn = Connection::open(db_path)?;
        let db = Database {
            conn: Mutex::new(conn),
        };
        
        db.init_schema()?;
        Ok(db)
    }
    
    pub fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // m3u8 playlists cache table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cached_playlists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT UNIQUE NOT NULL,
                playlist_type TEXT NOT NULL,
                version INTEGER,
                target_duration INTEGER,
                media_sequence INTEGER,
                segments_count INTEGER,
                total_duration REAL,
                data TEXT NOT NULL,
                cached_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        
        // Downloaded streams table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS downloaded_streams (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                output_path TEXT NOT NULL,
                file_size INTEGER,
                duration REAL,
                format TEXT,
                resolution TEXT,
                bitrate INTEGER,
                downloaded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        
        // FFmpeg probe results cache
        conn.execute(
            "CREATE TABLE IF NOT EXISTS probe_cache (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT UNIQUE NOT NULL,
                format_name TEXT,
                format_long_name TEXT,
                duration REAL,
                size INTEGER,
                bit_rate INTEGER,
                probe_score INTEGER,
                streams_info TEXT,
                metadata TEXT,
                probed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        
        // Create indexes for better query performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_playlists_url 
             ON cached_playlists(url)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_playlists_cached_at 
             ON cached_playlists(cached_at)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_downloads_url 
             ON downloaded_streams(url)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_downloads_downloaded_at 
             ON downloaded_streams(downloaded_at)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_probe_url 
             ON probe_cache(url)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_probe_probed_at 
             ON probe_cache(probed_at)",
            [],
        )?;
        
        Ok(())
    }
    
    // Cache a parsed m3u8 playlist
    pub fn cache_playlist(&self, url: &str, playlist_type: &str, data: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "INSERT OR REPLACE INTO cached_playlists (url, playlist_type, data, cached_at) 
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![url, playlist_type, data],
        )?;
        
        Ok(())
    }
    
    // Get cached playlist
    pub fn get_cached_playlist(&self, url: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        
        let result = conn.query_row(
            "SELECT data FROM cached_playlists WHERE url = ?1",
            params![url],
            |row| row.get(0),
        ).optional()?;
        
        Ok(result)
    }
    
    // Save download record
    pub fn save_download(&self, url: &str, output_path: &str, file_size: Option<i64>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "INSERT INTO downloaded_streams (url, output_path, file_size, downloaded_at) 
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![url, output_path, file_size],
        )?;
        
        Ok(())
    }
    
    // Get download history
    pub fn get_download_history(&self, limit: i32) -> Result<Vec<DownloadedStream>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, output_path, file_size, duration, format, resolution, bitrate, downloaded_at 
             FROM downloaded_streams 
             ORDER BY downloaded_at DESC 
             LIMIT ?1"
        )?;
        
        let downloads = stmt.query_map(params![limit], |row| {
            Ok(DownloadedStream {
                id: row.get(0)?,
                url: row.get(1)?,
                output_path: row.get(2)?,
                file_size: row.get(3)?,
                duration: row.get(4)?,
                format: row.get(5)?,
                resolution: row.get(6)?,
                bitrate: row.get(7)?,
                downloaded_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
        
        Ok(downloads)
    }
    
    // Cache probe result
    pub fn cache_probe_result(&self, url: &str, format_name: &str, streams_info: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "INSERT OR REPLACE INTO probe_cache (url, format_name, streams_info, probed_at) 
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![url, format_name, streams_info],
        )?;
        
        Ok(())
    }
    
    // Get cached probe result
    pub fn get_cached_probe(&self, url: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        
        let result = conn.query_row(
            "SELECT streams_info FROM probe_cache WHERE url = ?1",
            params![url],
            |row| row.get(0),
        ).optional()?;
        
        Ok(result)
    }
    
    // Search cached playlists
    pub fn search_cached_playlists(&self, query: &str) -> Result<Vec<CachedPlaylist>> {
        let conn = self.conn.lock().unwrap();
        let search_pattern = format!("%{}%", query);
        
        let mut stmt = conn.prepare(
            "SELECT id, url, playlist_type, version, target_duration, media_sequence, 
                    segments_count, total_duration, data, cached_at 
             FROM cached_playlists 
             WHERE url LIKE ?1 OR data LIKE ?1 
             ORDER BY cached_at DESC 
             LIMIT 100"
        )?;
        
        let playlists = stmt.query_map(params![search_pattern], |row| {
            Ok(CachedPlaylist {
                id: row.get(0)?,
                url: row.get(1)?,
                playlist_type: row.get(2)?,
                version: row.get(3)?,
                target_duration: row.get(4)?,
                media_sequence: row.get(5)?,
                segments_count: row.get(6)?,
                total_duration: row.get(7)?,
                data: row.get(8)?,
                cached_at: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
        
        Ok(playlists)
    }
    
    // Clear all cache
    pub fn clear_all_cache(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM cached_playlists", [])?;
        conn.execute("DELETE FROM downloaded_streams", [])?;
        conn.execute("DELETE FROM probe_cache", [])?;
        Ok(())
    }
    
    // Clear old cache entries
    pub fn clear_old_cache(&self, days: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let query = format!("DELETE FROM cached_playlists WHERE cached_at < datetime('now', '-{} days')", days);
        conn.execute(&query, [])?;
        
        let query = format!("DELETE FROM probe_cache WHERE probed_at < datetime('now', '-{} days')", days);
        conn.execute(&query, [])?;
        Ok(())
    }
    
    // Get cache statistics
    pub fn get_cache_stats(&self) -> Result<serde_json::Value> {
        let conn = self.conn.lock().unwrap();
        
        let playlist_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM cached_playlists",
            [],
            |row| row.get(0),
        )?;
        
        let download_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM downloaded_streams",
            [],
            |row| row.get(0),
        )?;
        
        let probe_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM probe_cache",
            [],
            |row| row.get(0),
        )?;
        
        let total_size: Option<i64> = conn.query_row(
            "SELECT SUM(file_size) FROM downloaded_streams",
            [],
            |row| row.get(0),
        ).optional()?;
        
        let latest_download: Option<String> = conn.query_row(
            "SELECT downloaded_at FROM downloaded_streams ORDER BY downloaded_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        ).optional()?;
        
        let latest_cache: Option<String> = conn.query_row(
            "SELECT cached_at FROM cached_playlists ORDER BY cached_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        ).optional()?;
        
        Ok(serde_json::json!({
            "cached_playlists": playlist_count,
            "downloaded_streams": download_count,
            "probe_results": probe_count,
            "total_download_size": total_size.unwrap_or(0),
            "latest_download": latest_download,
            "latest_cache": latest_cache,
        }))
    }
}

// Global database instance for use in async contexts
lazy_static::lazy_static! {
    pub static ref GLOBAL_DB: tokio::sync::RwLock<Option<std::sync::Arc<Database>>> = 
        tokio::sync::RwLock::new(None);
}

// Initialize global database
pub async fn init_global_db(db_path: PathBuf) -> Result<()> {
    let db = Database::new(db_path)?;
    let mut global = GLOBAL_DB.write().await;
    *global = Some(std::sync::Arc::new(db));
    Ok(())
}