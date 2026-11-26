use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::warn;

#[derive(Clone)]
pub struct MediaPreviewServer {
    port: u16,
    assets: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl MediaPreviewServer {
    pub async fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind local media preview server: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("Failed to inspect media server address: {e}"))?
            .port();

        let assets = Arc::new(RwLock::new(HashMap::new()));
        let assets_clone = Arc::clone(&assets);

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let map = Arc::clone(&assets_clone);
                        tokio::spawn(async move {
                            if let Err(err) = handle_connection(stream, map).await {
                                warn!("Media preview request error: {err}");
                            }
                        });
                    }
                    Err(err) => {
                        warn!("Media preview accept error: {err}");
                        break;
                    }
                }
            }
        });

        Ok(Self { port, assets })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn register_asset(&self, asset_id: impl Into<String>, path: PathBuf) {
        let mut guard = self.assets.write().await;
        guard.insert(asset_id.into(), path);
    }

    pub fn url_for(&self, asset_id: &str) -> String {
        format!("http://127.0.0.1:{}/media/{}", self.port, asset_id)
    }
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("mov") => "video/quicktime",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("m4a") | Some("aac") => "audio/mp4",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

fn parse_range_header(header: &str, total_len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.trim();
    let (start_str, end_str) = spec.split_once('-')?;
    if total_len == 0 {
        return None;
    }
    if start_str.is_empty() {
        let suffix: u64 = end_str.parse().ok()?;
        let start = total_len.saturating_sub(suffix);
        return Some((start, total_len - 1));
    }
    let start: u64 = start_str.parse().ok()?;
    let end: u64 = if end_str.is_empty() {
        total_len - 1
    } else {
        end_str.parse::<u64>().ok()?.min(total_len - 1)
    };
    if start <= end && start < total_len {
        Some((start, end))
    } else {
        None
    }
}

async fn handle_connection(
    stream: TcpStream,
    assets: Arc<RwLock<HashMap<String, PathBuf>>>,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);

    let mut request_line = String::new();
    let bytes_read = buf_reader
        .read_line(&mut request_line)
        .await
        .map_err(|e| format!("Read request line failed: {e}"))?;
    if bytes_read == 0 {
        return Ok(());
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    let mut range_header: Option<String> = None;
    loop {
        let mut line = String::new();
        let n = buf_reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("Read header failed: {e}"))?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, val)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("range") {
                range_header = Some(val.trim().to_string());
            }
        }
    }

    if method != "GET" && method != "HEAD" {
        writer
            .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    let Some(asset_id) = target.strip_prefix("/media/") else {
        writer
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    };
    let asset_id = asset_id.split('?').next().unwrap_or(asset_id);

    let path = {
        let guard = assets.read().await;
        guard.get(asset_id).cloned()
    };

    let Some(file_path) = path else {
        writer
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    };

    let mut file = match tokio::fs::File::open(&file_path).await {
        Ok(f) => f,
        Err(_) => {
            writer
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    };

    let meta = file.metadata().await.map_err(|e| e.to_string())?;
    let total_len = meta.len();
    let mime = content_type_for(&file_path);

    let (status_line, start, end, extra_headers) = if let Some(ref rh) = range_header {
        if let Some((s, e)) = parse_range_header(rh, total_len) {
            (
                "HTTP/1.1 206 Partial Content",
                s,
                e,
                format!("Content-Range: bytes {s}-{e}/{total_len}\r\n"),
            )
        } else {
            writer
                .write_all(
                    format!(
                        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total_len}\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    } else {
        (
            "HTTP/1.1 200 OK",
            0,
            total_len.saturating_sub(1),
            String::new(),
        )
    };

    let content_len = if total_len == 0 { 0 } else { end - start + 1 };
    let headers = format!(
        "{status_line}\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Length: {content_len}\r\n{extra_headers}Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n"
    );
    writer
        .write_all(headers.as_bytes())
        .await
        .map_err(|e| e.to_string())?;

    if method == "GET" && content_len > 0 {
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| e.to_string())?;
        let mut limited = file.take(content_len);
        // Client closing connection mid-seek is normal in HTML5 <video>.
        let _ = tokio::io::copy(&mut limited, &mut writer).await;
    }

    let _ = writer.flush().await;
    Ok(())
}
