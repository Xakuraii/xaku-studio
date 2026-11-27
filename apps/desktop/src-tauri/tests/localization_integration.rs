use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use uuid::Uuid;

use xakustudio_lib::core::localization::LocalizationService;
use xakustudio_lib::core::projects::ProjectManager;
use xakustudio_lib::database::{
    AppDatabase, DialogueSegmentRecord, GlossaryEntryRecord, SpeakerRecord,
};
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
async fn glossary_translation_persistence_speaker_voice_and_tts_audio_urls() {
    let temp_root = std::env::temp_dir().join(format!("xakustudio_loc_{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_root).await.unwrap();
    let _guard = TempDirGuard(temp_root.clone());

    let db = AppDatabase::connect_in_memory().await.unwrap();
    let preview = MediaPreviewServer::start().await.unwrap();
    let pm = ProjectManager::new(db.clone(), preview.clone());

    let summary = pm
        .create_project(&temp_root, "Drama Ep 3", "zh", "km", "balanced")
        .await
        .unwrap();
    let project_id = summary.project.id.clone();

    let worker = WorkerManager::new(WorkerConfig::discover_default(), Arc::new(NoopEventSink));
    let scheduler = TaskScheduler::start(db.clone(), Arc::clone(&worker), 1, 4, 3, Arc::new(|_| {}))
        .await
        .unwrap();
    let loc = LocalizationService::new(
        db.clone(),
        Arc::clone(&scheduler),
        worker,
        preview.clone(),
        temp_root.join("cache"),
    );
    loc.register_handlers().await;

    // 1. Glossary CRUD (global + project)
    let global_entry = GlossaryEntryRecord {
        id: Uuid::new_v4().to_string(),
        project_id: None,
        source_term: "总裁".into(),
        target_term: "លោកប្រធាន".into(),
        notes: "Global honorific".into(),
    };
    let proj_entry = GlossaryEntryRecord {
        id: Uuid::new_v4().to_string(),
        project_id: Some(project_id.clone()),
        source_term: "阿林".into(),
        target_term: "អាលីន".into(),
        notes: "Lead character".into(),
    };
    db.upsert_glossary_entry(&global_entry).await.unwrap();
    db.upsert_glossary_entry(&proj_entry).await.unwrap();
    assert_eq!(db.list_glossary(Some(&project_id)).await.unwrap().len(), 2);
    assert_eq!(db.list_glossary(None).await.unwrap().len(), 1);

    // 2. Seed speaker & segment, run translation.persist native handler
    let speaker_id = Uuid::new_v4().to_string();
    let seg_id = Uuid::new_v4().to_string();
    db.replace_dialogue_segments(
        &project_id,
        &[DialogueSegmentRecord {
            id: seg_id.clone(),
            project_id: project_id.clone(),
            segment_index: 0,
            speaker_id: None,
            start_ms: 0,
            end_ms: 1500,
            source_text: "总裁好".into(),
            source_text_original: "总裁好".into(),
            words_json: "[]".into(),
            emotion_est: "unestimated".into(),
            emotion_confidence: 0.0,
            translated_text: String::new(),
            translated_text_auto: String::new(),
            translation_status: "pending".into(),
            tts_provider_id: None,
            tts_voice_id: None,
            tts_audio_path: None,
            tts_duration_ms: None,
            sync_mode: "fit_tempo".into(),
            speed_ratio: 1.0,
            gain_db: 0.0,
            updated_at: String::new(),
        }],
    )
    .await
    .unwrap();

    db.replace_speakers_and_assign(
        &project_id,
        &[SpeakerRecord {
            id: speaker_id.clone(),
            project_id: project_id.clone(),
            diarization_label: "SPEAKER_00".into(),
            display_name: "Speaker 1".into(),
            voice_gender_est: "feminine".into(),
            gender_confidence: 0.8,
            pitch_hz_avg: Some(220.0),
            voice_provider_id: None,
            voice_id: None,
            voice_settings_json: "{}".into(),
            is_user_overridden: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }],
        &[(seg_id.clone(), Some(speaker_id.clone()))],
    )
    .await
    .unwrap();

    // Verify start_translation stores credential_keys without any API secret.
    let trans_tasks = loc
        .start_translation(&project_id, None, "Office greeting".into(), None)
        .await
        .unwrap();
    assert_eq!(trans_tasks.len(), 2);
    assert!(trans_tasks[0].input_json.contains("GEMINI_API_KEY"));
    assert!(!trans_tasks[0].input_json.contains("\"credentials\""));
    for t in &trans_tasks {
        let _ = scheduler.cancel_task(&t.id).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let trans_json = temp_root.join("trans.json");
    tokio::fs::write(
        &trans_json,
        json!({
            "translations": [
                { "segment_id": seg_id, "translated_text": "សួស្តីលោកប្រធាន" }
            ]
        })
        .to_string(),
    )
    .await
    .unwrap();

    let persist_id = Uuid::new_v4().to_string();
    scheduler
        .enqueue_dag(
            Some(project_id.clone()),
            Some("trans-native".into()),
            vec![NewTaskSpec {
                id: persist_id.clone(),
                task_type: "translation.persist".into(),
                resource_class: ResourceClass::CpuLight,
                input_json: json!({
                    "project_id": project_id,
                    "translation_path": trans_json.to_string_lossy(),
                }),
                depends_on: vec![],
                max_retries: 0,
            }],
        )
        .await
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(t) = db.get_task(&persist_id).await.unwrap() {
            if t.status == "completed" {
                break;
            }
            assert_ne!(t.status, "failed", "translation.persist failed: {:?}", t.error_message);
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let segs = db.list_dialogue_segments(&project_id).await.unwrap();
    assert_eq!(segs[0].translated_text, "សួស្តីលោកប្រធាន");
    assert_eq!(segs[0].translated_text_auto, "សួស្តីលោកប្រធាន");
    assert_eq!(segs[0].translation_status, "auto");

    let edited = db
        .update_segment_translation(&seg_id, "ជម្រាបសួរលោកប្រធាន")
        .await
        .unwrap();
    assert_eq!(edited.translated_text, "ជម្រាបសួរលោកប្រធាន");
    assert_eq!(edited.translated_text_auto, "សួស្តីលោកប្រធាន");
    assert_eq!(edited.translation_status, "edited");

    // 3. Assign speaker voice and register segment TTS audio URL
    let sp = db
        .assign_speaker_voice(
            &speaker_id,
            "edge-neural-tts",
            "km-KH-SreymomNeural",
            r#"{"speed_ratio":1.05,"pitch_hz_delta":0}"#,
        )
        .await
        .unwrap();
    assert_eq!(sp.voice_id.as_deref(), Some("km-KH-SreymomNeural"));

    let wav_file = temp_root.join("line1.wav");
    tokio::fs::write(&wav_file, b"RIFF0000WAVEfmt ").await.unwrap();
    db.update_segment_tts_output(
        &seg_id,
        "edge-neural-tts",
        "km-KH-SreymomNeural",
        &wav_file.to_string_lossy(),
        1250,
    )
    .await
    .unwrap();

    let urls = loc.get_segment_audio_urls(&project_id).await.unwrap();
    assert!(urls.contains_key(&seg_id));
}
