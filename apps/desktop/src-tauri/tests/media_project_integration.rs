use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

use xakustudio_lib::core::projects::ProjectManager;
use xakustudio_lib::database::AppDatabase;
use xakustudio_lib::media::ffmpeg::probe_media_engine;
use xakustudio_lib::media::preview_server::MediaPreviewServer;

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn create_synthetic_mp4(mp4_path: &PathBuf, srt_path: &PathBuf) {
    tokio::fs::write(srt_path, "1\n00:00:00,000 --> 00:00:01,500\nHello XakuStudio\n")
        .await
        .expect("write synthetic srt");

    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.args([
        "-v",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=320x180:rate=24:duration=2",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=44100:duration=2",
        "-i",
    ])
    .arg(srt_path)
    .args([
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-c:s",
        "mov_text",
    ])
    .arg(mp4_path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let out = cmd.output().await.expect("run ffmpeg synthetic sample");
    assert!(
        out.status.success(),
        "ffmpeg synthetic sample failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn sqlite_projects_ffmpeg_import_and_range_preview() {
    let engine_status = probe_media_engine().await;
    assert!(engine_status.ffmpeg_available, "ffmpeg must be available");
    assert!(engine_status.ffprobe_available, "ffprobe must be available");

    let temp_root = std::env::temp_dir().join(format!("xakustudio_test_{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_root).await.unwrap();
    let _guard = TempDirGuard(temp_root.clone());

    let db = AppDatabase::connect_in_memory()
        .await
        .expect("in-memory sqlite");
    let preview = MediaPreviewServer::start()
        .await
        .expect("start preview server");
    let pm = ProjectManager::new(db.clone(), preview.clone());

    // 1. Create project and verify directory structure + project.xaku
    let projects_parent = temp_root.join("projects");
    let summary = pm
        .create_project(&projects_parent, "Drama Ep 1", "zh", "km", "balanced")
        .await
        .expect("create project");
    let project_id = summary.project.id.clone();
    let project_root = PathBuf::from(&summary.project.root_path);
    assert!(project_root.join("project.xaku").exists());
    assert!(project_root.join("source").is_dir());
    assert!(project_root.join("assets/audio").is_dir());
    assert!(project_root.join("exports").is_dir());

    // 2. Rename project
    let renamed = pm
        .rename_project(&project_id, "Drama Ep 1 (Renamed)")
        .await
        .expect("rename project");
    assert_eq!(renamed.name, "Drama Ep 1 (Renamed)");

    // 3. Import synthetic MP4 non-destructively
    let sample_dir = temp_root.join("input");
    tokio::fs::create_dir_all(&sample_dir).await.unwrap();
    let sample_mp4 = sample_dir.join("ep01.mp4");
    let sample_srt = sample_dir.join("ep01.srt");
    create_synthetic_mp4(&sample_mp4, &sample_srt).await;

    let imported = pm
        .import_media(&project_id, &sample_mp4)
        .await
        .expect("import synthetic mp4");

    assert!(sample_mp4.exists(), "original source must remain untouched");
    assert!(project_root.join("source/ep01.mp4").exists());
    assert_eq!(imported.assets.len(), 4);

    let inspection = imported.inspection.expect("ffprobe inspection present");
    assert_eq!(inspection.video.as_ref().unwrap().width, 320);
    assert_eq!(inspection.video.as_ref().unwrap().height, 180);
    assert!(!inspection.audio_streams.is_empty());
    assert_eq!(inspection.subtitle_streams.len(), 1);

    let peaks = imported.waveform_peaks.expect("waveform peaks computed");
    assert_eq!(peaks.len(), 320);
    assert!(peaks.iter().any(|&p| p > 0.1), "expected non-silent sine peaks");

    // 4. Verify HTTP Range 206 support and unregistered path rejection
    let preview_url = imported.source_preview_url.expect("preview URL");
    let path_part = preview_url
        .split(&format!(":{}", preview.port()))
        .nth(1)
        .unwrap()
        .to_string();

    let mut stream = TcpStream::connect(("127.0.0.1", preview.port()))
        .await
        .unwrap();
    let req = format!(
        "GET {path_part} HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=0-63\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let resp_head = String::from_utf8_lossy(&buf[..buf.len().min(512)]);
    assert!(
        resp_head.starts_with("HTTP/1.1 206 Partial Content"),
        "expected 206 Partial Content, got: {resp_head}"
    );
    assert!(resp_head.contains("Content-Range: bytes 0-63/"));

    // Unregistered asset should 404
    let mut stream404 = TcpStream::connect(("127.0.0.1", preview.port()))
        .await
        .unwrap();
    stream404
        .write_all(b"GET /media/not-registered HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf404 = Vec::new();
    stream404.read_to_end(&mut buf404).await.unwrap();
    assert!(String::from_utf8_lossy(&buf404).starts_with("HTTP/1.1 404 Not Found"));

    // 5. Delete from DB and recover via project.xaku
    pm.delete_project(&project_id, false).await.unwrap();
    assert!(pm.list_projects().await.unwrap().is_empty());

    let recovered = pm
        .open_project_manifest(&project_root.join("project.xaku"))
        .await
        .expect("recover from project.xaku");
    assert_eq!(recovered.project.id, project_id);
    assert_eq!(recovered.project.name, "Drama Ep 1 (Renamed)");
}
