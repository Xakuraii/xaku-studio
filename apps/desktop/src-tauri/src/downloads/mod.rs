use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use reqwest::header::RANGE;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::database::{AppDatabase, DownloadRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    Pause,
    Cancel,
}

struct ActiveDownload {
    token: CancellationToken,
    stop_reason: Option<StopReason>,
}

pub type DownloadEventCallback = Arc<dyn Fn(&DownloadRecord) + Send + Sync>;

pub struct DownloadManager {
    db: AppDatabase,
    client: reqwest::Client,
    concurrency: Arc<Semaphore>,
    active: Arc<Mutex<HashMap<String, ActiveDownload>>>,
    on_update: DownloadEventCallback,
}

impl DownloadManager {
    pub fn new(db: AppDatabase, on_update: DownloadEventCallback) -> Arc<Self> {
        let client = reqwest::Client::builder()
            .user_agent("XakuStudio/0.1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Arc::new(Self {
            db,
            client,
            concurrency: Arc::new(Semaphore::new(3)),
            active: Arc::new(Mutex::new(HashMap::new())),
            on_update,
        })
    }

    pub async fn list_downloads(&self) -> Result<Vec<DownloadRecord>, String> {
        self.db.list_downloads().await
    }

    pub async fn enqueue_download(
        self: &Arc<Self>,
        resource_type: &str,
        resource_id: &str,
        url: &str,
        dest_path: &Path,
        sha256_expected: Option<String>,
    ) -> Result<DownloadRecord, String> {
        if let Some(existing) = self.db.find_download_by_resource(resource_id).await? {
            if existing.status == "completed" && Path::new(&existing.dest_path).exists() {
                if let Some(ref expected) = sha256_expected {
                    let actual = compute_file_sha256(Path::new(&existing.dest_path)).await?;
                    if actual.eq_ignore_ascii_case(expected.trim()) {
                        return Ok(existing);
                    }
                } else {
                    return Ok(existing);
                }
            }
        }

        let record = DownloadRecord {
            id: Uuid::new_v4().to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            url: url.to_string(),
            dest_path: dest_path.to_string_lossy().into_owned(),
            total_bytes: None,
            downloaded_bytes: 0,
            sha256_expected,
            status: "queued".to_string(),
            error_message: None,
            updated_at: String::new(),
        };
        self.db.upsert_download(&record).await?;
        let saved = self
            .db
            .get_download(&record.id)
            .await?
            .unwrap_or(record);
        (self.on_update)(&saved);

        self.spawn_download(saved.id.clone()).await;
        Ok(saved)
    }

    pub async fn pause_download(&self, id: &str) -> Result<bool, String> {
        let mut guard = self.active.lock().await;
        if let Some(entry) = guard.get_mut(id) {
            entry.stop_reason = Some(StopReason::Pause);
            entry.token.cancel();
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn cancel_download(&self, id: &str) -> Result<bool, String> {
        let cancelled_active = {
            let mut guard = self.active.lock().await;
            if let Some(entry) = guard.get_mut(id) {
                entry.stop_reason = Some(StopReason::Cancel);
                entry.token.cancel();
                true
            } else {
                false
            }
        };

        if !cancelled_active {
            if let Some(dl) = self.db.get_download(id).await? {
                let part = part_path_for(Path::new(&dl.dest_path));
                let _ = tokio::fs::remove_file(&part).await;
                self.db
                    .update_download_progress(id, dl.downloaded_bytes, dl.total_bytes, "cancelled", None)
                    .await?;
                if let Some(updated) = self.db.get_download(id).await? {
                    (self.on_update)(&updated);
                }
                return Ok(true);
            }
        }
        Ok(cancelled_active)
    }

    pub async fn resume_download(self: &Arc<Self>, id: &str) -> Result<DownloadRecord, String> {
        let dl = self
            .db
            .get_download(id)
            .await?
            .ok_or_else(|| format!("Download '{id}' not found"))?;
        if dl.status == "completed" && Path::new(&dl.dest_path).exists() {
            return Ok(dl);
        }

        {
            let guard = self.active.lock().await;
            if guard.contains_key(id) {
                return Ok(dl);
            }
        }

        self.db
            .update_download_progress(id, dl.downloaded_bytes, dl.total_bytes, "queued", None)
            .await?;
        let updated = self.db.get_download(id).await?.unwrap_or(dl);
        (self.on_update)(&updated);

        self.spawn_download(updated.id.clone()).await;
        Ok(updated)
    }

    async fn spawn_download(self: &Arc<Self>, id: String) {
        let token = CancellationToken::new();
        {
            let mut guard = self.active.lock().await;
            guard.insert(
                id.clone(),
                ActiveDownload {
                    token: token.clone(),
                    stop_reason: None,
                },
            );
        }

        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            mgr.execute_download(&id, token).await;
        });
    }

    async fn execute_download(&self, id: &str, token: CancellationToken) {
        let _permit = tokio::select! {
            p = self.concurrency.acquire() => match p {
                Ok(permit) => permit,
                Err(_) => return,
            },
            _ = token.cancelled() => {
                self.finalize_stopped(id, 0, None).await;
                return;
            }
        };

        let Some(dl) = self.db.get_download(id).await.ok().flatten() else {
            self.active.lock().await.remove(id);
            return;
        };

        let dest = PathBuf::from(&dl.dest_path);
        if let Some(parent) = dest.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                self.fail_download(id, 0, None, format!("Failed to create destination dir: {e}"))
                    .await;
                return;
            }
        }

        let part_path = part_path_for(&dest);
        let mut existing_bytes = tokio::fs::metadata(&part_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let mut req = self.client.get(&dl.url);
        if existing_bytes > 0 {
            req = req.header(RANGE, format!("bytes={existing_bytes}-"));
        }

        let response = tokio::select! {
            res = req.send() => res,
            _ = token.cancelled() => {
                self.finalize_stopped(id, existing_bytes as i64, dl.total_bytes).await;
                return;
            }
        };

        let resp = match response {
            Ok(r) => r,
            Err(e) => {
                self.fail_download(id, existing_bytes as i64, dl.total_bytes, format!("HTTP request failed: {e}"))
                    .await;
                return;
            }
        };

        let status = resp.status();
        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            self.fail_download(
                id,
                existing_bytes as i64,
                dl.total_bytes,
                format!("Server returned HTTP {}", status),
            )
            .await;
            return;
        }

        let (append_mode, total_bytes) = if status == StatusCode::PARTIAL_CONTENT && existing_bytes > 0 {
            let total = resp
                .content_length()
                .map(|rem| (existing_bytes + rem) as i64)
                .or(dl.total_bytes);
            (true, total)
        } else {
            existing_bytes = 0;
            let total = resp.content_length().map(|len| len as i64);
            (false, total)
        };

        let mut file = if append_mode {
            match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&part_path)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    self.fail_download(id, existing_bytes as i64, total_bytes, format!("Failed to open .part file: {e}"))
                        .await;
                    return;
                }
            }
        } else {
            match tokio::fs::File::create(&part_path).await {
                Ok(f) => f,
                Err(e) => {
                    self.fail_download(id, 0, total_bytes, format!("Failed to create .part file: {e}"))
                        .await;
                    return;
                }
            }
        };

        let mut downloaded = existing_bytes;
        let _ = self
            .db
            .update_download_progress(id, downloaded as i64, total_bytes, "downloading", None)
            .await;
        if let Some(updated) = self.db.get_download(id).await.ok().flatten() {
            (self.on_update)(&updated);
        }

        let mut stream = resp.bytes_stream();
        let mut last_persisted = downloaded;

        loop {
            let next_chunk = tokio::select! {
                c = stream.next() => c,
                _ = token.cancelled() => {
                    let _ = file.flush().await;
                    drop(file);
                    self.finalize_stopped(id, downloaded as i64, total_bytes).await;
                    return;
                }
            };

            match next_chunk {
                Some(Ok(chunk)) => {
                    if let Err(e) = file.write_all(&chunk).await {
                        self.fail_download(id, downloaded as i64, total_bytes, format!("File write failed: {e}"))
                            .await;
                        return;
                    }
                    downloaded += chunk.len() as u64;
                    if downloaded - last_persisted >= 64 * 1024 {
                        last_persisted = downloaded;
                        let _ = self
                            .db
                            .update_download_progress(id, downloaded as i64, total_bytes, "downloading", None)
                            .await;
                        if let Some(updated) = self.db.get_download(id).await.ok().flatten() {
                            (self.on_update)(&updated);
                        }
                    }
                }
                Some(Err(e)) => {
                    self.fail_download(id, downloaded as i64, total_bytes, format!("Stream error: {e}"))
                        .await;
                    return;
                }
                None => break,
            }
        }

        if let Err(e) = file.flush().await {
            self.fail_download(id, downloaded as i64, total_bytes, format!("Flush failed: {e}"))
                .await;
            return;
        }
        drop(file);

        if let Some(ref expected) = dl.sha256_expected {
            if !expected.trim().is_empty() {
                match compute_file_sha256(&part_path).await {
                    Ok(actual) if actual.eq_ignore_ascii_case(expected.trim()) => {}
                    Ok(actual) => {
                        let _ = tokio::fs::remove_file(&part_path).await;
                        self.fail_download(
                            id,
                            downloaded as i64,
                            total_bytes,
                            format!("SHA-256 mismatch (expected {expected}, got {actual})"),
                        )
                        .await;
                        return;
                    }
                    Err(e) => {
                        self.fail_download(id, downloaded as i64, total_bytes, e)
                            .await;
                        return;
                    }
                }
            }
        }

        if dest.exists() {
            let _ = tokio::fs::remove_file(&dest).await;
        }
        if let Err(e) = tokio::fs::rename(&part_path, &dest).await {
            self.fail_download(id, downloaded as i64, total_bytes, format!("Rename failed: {e}"))
                .await;
            return;
        }

        self.active.lock().await.remove(id);
        let _ = self
            .db
            .update_download_progress(
                id,
                downloaded as i64,
                total_bytes.or(Some(downloaded as i64)),
                "completed",
                None,
            )
            .await;
        if let Some(updated) = self.db.get_download(id).await.ok().flatten() {
            (self.on_update)(&updated);
        }
    }

    async fn finalize_stopped(&self, id: &str, downloaded_bytes: i64, total_bytes: Option<i64>) {
        let stop_reason = {
            let mut guard = self.active.lock().await;
            guard.remove(id).and_then(|e| e.stop_reason)
        };
        let status = match stop_reason {
            Some(StopReason::Pause) => "paused",
            _ => {
                if let Some(dl) = self.db.get_download(id).await.ok().flatten() {
                    let _ = tokio::fs::remove_file(part_path_for(Path::new(&dl.dest_path))).await;
                }
                "cancelled"
            }
        };
        let _ = self
            .db
            .update_download_progress(id, downloaded_bytes, total_bytes, status, None)
            .await;
        if let Some(updated) = self.db.get_download(id).await.ok().flatten() {
            (self.on_update)(&updated);
        }
    }

    async fn fail_download(
        &self,
        id: &str,
        downloaded_bytes: i64,
        total_bytes: Option<i64>,
        err_msg: String,
    ) {
        self.active.lock().await.remove(id);
        let _ = self
            .db
            .update_download_progress(
                id,
                downloaded_bytes,
                total_bytes,
                "failed",
                Some(&err_msg),
            )
            .await;
        if let Some(updated) = self.db.get_download(id).await.ok().flatten() {
            (self.on_update)(&updated);
        }
    }
}

fn part_path_for(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.part", dest.display()))
}

pub async fn compute_file_sha256(path: &Path) -> Result<String, String> {
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
