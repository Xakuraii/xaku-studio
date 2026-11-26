use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::database::{AppDatabase, MediaAssetRecord, ProjectRecord};
use crate::media::ffmpeg::{compute_wav_waveform_peaks, extract_audio_wav};
use crate::media::ffprobe::{inspect_media, MediaInspection};
use crate::media::preview_server::MediaPreviewServer;

pub const PROJECT_MANIFEST_VERSION: u32 = 1;
pub const PROJECT_MANIFEST_FILE: &str = "project.xaku";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub project_id: String,
    pub name: String,
    pub source_lang: String,
    pub target_lang: String,
    pub preset: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project: ProjectRecord,
    pub assets: Vec<MediaAssetRecord>,
    pub source_preview_url: Option<String>,
    pub inspection: Option<MediaInspection>,
    pub waveform_peaks: Option<Vec<f32>>,
}

pub fn sanitize_folder_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Project name cannot be empty".to_string());
    }
    let cleaned: String = trimmed
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            _ => c,
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ').to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return Err("Project name contains only invalid path characters".to_string());
    }
    Ok(cleaned)
}

async fn ensure_project_dirs(root: &Path) -> Result<(), String> {
    let subdirs = [
        "source",
        "assets/audio",
        "assets/voices",
        "assets/subtitles",
        "generated/transcripts",
        "generated/translations",
        "generated/dialogue",
        "generated/audio",
        "exports",
        "cache",
    ];
    for rel in subdirs {
        tokio::fs::create_dir_all(root.join(rel))
            .await
            .map_err(|e| format!("Failed to create project directory '{rel}': {e}"))?;
    }
    Ok(())
}

async fn write_manifest(root: &Path, manifest: &ProjectManifest) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("Failed to serialize project.xaku manifest: {e}"))?;
    tokio::fs::write(root.join(PROJECT_MANIFEST_FILE), json)
        .await
        .map_err(|e| format!("Failed to write project.xaku: {e}"))?;
    Ok(())
}

async fn compute_sha256(path: &Path) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open file for SHA-256: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| format!("Failed to read file for SHA-256: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

fn non_destructive_target_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    let ext = Path::new(file_name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    for idx in 1..1000 {
        let next_name = if ext.is_empty() {
            format!("{stem}_{idx}")
        } else {
            format!("{stem}_{idx}.{ext}")
        };
        let next = dir.join(next_name);
        if !next.exists() {
            return next;
        }
    }
    dir.join(format!("{stem}_{}", Uuid::new_v4()))
}

pub struct ProjectManager {
    db: AppDatabase,
    preview_server: MediaPreviewServer,
}

impl ProjectManager {
    pub fn new(db: AppDatabase, preview_server: MediaPreviewServer) -> Self {
        Self { db, preview_server }
    }

    pub async fn register_existing_assets(&self) -> Result<(), String> {
        let projects = self.db.list_projects().await?;
        for project in projects {
            let assets = self.db.list_media_assets(&project.id).await?;
            for asset in assets {
                if asset.role == "source_video" || asset.role.starts_with("extracted_audio") {
                    self.preview_server
                        .register_asset(&asset.id, PathBuf::from(&asset.file_path))
                        .await;
                }
            }
        }
        Ok(())
    }

    pub async fn create_project(
        &self,
        parent_dir: &Path,
        name: &str,
        source_lang: &str,
        target_lang: &str,
        preset: &str,
    ) -> Result<ProjectSummary, String> {
        let folder_slug = sanitize_folder_name(name)?;
        let root_path = non_destructive_target_path(parent_dir, &folder_slug);
        ensure_project_dirs(&root_path).await?;

        let now = chrono_utc_now_approx();
        let project_id = Uuid::new_v4().to_string();
        let manifest = ProjectManifest {
            schema_version: PROJECT_MANIFEST_VERSION,
            project_id: project_id.clone(),
            name: name.trim().to_string(),
            source_lang: source_lang.to_string(),
            target_lang: target_lang.to_string(),
            preset: preset.to_string(),
            created_at: now.clone(),
        };
        write_manifest(&root_path, &manifest).await?;

        let record = ProjectRecord {
            id: project_id,
            name: manifest.name,
            root_path: root_path.to_string_lossy().into_owned(),
            source_lang: manifest.source_lang,
            target_lang: manifest.target_lang,
            preset: manifest.preset,
            settings_json: "{}".to_string(),
            status: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.db.insert_project(&record).await?;

        Ok(ProjectSummary {
            project: record,
            assets: Vec::new(),
            source_preview_url: None,
            inspection: None,
            waveform_peaks: None,
        })
    }

    pub async fn open_project_manifest(&self, xaku_path: &Path) -> Result<ProjectSummary, String> {
        let raw = tokio::fs::read_to_string(xaku_path)
            .await
            .map_err(|e| format!("Failed to read {}: {e}", xaku_path.display()))?;
        let manifest: ProjectManifest = serde_json::from_str(&raw)
            .map_err(|e| format!("Invalid project.xaku format: {e}"))?;

        if manifest.schema_version > PROJECT_MANIFEST_VERSION {
            return Err(format!(
                "Unsupported project.xaku schema version {}",
                manifest.schema_version
            ));
        }

        let root_path = xaku_path
            .parent()
            .ok_or_else(|| "Invalid project.xaku path".to_string())?;
        ensure_project_dirs(root_path).await?;

        let record = ProjectRecord {
            id: manifest.project_id,
            name: manifest.name,
            root_path: root_path.to_string_lossy().into_owned(),
            source_lang: manifest.source_lang,
            target_lang: manifest.target_lang,
            preset: manifest.preset,
            settings_json: "{}".to_string(),
            status: "active".to_string(),
            created_at: manifest.created_at.clone(),
            updated_at: manifest.created_at,
        };
        self.db.upsert_project(&record).await?;
        self.get_project_summary(&record.id).await
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectRecord>, String> {
        self.db.list_projects().await
    }

    pub async fn rename_project(&self, project_id: &str, new_name: &str) -> Result<ProjectRecord, String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err("Project name cannot be empty".to_string());
        }
        self.db.rename_project(project_id, trimmed).await?;
        let updated = self
            .db
            .get_project(project_id)
            .await?
            .ok_or_else(|| format!("Project '{project_id}' not found"))?;

        let manifest_path = PathBuf::from(&updated.root_path).join(PROJECT_MANIFEST_FILE);
        if manifest_path.exists() {
            if let Ok(raw) = tokio::fs::read_to_string(&manifest_path).await {
                if let Ok(mut manifest) = serde_json::from_str::<ProjectManifest>(&raw) {
                    manifest.name = trimmed.to_string();
                    let _ = write_manifest(Path::new(&updated.root_path), &manifest).await;
                }
            }
        }
        Ok(updated)
    }

    pub async fn delete_project(&self, project_id: &str, delete_files: bool) -> Result<(), String> {
        let project = self
            .db
            .get_project(project_id)
            .await?
            .ok_or_else(|| format!("Project '{project_id}' not found"))?;
        self.db.delete_project(project_id).await?;

        if delete_files {
            let root = PathBuf::from(&project.root_path);
            if root.join(PROJECT_MANIFEST_FILE).exists() {
                tokio::fs::remove_dir_all(&root)
                    .await
                    .map_err(|e| format!("Failed to remove project directory: {e}"))?;
            }
        }
        Ok(())
    }

    pub async fn import_media(
        &self,
        project_id: &str,
        source_file: &Path,
    ) -> Result<ProjectSummary, String> {
        if !source_file.is_file() {
            return Err(format!(
                "Selected media path is not a file: {}",
                source_file.display()
            ));
        }

        let project = self
            .db
            .get_project(project_id)
            .await?
            .ok_or_else(|| format!("Project '{project_id}' not found"))?;
        let root = PathBuf::from(&project.root_path);
        ensure_project_dirs(&root).await?;

        let file_name = source_file
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "Invalid source file name".to_string())?;
        let dest_source = non_destructive_target_path(&root.join("source"), file_name);

        tokio::fs::copy(source_file, &dest_source)
            .await
            .map_err(|e| format!("Failed to copy media non-destructively: {e}"))?;

        let sha256 = compute_sha256(&dest_source).await?;
        let inspection = inspect_media(&dest_source).await?;
        let metadata_json = serde_json::to_string(&inspection)
            .map_err(|e| format!("Failed to serialize inspection: {e}"))?;

        let source_asset_id = Uuid::new_v4().to_string();
        let primary_audio = inspection.audio_streams.first();
        let mut assets = vec![MediaAssetRecord {
            id: source_asset_id.clone(),
            project_id: project.id.clone(),
            role: "source_video".to_string(),
            file_path: dest_source.to_string_lossy().into_owned(),
            sha256: Some(sha256),
            duration_ms: Some(inspection.duration_ms),
            width: inspection.video.as_ref().map(|v| v.width),
            height: inspection.video.as_ref().map(|v| v.height),
            fps_num: inspection.video.as_ref().map(|v| v.fps_num),
            fps_den: inspection.video.as_ref().map(|v| v.fps_den),
            audio_channels: primary_audio.map(|a| a.channels),
            sample_rate: primary_audio.map(|a| a.sample_rate),
            metadata_json,
            created_at: chrono_utc_now_approx(),
        }];

        if !inspection.audio_streams.is_empty() {
            let stem = dest_source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("source");
            let ai_wav = root.join("assets/audio").join(format!("{stem}_ai_16k.wav"));
            let mix_wav = root.join("assets/audio").join(format!("{stem}_mix_48k.wav"));
            let wave_json = root
                .join("assets/audio")
                .join(format!("{stem}_waveform.json"));

            extract_audio_wav(&dest_source, &ai_wav, 16_000, 1).await?;
            extract_audio_wav(&dest_source, &mix_wav, 48_000, 2).await?;

            let ai_wav_clone = ai_wav.clone();
            let peaks = tokio::task::spawn_blocking(move || {
                compute_wav_waveform_peaks(&ai_wav_clone, 320)
            })
            .await
            .map_err(|e| format!("Waveform task failed: {e}"))??;

            let peaks_bytes = serde_json::to_vec(&peaks)
                .map_err(|e| format!("Failed to serialize waveform: {e}"))?;
            tokio::fs::write(&wave_json, peaks_bytes)
                .await
                .map_err(|e| format!("Failed to write waveform cache: {e}"))?;

            assets.push(MediaAssetRecord {
                id: Uuid::new_v4().to_string(),
                project_id: project.id.clone(),
                role: "extracted_audio_ai".to_string(),
                file_path: ai_wav.to_string_lossy().into_owned(),
                sha256: None,
                duration_ms: Some(inspection.duration_ms),
                width: None,
                height: None,
                fps_num: None,
                fps_den: None,
                audio_channels: Some(1),
                sample_rate: Some(16_000),
                metadata_json: "{}".to_string(),
                created_at: chrono_utc_now_approx(),
            });
            assets.push(MediaAssetRecord {
                id: Uuid::new_v4().to_string(),
                project_id: project.id.clone(),
                role: "extracted_audio_mix".to_string(),
                file_path: mix_wav.to_string_lossy().into_owned(),
                sha256: None,
                duration_ms: Some(inspection.duration_ms),
                width: None,
                height: None,
                fps_num: None,
                fps_den: None,
                audio_channels: Some(2),
                sample_rate: Some(48_000),
                metadata_json: "{}".to_string(),
                created_at: chrono_utc_now_approx(),
            });
            assets.push(MediaAssetRecord {
                id: Uuid::new_v4().to_string(),
                project_id: project.id.clone(),
                role: "waveform".to_string(),
                file_path: wave_json.to_string_lossy().into_owned(),
                sha256: None,
                duration_ms: Some(inspection.duration_ms),
                width: None,
                height: None,
                fps_num: None,
                fps_den: None,
                audio_channels: None,
                sample_rate: None,
                metadata_json: "{}".to_string(),
                created_at: chrono_utc_now_approx(),
            });
        }

        self.db
            .replace_project_media_assets(&project.id, &assets)
            .await?;

        for asset in &assets {
            if asset.role == "source_video" || asset.role.starts_with("extracted_audio") {
                self.preview_server
                    .register_asset(&asset.id, PathBuf::from(&asset.file_path))
                    .await;
            }
        }

        self.get_project_summary(&project.id).await
    }

    pub async fn get_project_summary(&self, project_id: &str) -> Result<ProjectSummary, String> {
        let project = self
            .db
            .get_project(project_id)
            .await?
            .ok_or_else(|| format!("Project '{project_id}' not found"))?;
        let assets = self.db.list_media_assets(project_id).await?;

        let source_asset = assets.iter().find(|a| a.role == "source_video");
        let source_preview_url = if let Some(src) = source_asset {
            self.preview_server
                .register_asset(&src.id, PathBuf::from(&src.file_path))
                .await;
            Some(self.preview_server.url_for(&src.id))
        } else {
            None
        };

        let inspection = source_asset
            .and_then(|src| serde_json::from_str::<MediaInspection>(&src.metadata_json).ok());

        let waveform_peaks = if let Some(wave) = assets.iter().find(|a| a.role == "waveform") {
            tokio::fs::read(&wave.file_path)
                .await
                .ok()
                .and_then(|b| serde_json::from_slice::<Vec<f32>>(&b).ok())
        } else {
            None
        };

        Ok(ProjectSummary {
            project,
            assets,
            source_preview_url,
            inspection,
            waveform_peaks,
        })
    }
}

fn chrono_utc_now_approx() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}
