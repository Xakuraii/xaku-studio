use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub source_lang: String,
    pub target_lang: String,
    pub preset: String,
    pub settings_json: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MediaAssetRecord {
    pub id: String,
    pub project_id: String,
    pub role: String,
    pub file_path: String,
    pub sha256: Option<String>,
    pub duration_ms: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps_num: Option<i64>,
    pub fps_den: Option<i64>,
    pub audio_channels: Option<i64>,
    pub sample_rate: Option<i64>,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskRecord {
    pub id: String,
    pub project_id: Option<String>,
    pub pipeline_run_id: Option<String>,
    pub task_type: String,
    pub resource_class: String,
    pub status: String,
    pub progress: f64,
    pub stage_message: String,
    pub input_json: String,
    pub output_json: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i64,
    pub max_retries: i64,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskDependencyRecord {
    pub task_id: String,
    pub depends_on_task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DownloadRecord {
    pub id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub url: String,
    pub dest_path: String,
    pub total_bytes: Option<i64>,
    pub downloaded_bytes: i64,
    pub sha256_expected: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpeakerRecord {
    pub id: String,
    pub project_id: String,
    pub diarization_label: String,
    pub display_name: String,
    pub voice_gender_est: String,
    pub gender_confidence: f64,
    pub pitch_hz_avg: Option<f64>,
    pub voice_provider_id: Option<String>,
    pub voice_id: Option<String>,
    pub voice_settings_json: String,
    pub is_user_overridden: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DialogueSegmentRecord {
    pub id: String,
    pub project_id: String,
    pub segment_index: i64,
    pub speaker_id: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub source_text: String,
    pub source_text_original: String,
    pub words_json: String,
    pub emotion_est: String,
    pub emotion_confidence: f64,
    pub translated_text: String,
    pub translated_text_auto: String,
    pub translation_status: String,
    pub tts_provider_id: Option<String>,
    pub tts_voice_id: Option<String>,
    pub tts_audio_path: Option<String>,
    pub tts_duration_ms: Option<i64>,
    pub sync_mode: String,
    pub speed_ratio: f64,
    pub gain_db: f64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GlossaryEntryRecord {
    pub id: String,
    pub project_id: Option<String>,
    pub source_term: String,
    pub target_term: String,
    pub notes: String,
}

#[derive(Clone)]
pub struct AppDatabase {
    pool: SqlitePool,
}

impl AppDatabase {
    pub async fn connect(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create database directory: {e}"))?;
        }

        let options = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            db_path.to_string_lossy()
        ))
        .map_err(|e| format!("Invalid SQLite path: {e}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| format!("Failed to open SQLite database: {e}"))?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| format!("Failed to run SQLite migrations: {e}"))?;

        Ok(Self { pool })
    }

    pub async fn connect_in_memory() -> Result<Self, String> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(|e| format!("Invalid in-memory SQLite URL: {e}"))?
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|e| format!("Failed to open in-memory SQLite: {e}"))?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| format!("Failed to run SQLite migrations: {e}"))?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn insert_project(&self, project: &ProjectRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO projects (
                id, name, root_path, source_lang, target_lang, preset, settings_json, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(&project.id)
        .bind(&project.name)
        .bind(&project.root_path)
        .bind(&project.source_lang)
        .bind(&project.target_lang)
        .bind(&project.preset)
        .bind(&project.settings_json)
        .bind(&project.status)
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to insert project: {e}"))?;
        Ok(())
    }

    pub async fn upsert_project(&self, project: &ProjectRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO projects (
                id, name, root_path, source_lang, target_lang, preset, settings_json, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                root_path = excluded.root_path,
                source_lang = excluded.source_lang,
                target_lang = excluded.target_lang,
                preset = excluded.preset,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            "#,
        )
        .bind(&project.id)
        .bind(&project.name)
        .bind(&project.root_path)
        .bind(&project.source_lang)
        .bind(&project.target_lang)
        .bind(&project.preset)
        .bind(&project.settings_json)
        .bind(&project.status)
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to upsert project: {e}"))?;
        Ok(())
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectRecord>, String> {
        sqlx::query_as::<_, ProjectRecord>(
            "SELECT * FROM projects ORDER BY updated_at DESC, created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list projects: {e}"))
    }

    pub async fn get_project(&self, project_id: &str) -> Result<Option<ProjectRecord>, String> {
        sqlx::query_as::<_, ProjectRecord>("SELECT * FROM projects WHERE id = ?1")
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("Failed to fetch project: {e}"))
    }

    pub async fn rename_project(&self, project_id: &str, new_name: &str) -> Result<(), String> {
        let res = sqlx::query(
            "UPDATE projects SET name = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2",
        )
        .bind(new_name)
        .bind(project_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to rename project: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Project '{project_id}' not found"));
        }
        Ok(())
    }

    pub async fn delete_project(&self, project_id: &str) -> Result<(), String> {
        let res = sqlx::query("DELETE FROM projects WHERE id = ?1")
            .bind(project_id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("Failed to delete project: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Project '{project_id}' not found"));
        }
        Ok(())
    }

    pub async fn replace_project_media_assets(
        &self,
        project_id: &str,
        assets: &[MediaAssetRecord],
    ) -> Result<(), String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("Failed to begin SQLite transaction: {e}"))?;

        sqlx::query("DELETE FROM media_assets WHERE project_id = ?1")
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to clear old media assets: {e}"))?;

        for asset in assets {
            sqlx::query(
                r#"
                INSERT INTO media_assets (
                    id, project_id, role, file_path, sha256, duration_ms,
                    width, height, fps_num, fps_den, audio_channels, sample_rate, metadata_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
            )
            .bind(&asset.id)
            .bind(&asset.project_id)
            .bind(&asset.role)
            .bind(&asset.file_path)
            .bind(&asset.sha256)
            .bind(asset.duration_ms)
            .bind(asset.width)
            .bind(asset.height)
            .bind(asset.fps_num)
            .bind(asset.fps_den)
            .bind(asset.audio_channels)
            .bind(asset.sample_rate)
            .bind(&asset.metadata_json)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to insert media asset: {e}"))?;
        }

        sqlx::query(
            "UPDATE projects SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
        )
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Failed to update project timestamp: {e}"))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit media asset transaction: {e}"))?;

        Ok(())
    }

    pub async fn list_media_assets(&self, project_id: &str) -> Result<Vec<MediaAssetRecord>, String> {
        sqlx::query_as::<_, MediaAssetRecord>(
            "SELECT * FROM media_assets WHERE project_id = ?1 ORDER BY created_at ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list media assets: {e}"))
    }

    pub async fn insert_task_dag(
        &self,
        tasks: &[TaskRecord],
        deps: &[TaskDependencyRecord],
    ) -> Result<(), String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("Failed to begin task DAG transaction: {e}"))?;

        for t in tasks {
            sqlx::query(
                r#"
                INSERT INTO tasks (
                    id, project_id, pipeline_run_id, task_type, resource_class,
                    status, progress, stage_message, input_json, output_json,
                    error_code, error_message, retry_count, max_retries,
                    started_at, completed_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                "#,
            )
            .bind(&t.id)
            .bind(&t.project_id)
            .bind(&t.pipeline_run_id)
            .bind(&t.task_type)
            .bind(&t.resource_class)
            .bind(&t.status)
            .bind(t.progress)
            .bind(&t.stage_message)
            .bind(&t.input_json)
            .bind(&t.output_json)
            .bind(&t.error_code)
            .bind(&t.error_message)
            .bind(t.retry_count)
            .bind(t.max_retries)
            .bind(&t.started_at)
            .bind(&t.completed_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to insert task: {e}"))?;
        }

        for d in deps {
            sqlx::query(
                "INSERT INTO task_dependencies (task_id, depends_on_task_id) VALUES (?1, ?2)",
            )
            .bind(&d.task_id)
            .bind(&d.depends_on_task_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to insert task dependency: {e}"))?;
        }

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit task DAG: {e}"))?;
        Ok(())
    }

    pub async fn list_tasks(&self, project_id: Option<&str>) -> Result<Vec<TaskRecord>, String> {
        match project_id {
            Some(pid) => sqlx::query_as::<_, TaskRecord>(
                "SELECT * FROM tasks WHERE project_id = ?1 ORDER BY created_at DESC",
            )
            .bind(pid)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| format!("Failed to list project tasks: {e}")),
            None => sqlx::query_as::<_, TaskRecord>(
                "SELECT * FROM tasks ORDER BY created_at DESC LIMIT 200",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| format!("Failed to list tasks: {e}")),
        }
    }

    pub async fn list_queued_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        sqlx::query_as::<_, TaskRecord>(
            "SELECT * FROM tasks WHERE status = 'queued' ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list queued tasks: {e}"))
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskRecord>, String> {
        sqlx::query_as::<_, TaskRecord>("SELECT * FROM tasks WHERE id = ?1")
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("Failed to fetch task: {e}"))
    }

    pub async fn list_task_dependencies(&self, task_id: &str) -> Result<Vec<String>, String> {
        sqlx::query_scalar::<_, String>(
            "SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ?1",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list task dependencies: {e}"))
    }

    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: &str,
        progress: f64,
        stage_message: &str,
        output_json: Option<&str>,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), String> {
        let is_running = status == "running";
        let is_terminal = matches!(status, "completed" | "failed" | "cancelled");

        sqlx::query(
            r#"
            UPDATE tasks SET
                status = ?1,
                progress = ?2,
                stage_message = ?3,
                output_json = COALESCE(?4, output_json),
                error_code = ?5,
                error_message = ?6,
                started_at = CASE WHEN ?7 THEN COALESCE(started_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) ELSE started_at END,
                completed_at = CASE WHEN ?8 THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') ELSE completed_at END
            WHERE id = ?9
            "#,
        )
        .bind(status)
        .bind(progress)
        .bind(stage_message)
        .bind(output_json)
        .bind(error_code)
        .bind(error_message)
        .bind(is_running)
        .bind(is_terminal)
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update task status: {e}"))?;
        Ok(())
    }

    pub async fn mark_running_tasks_interrupted(&self) -> Result<u64, String> {
        let res = sqlx::query(
            r#"
            UPDATE tasks SET
                status = 'interrupted',
                stage_message = 'Interrupted by previous application shutdown',
                completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE status = 'running'
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to recover interrupted tasks: {e}"))?;
        Ok(res.rows_affected())
    }

    pub async fn reset_task_for_retry(&self, task_id: &str) -> Result<(), String> {
        let res = sqlx::query(
            r#"
            UPDATE tasks SET
                status = 'queued',
                progress = 0.0,
                stage_message = 'Queued for retry',
                error_code = NULL,
                error_message = NULL,
                completed_at = NULL,
                retry_count = retry_count + 1
            WHERE id = ?1
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to reset task for retry: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Task '{task_id}' not found"));
        }
        Ok(())
    }

    pub async fn upsert_download(&self, dl: &DownloadRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO downloads (
                id, resource_type, resource_id, url, dest_path, total_bytes,
                downloaded_bytes, sha256_expected, status, error_message, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(id) DO UPDATE SET
                url = excluded.url,
                dest_path = excluded.dest_path,
                total_bytes = excluded.total_bytes,
                downloaded_bytes = excluded.downloaded_bytes,
                sha256_expected = excluded.sha256_expected,
                status = excluded.status,
                error_message = excluded.error_message,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            "#,
        )
        .bind(&dl.id)
        .bind(&dl.resource_type)
        .bind(&dl.resource_id)
        .bind(&dl.url)
        .bind(&dl.dest_path)
        .bind(dl.total_bytes)
        .bind(dl.downloaded_bytes)
        .bind(&dl.sha256_expected)
        .bind(&dl.status)
        .bind(&dl.error_message)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to upsert download: {e}"))?;
        Ok(())
    }

    pub async fn update_download_progress(
        &self,
        id: &str,
        downloaded_bytes: i64,
        total_bytes: Option<i64>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), String> {
        sqlx::query(
            r#"
            UPDATE downloads SET
                downloaded_bytes = ?1,
                total_bytes = COALESCE(?2, total_bytes),
                status = ?3,
                error_message = ?4,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?5
            "#,
        )
        .bind(downloaded_bytes)
        .bind(total_bytes)
        .bind(status)
        .bind(error_message)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update download progress: {e}"))?;
        Ok(())
    }

    pub async fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>, String> {
        sqlx::query_as::<_, DownloadRecord>("SELECT * FROM downloads WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("Failed to get download: {e}"))
    }

    pub async fn find_download_by_resource(
        &self,
        resource_id: &str,
    ) -> Result<Option<DownloadRecord>, String> {
        sqlx::query_as::<_, DownloadRecord>(
            "SELECT * FROM downloads WHERE resource_id = ?1 ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(resource_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to find download by resource: {e}"))
    }

    pub async fn list_downloads(&self) -> Result<Vec<DownloadRecord>, String> {
        sqlx::query_as::<_, DownloadRecord>(
            "SELECT * FROM downloads ORDER BY updated_at DESC LIMIT 100",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list downloads: {e}"))
    }

    pub async fn update_task_progress(
        &self,
        task_id: &str,
        progress: f64,
        stage_message: &str,
    ) -> Result<(), String> {
        sqlx::query(
            r#"
            UPDATE tasks SET
                progress = ?1,
                stage_message = ?2
            WHERE id = ?3 AND status = 'running'
            "#,
        )
        .bind(progress)
        .bind(stage_message)
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update task progress: {e}"))?;
        Ok(())
    }

    pub async fn has_active_project_tasks(&self, project_id: &str) -> Result<bool, String> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM tasks WHERE project_id = ?1 AND status IN ('queued', 'running')",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("Failed to check active project tasks: {e}"))?;
        Ok(count > 0)
    }

    pub async fn list_latest_run_tasks(&self, project_id: &str) -> Result<Vec<TaskRecord>, String> {
        let latest_run: Option<String> = sqlx::query_scalar(
            r#"
            SELECT pipeline_run_id FROM tasks
            WHERE project_id = ?1 AND pipeline_run_id IS NOT NULL
            ORDER BY created_at DESC LIMIT 1
            "#,
        )
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to fetch latest pipeline run: {e}"))?;

        let Some(run_id) = latest_run else {
            return Ok(Vec::new());
        };

        sqlx::query_as::<_, TaskRecord>(
            "SELECT * FROM tasks WHERE project_id = ?1 AND pipeline_run_id = ?2 ORDER BY created_at ASC",
        )
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list latest run tasks: {e}"))
    }

    pub async fn replace_dialogue_segments(
        &self,
        project_id: &str,
        segments: &[DialogueSegmentRecord],
    ) -> Result<(), String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("Failed to begin dialogue segments transaction: {e}"))?;

        sqlx::query("DELETE FROM dialogue_segments WHERE project_id = ?1")
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to clear old dialogue segments: {e}"))?;

        for seg in segments {
            sqlx::query(
                r#"
                INSERT INTO dialogue_segments (
                    id, project_id, segment_index, speaker_id, start_ms, end_ms,
                    source_text, source_text_original, words_json,
                    emotion_est, emotion_confidence,
                    translated_text, translated_text_auto, translation_status,
                    tts_provider_id, tts_voice_id, tts_audio_path, tts_duration_ms,
                    sync_mode, speed_ratio, gain_db
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9,
                    ?10, ?11,
                    ?12, ?13, ?14,
                    ?15, ?16, ?17, ?18,
                    ?19, ?20, ?21
                )
                "#,
            )
            .bind(&seg.id)
            .bind(&seg.project_id)
            .bind(seg.segment_index)
            .bind(&seg.speaker_id)
            .bind(seg.start_ms)
            .bind(seg.end_ms)
            .bind(&seg.source_text)
            .bind(&seg.source_text_original)
            .bind(&seg.words_json)
            .bind(&seg.emotion_est)
            .bind(seg.emotion_confidence)
            .bind(&seg.translated_text)
            .bind(&seg.translated_text_auto)
            .bind(&seg.translation_status)
            .bind(&seg.tts_provider_id)
            .bind(&seg.tts_voice_id)
            .bind(&seg.tts_audio_path)
            .bind(seg.tts_duration_ms)
            .bind(&seg.sync_mode)
            .bind(seg.speed_ratio)
            .bind(seg.gain_db)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to insert dialogue segment: {e}"))?;
        }

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit dialogue segments: {e}"))?;
        Ok(())
    }

    pub async fn list_dialogue_segments(
        &self,
        project_id: &str,
    ) -> Result<Vec<DialogueSegmentRecord>, String> {
        sqlx::query_as::<_, DialogueSegmentRecord>(
            "SELECT * FROM dialogue_segments WHERE project_id = ?1 ORDER BY segment_index ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list dialogue segments: {e}"))
    }

    pub async fn list_speakers(&self, project_id: &str) -> Result<Vec<SpeakerRecord>, String> {
        sqlx::query_as::<_, SpeakerRecord>(
            "SELECT * FROM speakers WHERE project_id = ?1 ORDER BY created_at ASC, display_name ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list speakers: {e}"))
    }

    pub async fn replace_speakers_and_assign(
        &self,
        project_id: &str,
        speakers: &[SpeakerRecord],
        assignments: &[(String, Option<String>)],
    ) -> Result<(), String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("Failed to begin speaker replacement transaction: {e}"))?;

        sqlx::query("UPDATE dialogue_segments SET speaker_id = NULL WHERE project_id = ?1")
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to reset segment speakers: {e}"))?;

        sqlx::query("DELETE FROM speakers WHERE project_id = ?1")
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to delete old speakers: {e}"))?;

        for sp in speakers {
            sqlx::query(
                r#"
                INSERT INTO speakers (
                    id, project_id, diarization_label, display_name,
                    voice_gender_est, gender_confidence, pitch_hz_avg,
                    voice_provider_id, voice_id, voice_settings_json, is_user_overridden
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
            )
            .bind(&sp.id)
            .bind(&sp.project_id)
            .bind(&sp.diarization_label)
            .bind(&sp.display_name)
            .bind(&sp.voice_gender_est)
            .bind(sp.gender_confidence)
            .bind(sp.pitch_hz_avg)
            .bind(&sp.voice_provider_id)
            .bind(&sp.voice_id)
            .bind(&sp.voice_settings_json)
            .bind(sp.is_user_overridden)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to insert speaker: {e}"))?;
        }

        for (segment_id, speaker_id) in assignments {
            sqlx::query(
                "UPDATE dialogue_segments SET speaker_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2 AND project_id = ?3",
            )
            .bind(speaker_id)
            .bind(segment_id)
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to assign speaker to segment: {e}"))?;
        }

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit speaker assignments: {e}"))?;
        Ok(())
    }

    pub async fn update_segment_text(
        &self,
        segment_id: &str,
        source_text: &str,
    ) -> Result<DialogueSegmentRecord, String> {
        let res = sqlx::query(
            r#"
            UPDATE dialogue_segments SET
                source_text = ?1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?2
            "#,
        )
        .bind(source_text)
        .bind(segment_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update segment text: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Segment '{segment_id}' not found"));
        }

        sqlx::query_as::<_, DialogueSegmentRecord>("SELECT * FROM dialogue_segments WHERE id = ?1")
            .bind(segment_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| format!("Failed to reload segment: {e}"))
    }

    pub async fn update_segment_speaker(
        &self,
        segment_id: &str,
        speaker_id: Option<&str>,
    ) -> Result<DialogueSegmentRecord, String> {
        let res = sqlx::query(
            r#"
            UPDATE dialogue_segments SET
                speaker_id = ?1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?2
            "#,
        )
        .bind(speaker_id)
        .bind(segment_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update segment speaker: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Segment '{segment_id}' not found"));
        }

        sqlx::query_as::<_, DialogueSegmentRecord>("SELECT * FROM dialogue_segments WHERE id = ?1")
            .bind(segment_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| format!("Failed to reload segment: {e}"))
    }

    pub async fn insert_speaker(&self, speaker: &SpeakerRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO speakers (
                id, project_id, diarization_label, display_name,
                voice_gender_est, gender_confidence, pitch_hz_avg,
                voice_provider_id, voice_id, voice_settings_json, is_user_overridden
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
        )
        .bind(&speaker.id)
        .bind(&speaker.project_id)
        .bind(&speaker.diarization_label)
        .bind(&speaker.display_name)
        .bind(&speaker.voice_gender_est)
        .bind(speaker.gender_confidence)
        .bind(speaker.pitch_hz_avg)
        .bind(&speaker.voice_provider_id)
        .bind(&speaker.voice_id)
        .bind(&speaker.voice_settings_json)
        .bind(speaker.is_user_overridden)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to create speaker: {e}"))?;
        Ok(())
    }

    pub async fn rename_speaker(
        &self,
        speaker_id: &str,
        display_name: &str,
    ) -> Result<SpeakerRecord, String> {
        let trimmed = display_name.trim();
        if trimmed.is_empty() {
            return Err("Speaker name cannot be empty".to_string());
        }
        let res = sqlx::query(
            "UPDATE speakers SET display_name = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2",
        )
        .bind(trimmed)
        .bind(speaker_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to rename speaker: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Speaker '{speaker_id}' not found"));
        }

        sqlx::query_as::<_, SpeakerRecord>("SELECT * FROM speakers WHERE id = ?1")
            .bind(speaker_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| format!("Failed to reload speaker: {e}"))
    }

    pub async fn set_speaker_voice_presentation(
        &self,
        speaker_id: &str,
        voice_gender_est: &str,
    ) -> Result<SpeakerRecord, String> {
        if !matches!(voice_gender_est, "masculine" | "feminine" | "uncertain") {
            return Err(format!("Invalid voice presentation '{voice_gender_est}'"));
        }
        let res = sqlx::query(
            "UPDATE speakers SET voice_gender_est = ?1, is_user_overridden = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2",
        )
        .bind(voice_gender_est)
        .bind(speaker_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update voice presentation: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Speaker '{speaker_id}' not found"));
        }

        sqlx::query_as::<_, SpeakerRecord>("SELECT * FROM speakers WHERE id = ?1")
            .bind(speaker_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| format!("Failed to reload speaker: {e}"))
    }

    pub async fn merge_speakers(
        &self,
        source_speaker_id: &str,
        target_speaker_id: &str,
    ) -> Result<(), String> {
        if source_speaker_id == target_speaker_id {
            return Err("Cannot merge a speaker into itself".to_string());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("Failed to begin speaker merge transaction: {e}"))?;

        let source_proj: Option<String> =
            sqlx::query_scalar("SELECT project_id FROM speakers WHERE id = ?1")
                .bind(source_speaker_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| format!("Failed to look up source speaker: {e}"))?;
        let target_proj: Option<String> =
            sqlx::query_scalar("SELECT project_id FROM speakers WHERE id = ?1")
                .bind(target_speaker_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| format!("Failed to look up target speaker: {e}"))?;

        match (source_proj, target_proj) {
            (Some(sp), Some(tp)) if sp == tp => {}
            (None, _) => return Err(format!("Source speaker '{source_speaker_id}' not found")),
            (_, None) => return Err(format!("Target speaker '{target_speaker_id}' not found")),
            _ => return Err("Speakers belong to different projects".to_string()),
        }

        sqlx::query(
            "UPDATE dialogue_segments SET speaker_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE speaker_id = ?2",
        )
        .bind(target_speaker_id)
        .bind(source_speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Failed to reassign segments during merge: {e}"))?;

        sqlx::query("DELETE FROM speakers WHERE id = ?1")
            .bind(source_speaker_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to delete merged speaker: {e}"))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit speaker merge: {e}"))?;
        Ok(())
    }

    pub async fn split_segment(
        &self,
        segment_id: &str,
        char_index: usize,
    ) -> Result<Vec<DialogueSegmentRecord>, String> {
        let seg = sqlx::query_as::<_, DialogueSegmentRecord>(
            "SELECT * FROM dialogue_segments WHERE id = ?1",
        )
        .bind(segment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to fetch segment to split: {e}"))?
        .ok_or_else(|| format!("Segment '{segment_id}' not found"))?;

        let chars: Vec<char> = seg.source_text.chars().collect();
        if char_index == 0 || char_index >= chars.len() {
            return Err("Split position must fall inside segment text".to_string());
        }

        let left_text = chars[..char_index]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        let right_text = chars[char_index..]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if left_text.is_empty() || right_text.is_empty() {
            return Err("Both halves of a split segment must contain text".to_string());
        }

        #[derive(Deserialize, Serialize)]
        struct WordItem {
            start: f64,
            end: f64,
            word: String,
            #[serde(default)]
            probability: f64,
        }

        let words: Vec<WordItem> = serde_json::from_str(&seg.words_json).unwrap_or_default();
        let mut word_split_ms: Option<i64> = None;
        let mut left_words_json = "[]".to_string();
        let mut right_words_json = "[]".to_string();

        if !words.is_empty() {
            let mut acc_chars = 0usize;
            let mut split_word_idx = words.len() / 2;
            for (idx, w) in words.iter().enumerate() {
                let len = w.word.trim().chars().count().max(1);
                if acc_chars + len >= char_index {
                    split_word_idx = idx;
                    let mid_word = acc_chars + (len / 2);
                    let candidate = if char_index <= mid_word {
                        (w.start * 1000.0).round() as i64
                    } else {
                        split_word_idx = (idx + 1).min(words.len());
                        (w.end * 1000.0).round() as i64
                    };
                    if candidate > seg.start_ms && candidate < seg.end_ms {
                        word_split_ms = Some(candidate);
                    }
                    break;
                }
                acc_chars += len;
            }
            if let Some(split_ms) = word_split_ms {
                let left_slice: Vec<&WordItem> = words
                    .iter()
                    .take(split_word_idx)
                    .filter(|w| ((w.end * 1000.0).round() as i64) <= split_ms + 50)
                    .collect();
                let right_slice: Vec<&WordItem> = words
                    .iter()
                    .skip(split_word_idx)
                    .collect();
                left_words_json = serde_json::to_string(&left_slice).unwrap_or_else(|_| "[]".into());
                right_words_json =
                    serde_json::to_string(&right_slice).unwrap_or_else(|_| "[]".into());
            }
        }

        let split_ms = word_split_ms.unwrap_or_else(|| {
            let ratio = char_index as f64 / chars.len() as f64;
            seg.start_ms + ((seg.end_ms - seg.start_ms) as f64 * ratio).round() as i64
        });

        if split_ms <= seg.start_ms || split_ms >= seg.end_ms {
            return Err("Segment duration is too short to split".to_string());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("Failed to begin split transaction: {e}"))?;

        sqlx::query(
            "UPDATE dialogue_segments SET segment_index = segment_index + 1 WHERE project_id = ?1 AND segment_index > ?2",
        )
        .bind(&seg.project_id)
        .bind(seg.segment_index)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Failed to shift segment indices: {e}"))?;

        sqlx::query(
            r#"
            UPDATE dialogue_segments SET
                end_ms = ?1,
                source_text = ?2,
                words_json = ?3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?4
            "#,
        )
        .bind(split_ms)
        .bind(&left_text)
        .bind(&left_words_json)
        .bind(&seg.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Failed to update first split segment: {e}"))?;

        let new_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO dialogue_segments (
                id, project_id, segment_index, speaker_id, start_ms, end_ms,
                source_text, source_text_original, words_json,
                emotion_est, emotion_confidence,
                translated_text, translated_text_auto, translation_status,
                sync_mode, speed_ratio, gain_db
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9,
                ?10, ?11,
                '', '', 'pending',
                ?12, 1.0, 0.0
            )
            "#,
        )
        .bind(&new_id)
        .bind(&seg.project_id)
        .bind(seg.segment_index + 1)
        .bind(&seg.speaker_id)
        .bind(split_ms)
        .bind(seg.end_ms)
        .bind(&right_text)
        .bind(&right_text)
        .bind(&right_words_json)
        .bind(&seg.emotion_est)
        .bind(seg.emotion_confidence)
        .bind(&seg.sync_mode)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Failed to insert second split segment: {e}"))?;

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit segment split: {e}"))?;

        self.list_dialogue_segments(&seg.project_id).await
    }

    pub async fn list_glossary(&self, project_id: Option<&str>) -> Result<Vec<GlossaryEntryRecord>, String> {
        match project_id {
            Some(pid) => sqlx::query_as::<_, GlossaryEntryRecord>(
                "SELECT * FROM glossary_entries WHERE project_id IS NULL OR project_id = ?1 ORDER BY source_term ASC",
            )
            .bind(pid)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| format!("Failed to list glossary entries: {e}")),
            None => sqlx::query_as::<_, GlossaryEntryRecord>(
                "SELECT * FROM glossary_entries WHERE project_id IS NULL ORDER BY source_term ASC",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| format!("Failed to list global glossary entries: {e}")),
        }
    }

    pub async fn upsert_glossary_entry(&self, entry: &GlossaryEntryRecord) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO glossary_entries (id, project_id, source_term, target_term, notes)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                source_term = excluded.source_term,
                target_term = excluded.target_term,
                notes = excluded.notes
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.project_id)
        .bind(&entry.source_term)
        .bind(&entry.target_term)
        .bind(&entry.notes)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to save glossary entry: {e}"))?;
        Ok(())
    }

    pub async fn delete_glossary_entry(&self, id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM glossary_entries WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("Failed to delete glossary entry: {e}"))?;
        Ok(())
    }

    pub async fn apply_translations(
        &self,
        project_id: &str,
        items: &[(String, String)],
    ) -> Result<(), String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("Failed to begin translation persistence tx: {e}"))?;

        for (segment_id, translated_text) in items {
            sqlx::query(
                r#"
                UPDATE dialogue_segments SET
                    translated_text = ?1,
                    translated_text_auto = ?1,
                    translation_status = 'auto',
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE id = ?2 AND project_id = ?3
                "#,
            )
            .bind(translated_text)
            .bind(segment_id)
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Failed to update translated segment: {e}"))?;
        }

        tx.commit()
            .await
            .map_err(|e| format!("Failed to commit translations: {e}"))?;
        Ok(())
    }

    pub async fn update_segment_translation(
        &self,
        segment_id: &str,
        translated_text: &str,
    ) -> Result<DialogueSegmentRecord, String> {
        let res = sqlx::query(
            r#"
            UPDATE dialogue_segments SET
                translated_text = ?1,
                translation_status = 'edited',
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?2
            "#,
        )
        .bind(translated_text)
        .bind(segment_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update segment translation: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Segment '{segment_id}' not found"));
        }

        sqlx::query_as::<_, DialogueSegmentRecord>("SELECT * FROM dialogue_segments WHERE id = ?1")
            .bind(segment_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| format!("Failed to reload translated segment: {e}"))
    }

    pub async fn assign_speaker_voice(
        &self,
        speaker_id: &str,
        voice_provider_id: &str,
        voice_id: &str,
        voice_settings_json: &str,
    ) -> Result<SpeakerRecord, String> {
        let res = sqlx::query(
            r#"
            UPDATE speakers SET
                voice_provider_id = ?1,
                voice_id = ?2,
                voice_settings_json = ?3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?4
            "#,
        )
        .bind(voice_provider_id)
        .bind(voice_id)
        .bind(voice_settings_json)
        .bind(speaker_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to assign speaker voice: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Speaker '{speaker_id}' not found"));
        }

        sqlx::query_as::<_, SpeakerRecord>("SELECT * FROM speakers WHERE id = ?1")
            .bind(speaker_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| format!("Failed to reload speaker: {e}"))
    }

    pub async fn update_segment_tts_output(
        &self,
        segment_id: &str,
        provider_id: &str,
        voice_id: &str,
        audio_path: &str,
        duration_ms: i64,
    ) -> Result<DialogueSegmentRecord, String> {
        let res = sqlx::query(
            r#"
            UPDATE dialogue_segments SET
                tts_provider_id = ?1,
                tts_voice_id = ?2,
                tts_audio_path = ?3,
                tts_duration_ms = ?4,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?5
            "#,
        )
        .bind(provider_id)
        .bind(voice_id)
        .bind(audio_path)
        .bind(duration_ms)
        .bind(segment_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update segment TTS output: {e}"))?;

        if res.rows_affected() == 0 {
            return Err(format!("Segment '{segment_id}' not found"));
        }

        sqlx::query_as::<_, DialogueSegmentRecord>("SELECT * FROM dialogue_segments WHERE id = ?1")
            .bind(segment_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| format!("Failed to reload segment: {e}"))
    }
}
