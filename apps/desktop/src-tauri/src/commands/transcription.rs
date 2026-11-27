use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::core::transcription::{TranscriptPayload, TranscriptionService};
use crate::database::{AppDatabase, DialogueSegmentRecord, SpeakerRecord, TaskRecord};

#[tauri::command]
pub async fn start_transcription(
    service: State<'_, Arc<TranscriptionService>>,
    project_id: String,
    model_size: Option<String>,
) -> Result<Vec<TaskRecord>, String> {
    service.start(&project_id, model_size).await
}

#[tauri::command]
pub async fn get_transcript(
    service: State<'_, Arc<TranscriptionService>>,
    project_id: String,
) -> Result<TranscriptPayload, String> {
    service.get_transcript(&project_id).await
}

#[tauri::command]
pub async fn update_segment_text(
    db: State<'_, AppDatabase>,
    segment_id: String,
    source_text: String,
) -> Result<DialogueSegmentRecord, String> {
    db.update_segment_text(&segment_id, &source_text).await
}

#[tauri::command]
pub async fn update_segment_speaker(
    db: State<'_, AppDatabase>,
    segment_id: String,
    speaker_id: Option<String>,
) -> Result<DialogueSegmentRecord, String> {
    db.update_segment_speaker(&segment_id, speaker_id.as_deref())
        .await
}

#[tauri::command]
pub async fn create_speaker(
    db: State<'_, AppDatabase>,
    project_id: String,
    display_name: String,
) -> Result<SpeakerRecord, String> {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        return Err("Speaker name cannot be empty".to_string());
    }
    let record = SpeakerRecord {
        id: Uuid::new_v4().to_string(),
        project_id,
        diarization_label: format!("MANUAL_{}", &Uuid::new_v4().to_string()[..6]),
        display_name: trimmed.to_string(),
        voice_gender_est: "uncertain".to_string(),
        gender_confidence: 0.0,
        pitch_hz_avg: None,
        voice_provider_id: None,
        voice_id: None,
        voice_settings_json: "{}".to_string(),
        is_user_overridden: 1,
        created_at: String::new(),
        updated_at: String::new(),
    };
    db.insert_speaker(&record).await?;
    Ok(record)
}

#[tauri::command]
pub async fn rename_speaker(
    db: State<'_, AppDatabase>,
    speaker_id: String,
    display_name: String,
) -> Result<SpeakerRecord, String> {
    db.rename_speaker(&speaker_id, &display_name).await
}

#[tauri::command]
pub async fn set_speaker_voice_presentation(
    db: State<'_, AppDatabase>,
    speaker_id: String,
    voice_gender_est: String,
) -> Result<SpeakerRecord, String> {
    db.set_speaker_voice_presentation(&speaker_id, &voice_gender_est)
        .await
}

#[tauri::command]
pub async fn merge_speakers(
    db: State<'_, AppDatabase>,
    source_speaker_id: String,
    target_speaker_id: String,
) -> Result<(), String> {
    db.merge_speakers(&source_speaker_id, &target_speaker_id)
        .await
}

#[tauri::command]
pub async fn split_segment(
    db: State<'_, AppDatabase>,
    segment_id: String,
    char_index: usize,
) -> Result<Vec<DialogueSegmentRecord>, String> {
    db.split_segment(&segment_id, char_index).await
}
