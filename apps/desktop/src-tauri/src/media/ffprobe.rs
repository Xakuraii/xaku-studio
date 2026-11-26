use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoStreamInfo {
    pub index: i64,
    pub codec: String,
    pub width: i64,
    pub height: i64,
    pub fps_num: i64,
    pub fps_den: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStreamInfo {
    pub index: i64,
    pub codec: String,
    pub channels: i64,
    pub sample_rate: i64,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleStreamInfo {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInspection {
    pub format_name: String,
    pub duration_ms: i64,
    pub size_bytes: u64,
    pub video: Option<VideoStreamInfo>,
    pub audio_streams: Vec<AudioStreamInfo>,
    pub subtitle_streams: Vec<SubtitleStreamInfo>,
}

fn parse_fraction(raw: Option<&str>) -> (i64, i64) {
    let Some(text) = raw else {
        return (0, 1);
    };
    if let Some((n, d)) = text.split_once('/') {
        let num = n.trim().parse::<i64>().unwrap_or(0);
        let den = d.trim().parse::<i64>().unwrap_or(1).max(1);
        (num, den)
    } else {
        (text.trim().parse::<i64>().unwrap_or(0), 1)
    }
}

pub async fn inspect_media(path: &Path) -> Result<MediaInspection, String> {
    if !path.exists() {
        return Err(format!("Media file does not exist: {}", path.display()));
    }

    let mut cmd = tokio::process::Command::new("ffprobe");
    cmd.arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to invoke ffprobe: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ffprobe exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe JSON: {e}"))?;

    let format = json.get("format").cloned().unwrap_or(Value::Null);
    let format_name = format
        .get("format_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let duration_sec = format
        .get("duration")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let duration_ms = (duration_sec * 1000.0).round() as i64;
    let size_bytes = format
        .get("size")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let streams = json
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut video: Option<VideoStreamInfo> = None;
    let mut audio_streams = Vec::new();
    let mut subtitle_streams = Vec::new();

    for s in streams {
        let codec_type = s.get("codec_type").and_then(Value::as_str).unwrap_or("");
        let index = s.get("index").and_then(Value::as_i64).unwrap_or(0);
        let codec = s
            .get("codec_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let tags = s.get("tags");
        let language = tags
            .and_then(|t| t.get("language"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let title = tags
            .and_then(|t| t.get("title"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        match codec_type {
            "video" if video.is_none() => {
                let width = s.get("width").and_then(Value::as_i64).unwrap_or(0);
                let height = s.get("height").and_then(Value::as_i64).unwrap_or(0);
                let (fps_num, fps_den) =
                    parse_fraction(s.get("avg_frame_rate").and_then(Value::as_str).or_else(|| {
                        s.get("r_frame_rate").and_then(Value::as_str)
                    }));
                video = Some(VideoStreamInfo {
                    index,
                    codec,
                    width,
                    height,
                    fps_num,
                    fps_den,
                });
            }
            "audio" => {
                let channels = s.get("channels").and_then(Value::as_i64).unwrap_or(0);
                let sample_rate = s
                    .get("sample_rate")
                    .and_then(Value::as_str)
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                audio_streams.push(AudioStreamInfo {
                    index,
                    codec,
                    channels,
                    sample_rate,
                    language,
                });
            }
            "subtitle" => {
                subtitle_streams.push(SubtitleStreamInfo {
                    index,
                    codec,
                    language,
                    title,
                });
            }
            _ => {}
        }
    }

    Ok(MediaInspection {
        format_name,
        duration_ms,
        size_bytes,
        video,
        audio_streams,
        subtitle_streams,
    })
}
