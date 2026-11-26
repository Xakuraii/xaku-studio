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
}
