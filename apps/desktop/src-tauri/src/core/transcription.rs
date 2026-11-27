use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::database::{AppDatabase, DialogueSegmentRecord, SpeakerRecord, TaskRecord};
use crate::scheduler::{NativeTaskHandler, NewTaskSpec, ResourceClass, TaskScheduler};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptPayload {
    pub segments: Vec<DialogueSegmentRecord>,
    pub speakers: Vec<SpeakerRecord>,
    pub latest_run_tasks: Vec<TaskRecord>,
}

#[derive(Debug, Deserialize)]
struct SttOutputDoc {
    segments: Vec<SttSegmentItem>,
}

#[derive(Debug, Deserialize)]
struct SttSegmentItem {
    index: i64,
    start: f64,
    end: f64,
    text: String,
    #[serde(default)]
    words: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct DiarizationDoc {
    speakers: Vec<String>,
    turns: Vec<DiarizationTurn>,
}

#[derive(Debug, Deserialize)]
struct DiarizationTurn {
    start: f64,
    end: f64,
    speaker: String,
}

#[derive(Debug, Deserialize)]
struct VoiceAnalyzeDoc {
    #[serde(default)]
    speakers: HashMap<String, VoiceSpeakerInfo>,
}

#[derive(Debug, Deserialize)]
struct VoiceSpeakerInfo {
    median_f0_hz: Option<f64>,
    #[serde(default = "default_uncertain")]
    voice_presentation: String,
    #[serde(default)]
    confidence: f64,
}

fn default_uncertain() -> String {
    "uncertain".to_string()
}

/// Assign each dialogue segment to the speaker label with greatest overlap (>= 20% of segment duration).
pub fn assign_speakers(
    segments: &[(i64, i64)],
    turns: &[(i64, i64, String)],
) -> Vec<Option<String>> {
    segments
        .iter()
        .map(|&(seg_start, seg_end)| {
            let duration = seg_end - seg_start;
            if duration <= 0 || turns.is_empty() {
                return None;
            }

            let mut overlaps: HashMap<&str, i64> = HashMap::new();
            for (t_start, t_end, label) in turns {
                let ov_start = seg_start.max(*t_start);
                let ov_end = seg_end.min(*t_end);
                if ov_end > ov_start {
                    *overlaps.entry(label.as_str()).or_insert(0) += ov_end - ov_start;
                }
            }

            let mut best_label: Option<&str> = None;
            let mut best_overlap: i64 = 0;
            for (label, ov) in overlaps {
                if ov > best_overlap
                    || (ov == best_overlap && best_label.is_some_and(|cur| label < cur))
                {
                    best_overlap = ov;
                    best_label = Some(label);
                }
            }

            // Require at least 20% overlap (`best_overlap * 5 >= duration`).
            match best_label {
                Some(lbl) if best_overlap * 5 >= duration => Some(lbl.to_string()),
                _ => None,
            }
        })
        .collect()
}

pub fn preset_to_whisper_model(preset: &str) -> &'static str {
    match preset {
        "performance" => "small",
        "quality" => "large-v3-turbo",
        _ => "medium",
    }
}

pub struct TranscriptionService {
    db: AppDatabase,
    scheduler: Arc<TaskScheduler>,
}

impl TranscriptionService {
    pub fn new(db: AppDatabase, scheduler: Arc<TaskScheduler>) -> Arc<Self> {
        Arc::new(Self { db, scheduler })
    }

    pub async fn register_handlers(self: &Arc<Self>) {
        let db_persist = self.db.clone();
        let persist_handler: NativeTaskHandler = Arc::new(move |task| {
            let db = db_persist.clone();
            Box::pin(async move { handle_persist_transcript(&db, &task).await })
        });
        self.scheduler
            .register_native_handler("transcript.persist", persist_handler)
            .await;

        let db_speakers = self.db.clone();
        let speakers_handler: NativeTaskHandler = Arc::new(move |task| {
            let db = db_speakers.clone();
            Box::pin(async move { handle_apply_speakers(&db, &task).await })
        });
        self.scheduler
            .register_native_handler("speakers.apply", speakers_handler)
            .await;
    }

    pub async fn start(
        &self,
        project_id: &str,
        model_size_override: Option<String>,
    ) -> Result<Vec<TaskRecord>, String> {
        let project = self
            .db
            .get_project(project_id)
            .await?
            .ok_or_else(|| format!("Project '{project_id}' not found"))?;

        if self.db.has_active_project_tasks(project_id).await? {
            return Err("A pipeline run is already queued or running for this project".to_string());
        }

        let assets = self.db.list_media_assets(project_id).await?;
        let ai_audio = assets
            .iter()
            .find(|a| a.role == "extracted_audio_ai")
            .ok_or_else(|| "Import media with an audio stream first".to_string())?;

        let run_id = Uuid::new_v4().to_string();
        let transcripts_dir = Path::new(&project.root_path).join("generated/transcripts");
        tokio::fs::create_dir_all(&transcripts_dir)
            .await
            .map_err(|e| format!("Failed to create generated/transcripts directory: {e}"))?;

        let stt_path = transcripts_dir.join(format!("{run_id}_stt.json"));
        let diar_path = transcripts_dir.join(format!("{run_id}_diarization.json"));
        let voice_path = transcripts_dir.join(format!("{run_id}_voice.json"));

        let model_size = model_size_override
            .filter(|s| !s.trim().is_empty() && s != "auto")
            .unwrap_or_else(|| preset_to_whisper_model(&project.preset).to_string());

        let stt_id = Uuid::new_v4().to_string();
        let diar_id = Uuid::new_v4().to_string();
        let voice_id = Uuid::new_v4().to_string();
        let persist_id = Uuid::new_v4().to_string();
        let apply_speakers_id = Uuid::new_v4().to_string();

        let nodes = vec![
            NewTaskSpec {
                id: stt_id.clone(),
                task_type: "stt.transcribe".to_string(),
                resource_class: ResourceClass::GpuHeavy,
                input_json: json!({
                    "audio_path": ai_audio.file_path,
                    "output_path": stt_path.to_string_lossy(),
                    "language": project.source_lang,
                    "model_size": model_size,
                    "device": "auto",
                    "unload_after": true,
                }),
                depends_on: vec![],
                max_retries: 1,
            },
            NewTaskSpec {
                id: diar_id.clone(),
                task_type: "diarization.run".to_string(),
                resource_class: ResourceClass::GpuHeavy,
                input_json: json!({
                    "audio_path": ai_audio.file_path,
                    "output_path": diar_path.to_string_lossy(),
                    "device": "auto",
                    "credential_keys": ["HF_TOKEN"],
                }),
                depends_on: vec![],
                max_retries: 0,
            },
            NewTaskSpec {
                id: voice_id.clone(),
                task_type: "voice.analyze".to_string(),
                resource_class: ResourceClass::CpuLight,
                input_json: json!({
                    "audio_path": ai_audio.file_path,
                    "diarization_path": diar_path.to_string_lossy(),
                    "output_path": voice_path.to_string_lossy(),
                }),
                depends_on: vec![diar_id],
                max_retries: 1,
            },
            NewTaskSpec {
                id: persist_id.clone(),
                task_type: "transcript.persist".to_string(),
                resource_class: ResourceClass::CpuLight,
                input_json: json!({
                    "project_id": project.id,
                    "stt_path": stt_path.to_string_lossy(),
                }),
                depends_on: vec![stt_id],
                max_retries: 0,
            },
            NewTaskSpec {
                id: apply_speakers_id,
                task_type: "speakers.apply".to_string(),
                resource_class: ResourceClass::CpuLight,
                input_json: json!({
                    "project_id": project.id,
                    "diarization_path": diar_path.to_string_lossy(),
                    "voice_path": voice_path.to_string_lossy(),
                }),
                depends_on: vec![persist_id, voice_id],
                max_retries: 0,
            },
        ];

        self.scheduler
            .enqueue_dag(Some(project_id.to_string()), Some(run_id), nodes)
            .await
    }

    pub async fn get_transcript(&self, project_id: &str) -> Result<TranscriptPayload, String> {
        let segments = self.db.list_dialogue_segments(project_id).await?;
        let speakers = self.db.list_speakers(project_id).await?;
        let latest_run_tasks = self.db.list_latest_run_tasks(project_id).await?;
        Ok(TranscriptPayload {
            segments,
            speakers,
            latest_run_tasks,
        })
    }
}

async fn handle_persist_transcript(
    db: &AppDatabase,
    task: &TaskRecord,
) -> Result<serde_json::Value, (String, String)> {
    let input: serde_json::Value = serde_json::from_str(&task.input_json)
        .map_err(|e| ("InvalidInput".to_string(), format!("Bad input_json: {e}")))?;

    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ("InvalidInput".to_string(), "Missing project_id".to_string()))?;
    let stt_path = input
        .get("stt_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ("InvalidInput".to_string(), "Missing stt_path".to_string()))?;

    let raw = tokio::fs::read_to_string(stt_path)
        .await
        .map_err(|e| ("FileReadError".to_string(), format!("Failed to read STT JSON: {e}")))?;
    let doc: SttOutputDoc = serde_json::from_str(&raw)
        .map_err(|e| ("ParseError".to_string(), format!("Invalid STT JSON: {e}")))?;

    let records: Vec<DialogueSegmentRecord> = doc
        .segments
        .into_iter()
        .enumerate()
        .map(|(idx, seg)| {
            let start_ms = (seg.start * 1000.0).round() as i64;
            let end_ms = ((seg.end * 1000.0).round() as i64).max(start_ms + 1);
            let words_json = if seg.words.is_array() {
                seg.words.to_string()
            } else {
                "[]".to_string()
            };
            DialogueSegmentRecord {
                id: Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                segment_index: if seg.index >= 0 { seg.index } else { idx as i64 },
                speaker_id: None,
                start_ms,
                end_ms,
                source_text: seg.text.clone(),
                source_text_original: seg.text,
                words_json,
                emotion_est: "unestimated".to_string(),
                emotion_confidence: 0.0,
                translated_text: String::new(),
                translated_text_auto: String::new(),
                translation_status: "pending".to_string(),
                tts_provider_id: None,
                tts_voice_id: None,
                tts_audio_path: None,
                tts_duration_ms: None,
                sync_mode: "fit_tempo".to_string(),
                speed_ratio: 1.0,
                gain_db: 0.0,
                updated_at: String::new(),
            }
        })
        .collect();

    let count = records.len();
    db.replace_dialogue_segments(project_id, &records)
        .await
        .map_err(|e| ("DatabaseError".to_string(), e))?;

    Ok(json!({ "segment_count": count }))
}

async fn handle_apply_speakers(
    db: &AppDatabase,
    task: &TaskRecord,
) -> Result<serde_json::Value, (String, String)> {
    let input: serde_json::Value = serde_json::from_str(&task.input_json)
        .map_err(|e| ("InvalidInput".to_string(), format!("Bad input_json: {e}")))?;

    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ("InvalidInput".to_string(), "Missing project_id".to_string()))?;
    let diar_path = input
        .get("diarization_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                "InvalidInput".to_string(),
                "Missing diarization_path".to_string(),
            )
        })?;
    let voice_path = input
        .get("voice_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ("InvalidInput".to_string(), "Missing voice_path".to_string()))?;

    let diar_raw = tokio::fs::read_to_string(diar_path)
        .await
        .map_err(|e| ("FileReadError".to_string(), format!("Failed to read diarization JSON: {e}")))?;
    let diar: DiarizationDoc = serde_json::from_str(&diar_raw)
        .map_err(|e| ("ParseError".to_string(), format!("Invalid diarization JSON: {e}")))?;

    let voice_doc: VoiceAnalyzeDoc = match tokio::fs::read_to_string(voice_path).await {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or(VoiceAnalyzeDoc {
            speakers: HashMap::new(),
        }),
        Err(_) => VoiceAnalyzeDoc {
            speakers: HashMap::new(),
        },
    };

    let mut label_to_id: HashMap<String, String> = HashMap::new();
    let mut speaker_records: Vec<SpeakerRecord> = Vec::new();
    let mut ordered_labels: Vec<String> = diar.speakers.clone();
    for t in &diar.turns {
        if !ordered_labels.contains(&t.speaker) {
            ordered_labels.push(t.speaker.clone());
        }
    }

    for (idx, label) in ordered_labels.into_iter().enumerate() {
        let sp_id = Uuid::new_v4().to_string();
        let voice = voice_doc.speakers.get(&label);
        speaker_records.push(SpeakerRecord {
            id: sp_id.clone(),
            project_id: project_id.to_string(),
            diarization_label: label.clone(),
            display_name: format!("Speaker {}", idx + 1),
            voice_gender_est: voice
                .map(|v| v.voice_presentation.clone())
                .unwrap_or_else(default_uncertain),
            gender_confidence: voice.map(|v| v.confidence).unwrap_or(0.0),
            pitch_hz_avg: voice.and_then(|v| v.median_f0_hz),
            voice_provider_id: None,
            voice_id: None,
            voice_settings_json: "{}".to_string(),
            is_user_overridden: 0,
            created_at: String::new(),
            updated_at: String::new(),
        });
        label_to_id.insert(label, sp_id);
    }

    let segments = db
        .list_dialogue_segments(project_id)
        .await
        .map_err(|e| ("DatabaseError".to_string(), e))?;
    let seg_windows: Vec<(i64, i64)> = segments.iter().map(|s| (s.start_ms, s.end_ms)).collect();
    let turn_windows: Vec<(i64, i64, String)> = diar
        .turns
        .iter()
        .map(|t| {
            (
                (t.start * 1000.0).round() as i64,
                (t.end * 1000.0).round() as i64,
                t.speaker.clone(),
            )
        })
        .collect();

    let assigned_labels = assign_speakers(&seg_windows, &turn_windows);
    let assignments: Vec<(String, Option<String>)> = segments
        .iter()
        .zip(assigned_labels.into_iter())
        .map(|(seg, maybe_lbl)| {
            let sp_id = maybe_lbl.and_then(|lbl| label_to_id.get(&lbl).cloned());
            (seg.id.clone(), sp_id)
        })
        .collect();

    let speaker_count = speaker_records.len();
    db.replace_speakers_and_assign(project_id, &speaker_records, &assignments)
        .await
        .map_err(|e| ("DatabaseError".to_string(), e))?;

    Ok(json!({
        "speaker_count": speaker_count,
        "assigned_segments": assignments.iter().filter(|(_, s)| s.is_some()).count(),
    }))
}

#[cfg(test)]
mod tests {
    use super::assign_speakers;

    #[test]
    fn assign_speakers_handles_clear_winner_insufficient_overlap_and_empty_turns() {
        let segments = vec![(0, 1000), (1000, 2000), (2000, 3000)];
        let turns = vec![
            (0, 800, "SPEAKER_00".to_string()),
            (800, 1000, "SPEAKER_01".to_string()),
            // Only 150ms overlap on [1000, 2000] -> < 20%
            (1000, 1150, "SPEAKER_01".to_string()),
        ];

        let assigned = assign_speakers(&segments, &turns);
        assert_eq!(assigned[0].as_deref(), Some("SPEAKER_00"));
        assert_eq!(assigned[1], None);
        assert_eq!(assigned[2], None);

        assert_eq!(assign_speakers(&segments, &[]), vec![None, None, None]);
    }
}
