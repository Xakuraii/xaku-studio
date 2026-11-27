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
}
