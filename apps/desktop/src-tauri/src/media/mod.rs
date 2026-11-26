pub mod ffmpeg;
pub mod ffprobe;
pub mod preview_server;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaEngineStatus {
    pub ffmpeg_available: bool,
    pub ffprobe_available: bool,
    pub ffmpeg_version: Option<String>,
    pub has_nvenc: bool,
    pub has_libx264: bool,
    pub has_libass: bool,
    pub has_rubberband: bool,
    pub error: Option<String>,
}
