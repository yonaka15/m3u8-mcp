use reqwest;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum M3u8Error {
    NetworkError(String),
    ParseError(String),
    InvalidUrl(String),
}

impl fmt::Display for M3u8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            M3u8Error::NetworkError(msg) => write!(f, "Network error: {}", msg),
            M3u8Error::ParseError(msg) => write!(f, "Parse error: {}", msg),
            M3u8Error::InvalidUrl(msg) => write!(f, "Invalid URL: {}", msg),
        }
    }
}

impl Error for M3u8Error {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Segment {
    pub uri: String,
    pub duration: f32,
    pub title: Option<String>,
    pub byte_range: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: u64,
    pub resolution: Option<String>,
    pub codecs: Option<String>,
    pub frame_rate: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ParsedPlaylist {
    #[serde(rename = "master")]
    Master {
        version: Option<u8>,
        variants: Vec<Variant>,
    },
    #[serde(rename = "media")]
    Media {
        version: Option<u8>,
        target_duration: Option<u64>,
        segments: Vec<Segment>,
    },
}

pub struct M3u8Parser {
    client: reqwest::Client,
}

impl M3u8Parser {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("m3u8-mcp/0.1.0")
            .build()
            .unwrap_or_default();
        
        Self { client }
    }

    pub async fn parse_url(&self, url: &str) -> Result<ParsedPlaylist, M3u8Error> {
        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(M3u8Error::InvalidUrl("URL must start with http:// or https://".to_string()));
        }

        // Fetch the playlist content
        let content = self.fetch_playlist(url).await?;
        
        // Parse the playlist
        self.parse_content(&content, url)
    }

    async fn fetch_playlist(&self, url: &str) -> Result<String, M3u8Error> {
        let response = self.client
            .get(url)
            .send()
            .await
            .map_err(|e| M3u8Error::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(M3u8Error::NetworkError(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        response
            .text()
            .await
            .map_err(|e| M3u8Error::NetworkError(e.to_string()))
    }

    fn parse_content(&self, content: &str, base_url: &str) -> Result<ParsedPlaylist, M3u8Error> {
        // Check if it's a valid m3u8 file
        if !content.starts_with("#EXTM3U") {
            return Err(M3u8Error::ParseError("Not a valid m3u8 file".to_string()));
        }

        // Determine if it's a master or media playlist
        if content.contains("#EXT-X-STREAM-INF:") {
            self.parse_master_playlist(content, base_url)
        } else {
            self.parse_media_playlist(content, base_url)
        }
    }

    fn parse_master_playlist(&self, content: &str, base_url: &str) -> Result<ParsedPlaylist, M3u8Error> {
        let mut variants = Vec::new();
        let mut version = None;
        let lines: Vec<&str> = content.lines().collect();
        
        for i in 0..lines.len() {
            let line = lines[i].trim();
            
            if line.starts_with("#EXT-X-VERSION:") {
                version = line.replace("#EXT-X-VERSION:", "")
                    .trim()
                    .parse::<u8>()
                    .ok();
            } else if line.starts_with("#EXT-X-STREAM-INF:") {
                let info = line.replace("#EXT-X-STREAM-INF:", "");
                let mut variant = Variant {
                    uri: String::new(),
                    bandwidth: 0,
                    resolution: None,
                    codecs: None,
                    frame_rate: None,
                };

                // Parse attributes
                for attr in info.split(',') {
                    let parts: Vec<&str> = attr.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        let key = parts[0].trim();
                        let value = parts[1].trim().trim_matches('"');
                        
                        match key {
                            "BANDWIDTH" => {
                                variant.bandwidth = value.parse().unwrap_or(0);
                            }
                            "RESOLUTION" => {
                                variant.resolution = Some(value.to_string());
                            }
                            "CODECS" => {
                                variant.codecs = Some(value.to_string());
                            }
                            "FRAME-RATE" => {
                                variant.frame_rate = value.parse().ok();
                            }
                            _ => {}
                        }
                    }
                }

                // Next line should be the URI
                if i + 1 < lines.len() {
                    let uri = lines[i + 1].trim();
                    if !uri.starts_with("#") {
                        variant.uri = self.resolve_uri(uri, base_url);
                        variants.push(variant);
                    }
                }
            }
        }

        Ok(ParsedPlaylist::Master { version, variants })
    }

    fn parse_media_playlist(&self, content: &str, base_url: &str) -> Result<ParsedPlaylist, M3u8Error> {
        let mut segments = Vec::new();
        let mut version = None;
        let mut target_duration = None;
        let lines: Vec<&str> = content.lines().collect();
        
        for i in 0..lines.len() {
            let line = lines[i].trim();
            
            if line.starts_with("#EXT-X-VERSION:") {
                version = line.replace("#EXT-X-VERSION:", "")
                    .trim()
                    .parse::<u8>()
                    .ok();
            } else if line.starts_with("#EXT-X-TARGETDURATION:") {
                target_duration = line.replace("#EXT-X-TARGETDURATION:", "")
                    .trim()
                    .parse::<u64>()
                    .ok();
            } else if line.starts_with("#EXTINF:") {
                let info = line.replace("#EXTINF:", "");
                let parts: Vec<&str> = info.split(',').collect();
                
                let duration = parts[0].parse::<f32>().unwrap_or(0.0);
                let title = if parts.len() > 1 {
                    Some(parts[1].to_string())
                } else {
                    None
                };

                // Next line should be the URI
                if i + 1 < lines.len() {
                    let uri = lines[i + 1].trim();
                    if !uri.starts_with("#") {
                        segments.push(Segment {
                            uri: self.resolve_uri(uri, base_url),
                            duration,
                            title,
                            byte_range: None,
                        });
                    }
                }
            } else if line.starts_with("#EXT-X-BYTERANGE:") && !segments.is_empty() {
                let byte_range = line.replace("#EXT-X-BYTERANGE:", "").trim().to_string();
                if let Some(last) = segments.last_mut() {
                    last.byte_range = Some(byte_range);
                }
            }
        }

        Ok(ParsedPlaylist::Media {
            version,
            target_duration,
            segments,
        })
    }

    fn resolve_uri(&self, uri: &str, base_url: &str) -> String {
        if uri.starts_with("http://") || uri.starts_with("https://") {
            uri.to_string()
        } else if uri.starts_with("/") {
            // Absolute path
            if let Ok(url) = url::Url::parse(base_url) {
                format!("{}://{}{}", url.scheme(), url.host_str().unwrap_or(""), uri)
            } else {
                uri.to_string()
            }
        } else {
            // Relative path
            if let Some(pos) = base_url.rfind('/') {
                format!("{}/{}", &base_url[..pos], uri)
            } else {
                format!("{}/{}", base_url, uri)
            }
        }
    }

    pub async fn extract_segments(&self, url: &str, base_url: Option<&str>) -> Result<Vec<String>, M3u8Error> {
        // Fetch the playlist content
        let content = self.fetch_playlist(url).await?;
        
        // Use the provided base_url or the URL itself
        let base = base_url.unwrap_or(url);
        
        // Parse the playlist
        let playlist = self.parse_content(&content, base)?;
        
        match playlist {
            ParsedPlaylist::Media { segments, .. } => {
                // Extract segment URLs from media playlist
                Ok(segments.into_iter().map(|s| s.uri).collect())
            }
            ParsedPlaylist::Master { variants, .. } => {
                // For master playlist, we need to fetch one of the variant playlists
                // Let's use the first variant for simplicity
                if let Some(first_variant) = variants.first() {
                    // Fetch and parse the variant playlist directly
                    let variant_url = &first_variant.uri;
                    let variant_content = self.fetch_playlist(variant_url).await?;
                    let variant_playlist = self.parse_content(&variant_content, variant_url)?;
                    
                    match variant_playlist {
                        ParsedPlaylist::Media { segments, .. } => {
                            Ok(segments.into_iter().map(|s| s.uri).collect())
                        }
                        _ => Ok(Vec::new())
                    }
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_uri() {
        let parser = M3u8Parser::new();
        
        // Test absolute URL
        assert_eq!(
            parser.resolve_uri("https://example.com/video.ts", "https://base.com/playlist.m3u8"),
            "https://example.com/video.ts"
        );
        
        // Test absolute path
        assert_eq!(
            parser.resolve_uri("/videos/video.ts", "https://example.com/playlist.m3u8"),
            "https://example.com/videos/video.ts"
        );
        
        // Test relative path
        assert_eq!(
            parser.resolve_uri("video.ts", "https://example.com/streams/playlist.m3u8"),
            "https://example.com/streams/video.ts"
        );
    }
}