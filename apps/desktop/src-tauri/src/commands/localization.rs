use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::core::localization::{LocalizationService, VoicePreset, VoicePreviewResult};
use crate::database::{
    AppDatabase, DialogueSegmentRecord, GlossaryEntryRecord, SpeakerRecord, TaskRecord,
};

#[tauri::command]
pub async fn start_translation(
    service: State<'_, Arc<LocalizationService>>,
    project_id: String,
    segment_ids: Option<Vec<String>>,
    scene_context: Option<String>,
    model: Option<String>,
) -> Result<Vec<TaskRecord>, String> {
    service
        .start_translation(
            &project_id,
            segment_ids,
            scene_context.unwrap_or_default(),
            model,
        )
        .await
}

#[tauri::command]
pub async fn update_segment_translation(
    db: State<'_, AppDatabase>,
    segment_id: String,
    translated_text: String,
) -> Result<DialogueSegmentRecord, String> {
    db.update_segment_translation(&segment_id, &translated_text)
        .await
}

#[tauri::command]
pub async fn list_glossary(
    db: State<'_, AppDatabase>,
    project_id: Option<String>,
) -> Result<Vec<GlossaryEntryRecord>, String> {
    db.list_glossary(project_id.as_deref()).await
}

#[tauri::command]
pub async fn upsert_glossary_entry(
    db: State<'_, AppDatabase>,
    id: Option<String>,
    project_id: Option<String>,
    source_term: String,
    target_term: String,
    notes: Option<String>,
) -> Result<GlossaryEntryRecord, String> {
    let src = source_term.trim();
    let tgt = target_term.trim();
    if src.is_empty() || tgt.is_empty() {
        return Err("Source term and target term cannot be empty".to_string());
    }
    let entry = GlossaryEntryRecord {
        id: id.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| Uuid::new_v4().to_string()),
        project_id,
        source_term: src.to_string(),
        target_term: tgt.to_string(),
        notes: notes.unwrap_or_default(),
    };
    db.upsert_glossary_entry(&entry).await?;
    Ok(entry)
}

#[tauri::command]
pub async fn delete_glossary_entry(db: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    db.delete_glossary_entry(&id).await
}

#[tauri::command]
pub async fn list_tts_voices(
    service: State<'_, Arc<LocalizationService>>,
) -> Result<Vec<VoicePreset>, String> {
    service.list_tts_voices().await
}

#[tauri::command]
pub async fn assign_speaker_voice(
    db: State<'_, AppDatabase>,
    speaker_id: String,
    voice_provider_id: String,
    voice_id: String,
    voice_settings_json: String,
) -> Result<SpeakerRecord, String> {
    db.assign_speaker_voice(
        &speaker_id,
        &voice_provider_id,
        &voice_id,
        &voice_settings_json,
    )
    .await
}

#[tauri::command]
pub async fn preview_tts_voice(
    service: State<'_, Arc<LocalizationService>>,
    text: String,
    voice_id: String,
    speed_ratio: Option<f64>,
    pitch_hz_delta: Option<i64>,
) -> Result<VoicePreviewResult, String> {
    service
        .preview_voice(
            text,
            voice_id,
            speed_ratio.unwrap_or(1.0),
            pitch_hz_delta.unwrap_or(0),
        )
        .await
}

#[tauri::command]
pub async fn synthesize_project_tts(
    service: State<'_, Arc<LocalizationService>>,
    project_id: String,
) -> Result<Vec<TaskRecord>, String> {
    service.synthesize_project_tts(&project_id).await
}

#[tauri::command]
pub async fn synthesize_segment_tts(
    service: State<'_, Arc<LocalizationService>>,
    segment_id: String,
) -> Result<DialogueSegmentRecord, String> {
    service.synthesize_segment_tts(&segment_id).await
}

#[tauri::command]
pub async fn get_segment_audio_urls(
    service: State<'_, Arc<LocalizationService>>,
    project_id: String,
) -> Result<HashMap<String, String>, String> {
    service.get_segment_audio_urls(&project_id).await
}
