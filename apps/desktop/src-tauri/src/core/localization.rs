use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::database::{AppDatabase, DialogueSegmentRecord, SpeakerRecord, TaskRecord};
use crate::media::preview_server::MediaPreviewServer;
use crate::scheduler::{NativeTaskHandler, NewTaskSpec, ResourceClass, TaskScheduler};
use crate::worker::WorkerManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicePreset {
    pub provider_id: String,
    pub voice_id: String,
    pub display_name: String,
    pub language: String,
    pub gender_presentation: String,
    pub supports_rate: bool,
    pub supports_pitch: bool,
    pub requires_authorization: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicePreviewResult {
    pub asset_id: String,
    pub preview_url: String,
    pub duration_ms: i64,
    pub voice_id: String,
}

#[derive(Debug, Deserialize)]
struct TranslationOutputDoc {
    translations: Vec<TranslationOutputItem>,
}

#[derive(Debug, Deserialize)]
struct TranslationOutputItem {
    segment_id: String,
    translated_text: String,
}

#[derive(Debug, Deserialize)]
struct TtsBatchPersistInput {
    results_items: Vec<TtsBatchItemResult>,
}

#[derive(Debug, Deserialize)]
struct TtsBatchItemResult {
    segment_id: String,
    output_path: String,
    duration_ms: i64,
    voice_id: String,
    provider_id: String,
}

pub struct LocalizationService {
    db: AppDatabase,
    scheduler: Arc<TaskScheduler>,
    worker: Arc<WorkerManager>,
    preview_server: MediaPreviewServer,
    cache_dir: PathBuf,
}

impl LocalizationService {
    pub fn new(
        db: AppDatabase,
        scheduler: Arc<TaskScheduler>,
        worker: Arc<WorkerManager>,
        preview_server: MediaPreviewServer,
        cache_dir: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            scheduler,
            worker,
            preview_server,
            cache_dir,
        })
    }

    pub async fn register_handlers(self: &Arc<Self>) {
        let db_trans = self.db.clone();
        let trans_persist: NativeTaskHandler = Arc::new(move |task| {
            let db = db_trans.clone();
            Box::pin(async move { handle_persist_translations(&db, &task).await })
        });
        self.scheduler
            .register_native_handler("translation.persist", trans_persist)
            .await;

        let db_tts = self.db.clone();
        let preview_tts = self.preview_server.clone();
        let tts_persist: NativeTaskHandler = Arc::new(move |task| {
            let db = db_tts.clone();
            let preview = preview_tts.clone();
            Box::pin(async move { handle_persist_tts_batch(&db, &preview, &task).await })
        });
        self.scheduler
            .register_native_handler("tts.persist_batch", tts_persist)
            .await;
    }

    pub async fn start_translation(
        &self,
        project_id: &str,
        segment_ids: Option<Vec<String>>,
        scene_context: String,
        model: Option<String>,
    ) -> Result<Vec<TaskRecord>, String> {
        let project = self
            .db
            .get_project(project_id)
            .await?
            .ok_or_else(|| format!("Project '{project_id}' not found"))?;

        if self.db.has_active_project_tasks(project_id).await? {
            return Err("A pipeline run is already queued or running for this project".to_string());
        }

        let all_segments = self.db.list_dialogue_segments(project_id).await?;
        let speakers = self.db.list_speakers(project_id).await?;
        let speaker_map: HashMap<String, &SpeakerRecord> =
            speakers.iter().map(|s| (s.id.clone(), s)).collect();

        let filter_set = segment_ids.map(|ids| ids.into_iter().collect::<std::collections::HashSet<_>>());
        let selected: Vec<Value> = all_segments
            .iter()
            .filter(|seg| {
                !seg.source_text.trim().is_empty()
                    && filter_set
                        .as_ref()
                        .is_none_or(|set| set.contains(&seg.id))
            })
            .map(|seg| {
                let sp = seg.speaker_id.as_ref().and_then(|id| speaker_map.get(id));
                json!({
                    "id": seg.id,
                    "segment_index": seg.segment_index,
                    "speaker_name": sp.map(|s| s.display_name.as_str()).unwrap_or("Speaker"),
                    "voice_presentation": sp.map(|s| s.voice_gender_est.as_str()).unwrap_or("uncertain"),
                    "start_ms": seg.start_ms,
                    "end_ms": seg.end_ms,
                    "source_text": seg.source_text,
                })
            })
            .collect();

        if selected.is_empty() {
            return Err("No transcribed dialogue segments available to translate".to_string());
        }

        let glossary_rows = self.db.list_glossary(Some(project_id)).await?;
        let glossary_json: Vec<Value> = glossary_rows
            .into_iter()
            .map(|g| {
                json!({
                    "source_term": g.source_term,
                    "target_term": g.target_term,
                    "notes": g.notes,
                })
            })
            .collect();

        let run_id = Uuid::new_v4().to_string();
        let trans_dir = Path::new(&project.root_path).join("generated/translations");
        tokio::fs::create_dir_all(&trans_dir)
            .await
            .map_err(|e| format!("Failed to create generated/translations directory: {e}"))?;
        let out_path = trans_dir.join(format!("{run_id}_translation.json"));

        let trans_id = Uuid::new_v4().to_string();
        let persist_id = Uuid::new_v4().to_string();

        let nodes = vec![
            NewTaskSpec {
                id: trans_id.clone(),
                task_type: "translation.run".to_string(),
                resource_class: ResourceClass::NetworkIo,
                input_json: json!({
                    "source_lang": project.source_lang,
                    "target_lang": project.target_lang,
                    "output_path": out_path.to_string_lossy(),
                    "segments": selected,
                    "glossary": glossary_json,
                    "scene_context": scene_context,
                    "model": model.filter(|m| !m.trim().is_empty()).unwrap_or_else(|| "gemini-4-hydrogen".to_string()),
                    "credential_keys": ["GEMINI_API_KEY"],
                }),
                depends_on: vec![],
                max_retries: 1,
            },
            NewTaskSpec {
                id: persist_id,
                task_type: "translation.persist".to_string(),
                resource_class: ResourceClass::CpuLight,
                input_json: json!({
                    "project_id": project.id,
                    "translation_path": out_path.to_string_lossy(),
                }),
                depends_on: vec![trans_id],
                max_retries: 0,
            },
        ];

        self.scheduler
            .enqueue_dag(Some(project_id.to_string()), Some(run_id), nodes)
            .await
    }

    pub async fn list_tts_voices(&self) -> Result<Vec<VoicePreset>, String> {
        let res = self
            .worker
            .call(
                "task.run",
                json!({
                    "task_id": Uuid::new_v4().to_string(),
                    "task_type": "tts.list_voices",
                    "payload": {},
                }),
            )
            .await?;
        let voices_val = res
            .get("output")
            .and_then(|o| o.get("voices"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        serde_json::from_value(voices_val)
            .map_err(|e| format!("Failed to parse TTS voices list: {e}"))
    }

    pub async fn preview_voice(
        &self,
        text: String,
        voice_id: String,
        speed_ratio: f64,
        pitch_hz_delta: i64,
    ) -> Result<VoicePreviewResult, String> {
        tokio::fs::create_dir_all(&self.cache_dir)
            .await
            .map_err(|e| format!("Failed to create cache dir: {e}"))?;
        let asset_id = format!("voice-preview-{}", Uuid::new_v4());
        let out_wav = self.cache_dir.join(format!("{asset_id}.wav"));

        let res = self
            .worker
            .call_rpc_with_timeout(
                "task.run",
                json!({
                    "task_id": Uuid::new_v4().to_string(),
                    "task_type": "tts.synthesize",
                    "payload": {
                        "text": text,
                        "voice_id": voice_id,
                        "output_path": out_wav.to_string_lossy(),
                        "speed_ratio": speed_ratio,
                        "pitch_hz_delta": pitch_hz_delta,
                    }
                }),
                None,
            )
            .await
            .map_err(|e| format!("[{}] {}", e.code, e.message))?;

        let duration_ms = res
            .get("output")
            .and_then(|o| o.get("duration_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(0);

        self.preview_server
            .register_asset(&asset_id, out_wav)
            .await;

        Ok(VoicePreviewResult {
            preview_url: self.preview_server.url_for(&asset_id),
            asset_id,
            duration_ms,
            voice_id,
        })
    }

    pub async fn synthesize_segment_tts(
        &self,
        segment_id: &str,
    ) -> Result<DialogueSegmentRecord, String> {
        let segments_all = sqlx::query_as::<_, DialogueSegmentRecord>(
            "SELECT * FROM dialogue_segments WHERE id = ?1",
        )
        .bind(segment_id)
        .fetch_optional(self.db.pool())
        .await
        .map_err(|e| format!("Failed to load segment: {e}"))?
        .ok_or_else(|| format!("Segment '{segment_id}' not found"))?;

        if segments_all.translated_text.trim().is_empty() {
            return Err("Translate this segment before generating speech".to_string());
        }

        let project = self
            .db
            .get_project(&segments_all.project_id)
            .await?
            .ok_or_else(|| "Project not found".to_string())?;
        let speakers = self.db.list_speakers(&project.id).await?;
        let speaker = segments_all
            .speaker_id
            .as_ref()
            .and_then(|sid| speakers.iter().find(|s| &s.id == sid))
            .ok_or_else(|| "Assign a speaker to this segment first".to_string())?;

        let voice_id = speaker
            .voice_id
            .as_ref()
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| format!("Assign a voice to '{}' first", speaker.display_name))?
            .clone();

        let settings: Value =
            serde_json::from_str(&speaker.voice_settings_json).unwrap_or_else(|_| json!({}));
        let speed_ratio = settings
            .get("speed_ratio")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        let pitch_hz_delta = settings
            .get("pitch_hz_delta")
            .and_then(Value::as_i64)
            .unwrap_or(0);

        let dialogue_dir = Path::new(&project.root_path).join("generated/dialogue");
        tokio::fs::create_dir_all(&dialogue_dir)
            .await
            .map_err(|e| format!("Failed to create generated/dialogue directory: {e}"))?;
        let out_wav = dialogue_dir.join(format!("{}.wav", segments_all.id));

        let res = self
            .worker
            .call_rpc_with_timeout(
                "task.run",
                json!({
                    "task_id": Uuid::new_v4().to_string(),
                    "task_type": "tts.synthesize",
                    "payload": {
                        "text": segments_all.translated_text,
                        "voice_id": voice_id,
                        "output_path": out_wav.to_string_lossy(),
                        "speed_ratio": speed_ratio,
                        "pitch_hz_delta": pitch_hz_delta,
                    }
                }),
                None,
            )
            .await
            .map_err(|e| format!("[{}] {}", e.code, e.message))?;

        let output = res.get("output").cloned().unwrap_or_default();
        let duration_ms = output
            .get("duration_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let provider_id = output
            .get("provider_id")
            .and_then(Value::as_str)
            .unwrap_or("edge-neural-tts")
            .to_string();

        let updated = self
            .db
            .update_segment_tts_output(
                &segments_all.id,
                &provider_id,
                &voice_id,
                &out_wav.to_string_lossy(),
                duration_ms,
            )
            .await?;

        self.preview_server
            .register_asset(format!("seg-tts-{}", updated.id), out_wav)
            .await;

        Ok(updated)
    }

    pub async fn synthesize_project_tts(&self, project_id: &str) -> Result<Vec<TaskRecord>, String> {
        let project = self
            .db
            .get_project(project_id)
            .await?
            .ok_or_else(|| format!("Project '{project_id}' not found"))?;
        if self.db.has_active_project_tasks(project_id).await? {
            return Err("A pipeline run is already queued or running for this project".to_string());
        }

        let segments = self.db.list_dialogue_segments(project_id).await?;
        let speakers = self.db.list_speakers(project_id).await?;
        let speaker_map: HashMap<String, &SpeakerRecord> =
            speakers.iter().map(|s| (s.id.clone(), s)).collect();

        let dialogue_dir = Path::new(&project.root_path).join("generated/dialogue");
        tokio::fs::create_dir_all(&dialogue_dir)
            .await
            .map_err(|e| format!("Failed to create generated/dialogue directory: {e}"))?;

        let mut items: Vec<Value> = Vec::new();
        for seg in &segments {
            if seg.translated_text.trim().is_empty() {
                continue;
            }
            let Some(sp) = seg.speaker_id.as_ref().and_then(|sid| speaker_map.get(sid)) else {
                continue;
            };
            let Some(ref voice_id) = sp.voice_id else {
                continue;
            };
            if voice_id.trim().is_empty() {
                continue;
            }
            let settings: Value =
                serde_json::from_str(&sp.voice_settings_json).unwrap_or_else(|_| json!({}));
            let speed_ratio = settings
                .get("speed_ratio")
                .and_then(Value::as_f64)
                .unwrap_or(1.0);
            let pitch_hz_delta = settings
                .get("pitch_hz_delta")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let out_wav = dialogue_dir.join(format!("{}.wav", seg.id));
            items.push(json!({
                "segment_id": seg.id,
                "text": seg.translated_text,
                "voice_id": voice_id,
                "output_path": out_wav.to_string_lossy(),
                "speed_ratio": speed_ratio,
                "pitch_hz_delta": pitch_hz_delta,
            }));
        }

        if items.is_empty() {
            return Err(
                "No translated segments have an assigned speaker voice. Translate lines and assign character voices first.".to_string(),
            );
        }

        let run_id = Uuid::new_v4().to_string();
        let tts_id = Uuid::new_v4().to_string();
        let persist_id = Uuid::new_v4().to_string();

        let nodes = vec![
            NewTaskSpec {
                id: tts_id.clone(),
                task_type: "tts.synthesize".to_string(),
                resource_class: ResourceClass::NetworkIo,
                input_json: json!({ "items": items }),
                depends_on: vec![],
                max_retries: 1,
            },
            NewTaskSpec {
                id: persist_id,
                task_type: "tts.persist_batch".to_string(),
                resource_class: ResourceClass::CpuLight,
                input_json: json!({ "tts_task_id": tts_id }),
                depends_on: vec![tts_id],
                max_retries: 0,
            },
        ];

        self.scheduler
            .enqueue_dag(Some(project_id.to_string()), Some(run_id), nodes)
            .await
    }

    pub async fn get_segment_audio_urls(
        &self,
        project_id: &str,
    ) -> Result<HashMap<String, String>, String> {
        let segments = self.db.list_dialogue_segments(project_id).await?;
        let mut urls = HashMap::new();
        for seg in segments {
            if let Some(ref path_str) = seg.tts_audio_path {
                let p = PathBuf::from(path_str);
                if p.exists() {
                    let asset_id = format!("seg-tts-{}", seg.id);
                    self.preview_server.register_asset(&asset_id, p).await;
                    urls.insert(seg.id, self.preview_server.url_for(&asset_id));
                }
            }
        }
        Ok(urls)
    }
}

pub async fn handle_persist_translations(
    db: &AppDatabase,
    task: &TaskRecord,
) -> Result<Value, (String, String)> {
    let input: Value = serde_json::from_str(&task.input_json)
        .map_err(|e| ("InvalidInput".to_string(), format!("Bad input_json: {e}")))?;

    let project_id = input
        .get("project_id")
        .and_then(Value::as_str)
        .ok_or_else(|| ("InvalidInput".to_string(), "Missing project_id".to_string()))?;
    let trans_path = input
        .get("translation_path")
        .and_then(Value::as_str)
        .ok_or_else(|| ("InvalidInput".to_string(), "Missing translation_path".to_string()))?;

    let raw = tokio::fs::read_to_string(trans_path)
        .await
        .map_err(|e| ("FileReadError".to_string(), format!("Failed to read translation JSON: {e}")))?;
    let doc: TranslationOutputDoc = serde_json::from_str(&raw)
        .map_err(|e| ("ParseError".to_string(), format!("Invalid translation JSON: {e}")))?;

    let pairs: Vec<(String, String)> = doc
        .translations
        .into_iter()
        .map(|t| (t.segment_id, t.translated_text))
        .collect();
    let count = pairs.len();

    db.apply_translations(project_id, &pairs)
        .await
        .map_err(|e| ("DatabaseError".to_string(), e))?;

    Ok(json!({ "translated_count": count }))
}

async fn handle_persist_tts_batch(
    db: &AppDatabase,
    preview_server: &MediaPreviewServer,
    task: &TaskRecord,
) -> Result<Value, (String, String)> {
    let input: Value = serde_json::from_str(&task.input_json)
        .map_err(|e| ("InvalidInput".to_string(), format!("Bad input_json: {e}")))?;

    let tts_task_id = input
        .get("tts_task_id")
        .and_then(Value::as_str)
        .ok_or_else(|| ("InvalidInput".to_string(), "Missing tts_task_id".to_string()))?;

    let upstream = db
        .get_task(tts_task_id)
        .await
        .map_err(|e| ("DatabaseError".to_string(), e))?
        .ok_or_else(|| ("InvalidInput".to_string(), "Upstream TTS task missing".to_string()))?;

    let out_val: Value = serde_json::from_str(&upstream.output_json)
        .map_err(|e| ("ParseError".to_string(), format!("Invalid TTS output JSON: {e}")))?;
    let items_val = out_val
        .get("output")
        .and_then(|o| o.get("items"))
        .cloned()
        .unwrap_or_else(|| json!([]));

    let parsed: TtsBatchPersistInput = serde_json::from_value(json!({ "results_items": items_val }))
        .map_err(|e| ("ParseError".to_string(), format!("Invalid TTS items: {e}")))?;

    let mut count = 0usize;
    for item in parsed.results_items {
        db.update_segment_tts_output(
            &item.segment_id,
            &item.provider_id,
            &item.voice_id,
            &item.output_path,
            item.duration_ms,
        )
        .await
        .map_err(|e| ("DatabaseError".to_string(), e))?;

        preview_server
            .register_asset(format!("seg-tts-{}", item.segment_id), PathBuf::from(&item.output_path))
            .await;
        count += 1;
    }

    Ok(json!({ "synthesized_count": count }))
}
