use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use uuid::Uuid;

use xakustudio_lib::core::projects::ProjectManager;
use xakustudio_lib::core::transcription::TranscriptionService;
use xakustudio_lib::database::{AppDatabase, MediaAssetRecord};
use xakustudio_lib::media::preview_server::MediaPreviewServer;
use xakustudio_lib::scheduler::{NewTaskSpec, ResourceClass, TaskScheduler};
use xakustudio_lib::worker::{NoopEventSink, WorkerConfig, WorkerManager};

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn transcription_native_persistence_speakers_edit_merge_split_and_no_secrets_in_db() {
    let temp_root = std::env::temp_dir().join(format!("xakustudio_tx_{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_root).await.unwrap();
    let _guard = TempDirGuard(temp_root.clone());

    let db = AppDatabase::connect_in_memory().await.unwrap();
    let preview = MediaPreviewServer::start().await.unwrap();
    let pm = ProjectManager::new(db.clone(), preview);

    let summary = pm
        .create_project(&temp_root, "Drama Ep 2", "zh", "km", "balanced")
        .await
        .unwrap();
    let project_id = summary.project.id.clone();
    let project_root = PathBuf::from(&summary.project.root_path);

    // Seed an extracted_audio_ai asset so TranscriptionService::start can enqueue DAG.
    let fake_wav = project_root.join("assets/audio/sample_ai_16k.wav");
    tokio::fs::write(&fake_wav, b"RIFF0000WAVE").await.unwrap();
    db.replace_project_media_assets(
        &project_id,
        &[MediaAssetRecord {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.clone(),
            role: "extracted_audio_ai".into(),
            file_path: fake_wav.to_string_lossy().into_owned(),
            sha256: None,
            duration_ms: Some(4000),
            width: None,
            height: None,
            fps_num: None,
            fps_den: None,
            audio_channels: Some(1),
            sample_rate: Some(16_000),
            metadata_json: "{}".into(),
            created_at: String::new(),
        }],
    )
    .await
    .unwrap();

    let worker = WorkerManager::new(WorkerConfig::discover_default(), Arc::new(NoopEventSink));
    let scheduler = TaskScheduler::start(db.clone(), worker, 1, 4, 3, Arc::new(|_| {}))
        .await
        .unwrap();
    let svc = TranscriptionService::new(db.clone(), Arc::clone(&scheduler));
    svc.register_handlers().await;

    // 1. Verify TranscriptionService::start stores credential_keys, NEVER secret values, in SQLite.
    let enqueued = svc.start(&project_id, None).await.unwrap();
    assert_eq!(enqueued.len(), 5);
    let diar_task = enqueued
        .iter()
        .find(|t| t.task_type == "diarization.run")
        .unwrap();
    assert!(diar_task.input_json.contains("credential_keys"));
    assert!(!diar_task.input_json.contains("credentials"));
    for t in &enqueued {
        let _ = scheduler.cancel_task(&t.id).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2. Write synthetic STT, diarization, and voice JSON files and run native handlers.
    let stt_json = temp_root.join("stt.json");
    let diar_json = temp_root.join("diar.json");
    let voice_json = temp_root.join("voice.json");

    tokio::fs::write(
        &stt_json,
        json!({
            "language": "zh",
            "segments": [
                {
                    "index": 0,
                    "start": 0.0,
                    "end": 2.0,
                    "text": "你好世界今天真好",
                    "words": [
                        { "start": 0.0, "end": 0.8, "word": "你好", "probability": 0.9 },
                        { "start": 0.8, "end": 1.4, "word": "世界", "probability": 0.9 },
                        { "start": 1.4, "end": 2.0, "word": "今天真好", "probability": 0.9 }
                    ]
                },
                {
                    "index": 1,
                    "start": 2.1,
                    "end": 4.0,
                    "text": "我也这么觉得",
                    "words": []
                }
            ]
        })
        .to_string(),
    )
    .await
    .unwrap();

    tokio::fs::write(
        &diar_json,
        json!({
            "speakers": ["SPEAKER_00", "SPEAKER_01"],
            "turns": [
                { "start": 0.0, "end": 2.0, "speaker": "SPEAKER_00" },
                { "start": 2.1, "end": 4.0, "speaker": "SPEAKER_01" }
            ]
        })
        .to_string(),
    )
    .await
    .unwrap();

    tokio::fs::write(
        &voice_json,
        json!({
            "speakers": {
                "SPEAKER_00": { "median_f0_hz": 120.0, "voice_presentation": "masculine", "confidence": 0.79 },
                "SPEAKER_01": { "median_f0_hz": 225.0, "voice_presentation": "feminine", "confidence": 0.85 }
            }
        })
        .to_string(),
    )
    .await
    .unwrap();

    let persist_id = Uuid::new_v4().to_string();
    let apply_id = Uuid::new_v4().to_string();
    scheduler
        .enqueue_dag(
            Some(project_id.clone()),
            Some("native-only".into()),
            vec![
                NewTaskSpec {
                    id: persist_id.clone(),
                    task_type: "transcript.persist".into(),
                    resource_class: ResourceClass::CpuLight,
                    input_json: json!({
                        "project_id": project_id,
                        "stt_path": stt_json.to_string_lossy(),
                    }),
                    depends_on: vec![],
                    max_retries: 0,
                },
                NewTaskSpec {
                    id: apply_id.clone(),
                    task_type: "speakers.apply".into(),
                    resource_class: ResourceClass::CpuLight,
                    input_json: json!({
                        "project_id": project_id,
                        "diarization_path": diar_json.to_string_lossy(),
                        "voice_path": voice_json.to_string_lossy(),
                    }),
                    depends_on: vec![persist_id],
                    max_retries: 0,
                },
            ],
        )
        .await
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(t) = db.get_task(&apply_id).await.unwrap() {
            if t.status == "completed" {
                break;
            }
            assert_ne!(t.status, "failed", "speakers.apply failed: {:?}", t.error_message);
        }
        assert!(Instant::now() < deadline, "native persistence timed out");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let payload = svc.get_transcript(&project_id).await.unwrap();
    assert_eq!(payload.segments.len(), 2);
    assert_eq!(payload.speakers.len(), 2);
    assert_eq!(payload.segments[0].emotion_est, "unestimated");
    assert_eq!(payload.speakers[0].display_name, "Speaker 1");
    assert_eq!(payload.speakers[0].voice_gender_est, "masculine");
    assert_eq!(payload.speakers[1].voice_gender_est, "feminine");
    assert_eq!(
        payload.segments[0].speaker_id.as_deref(),
        Some(payload.speakers[0].id.as_str())
    );

    // 3. Text edit preserves source_text_original
    let edited = db
        .update_segment_text(&payload.segments[0].id, "你好，世界今天真好")
        .await
        .unwrap();
    assert_eq!(edited.source_text, "你好，世界今天真好");
    assert_eq!(edited.source_text_original, "你好世界今天真好");

    // 4. Split segment at char_index 3 ("你好，" | "世界今天真好")
    let after_split = db
        .split_segment(&payload.segments[0].id, 3)
        .await
        .unwrap();
    assert_eq!(after_split.len(), 3);
    assert_eq!(after_split[0].source_text, "你好，");
    assert_eq!(after_split[1].source_text, "世界今天真好");
    assert!(after_split[0].end_ms > after_split[0].start_ms);
    assert_eq!(after_split[0].end_ms, after_split[1].start_ms);

    // 5. Merge Speaker 2 into Speaker 1
    db.merge_speakers(&payload.speakers[1].id, &payload.speakers[0].id)
        .await
        .unwrap();
    let after_merge = svc.get_transcript(&project_id).await.unwrap();
    assert_eq!(after_merge.speakers.len(), 1);
    assert!(after_merge
        .segments
        .iter()
        .all(|s| s.speaker_id.as_deref() == Some(payload.speakers[0].id.as_str())));
}
