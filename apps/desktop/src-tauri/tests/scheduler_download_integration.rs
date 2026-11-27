use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use uuid::Uuid;

use xakustudio_lib::database::{AppDatabase, TaskRecord};
use xakustudio_lib::downloads::DownloadManager;
use xakustudio_lib::resources::credentials::CredentialStore;
use xakustudio_lib::scheduler::{NewTaskSpec, ResourceClass, TaskScheduler};
use xakustudio_lib::worker::{NoopEventSink, WorkerConfig, WorkerManager};

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn windows_credential_store_roundtrip() {
    let key = format!("TEST_KEY_{}", Uuid::new_v4());
    assert!(!CredentialStore::has_secret(&key));
    assert_eq!(CredentialStore::get_secret(&key).unwrap(), None);

    CredentialStore::set_secret(&key, "super-secret-value-123").unwrap();
    assert!(CredentialStore::has_secret(&key));
    assert_eq!(
        CredentialStore::get_secret(&key).unwrap().as_deref(),
        Some("super-secret-value-123")
    );

    CredentialStore::delete_secret(&key).unwrap();
    assert!(!CredentialStore::has_secret(&key));
    assert_eq!(CredentialStore::get_secret(&key).unwrap(), None);
}

#[tokio::test]
async fn scheduler_crash_recovery_dag_gpu_exclusivity_and_cancel_retry() {
    let db = AppDatabase::connect_in_memory().await.unwrap();

    // Seed a leftover running task to verify startup crash recovery.
    let stale_id = Uuid::new_v4().to_string();
    db.insert_task_dag(
        &[TaskRecord {
            id: stale_id.clone(),
            project_id: None,
            pipeline_run_id: Some("old-run".into()),
            task_type: "system.self_test".into(),
            resource_class: "gpu_heavy".into(),
            status: "running".into(),
            progress: 40.0,
            stage_message: "Running before crash".into(),
            input_json: "{}".into(),
            output_json: "{}".into(),
            error_code: None,
            error_message: None,
            retry_count: 0,
            max_retries: 2,
            started_at: None,
            completed_at: None,
            created_at: String::new(),
        }],
        &[],
    )
    .await
    .unwrap();

    let timings: Arc<Mutex<HashMap<String, (Option<Instant>, Option<Instant>)>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let timings_cb = Arc::clone(&timings);

    let worker = WorkerManager::new(WorkerConfig::discover_default(), Arc::new(NoopEventSink));
    let scheduler = TaskScheduler::start(
        db.clone(),
        worker,
        1, // Strict 1 GPU-heavy slot
        4,
        3,
        Arc::new(move |task| {
            let mut guard = timings_cb.lock().unwrap();
            let entry = guard.entry(task.id.clone()).or_insert((None, None));
            if task.status == "running" && entry.0.is_none() {
                entry.0 = Some(Instant::now());
            }
            if task.status == "completed" {
                entry.1 = Some(Instant::now());
            }
        }),
    )
    .await
    .unwrap();

    let recovered = db.get_task(&stale_id).await.unwrap().unwrap();
    assert_eq!(recovered.status, "interrupted");

    // Build 4-node DAG: root -> (gpu_a, gpu_b) -> merge
    let root_id = format!("root-{}", Uuid::new_v4());
    let gpu_a_id = format!("gpu-a-{}", Uuid::new_v4());
    let gpu_b_id = format!("gpu-b-{}", Uuid::new_v4());
    let merge_id = format!("merge-{}", Uuid::new_v4());

    scheduler
        .enqueue_dag(
            None,
            Some("run-1".into()),
            vec![
                NewTaskSpec {
                    id: root_id.clone(),
                    task_type: "test.sleep".into(),
                    resource_class: ResourceClass::CpuLight,
                    input_json: json!({ "mode": "local_sleep", "duration_ms": 40 }),
                    depends_on: vec![],
                    max_retries: 1,
                },
                NewTaskSpec {
                    id: gpu_a_id.clone(),
                    task_type: "test.sleep".into(),
                    resource_class: ResourceClass::GpuHeavy,
                    input_json: json!({ "mode": "local_sleep", "duration_ms": 90 }),
                    depends_on: vec![root_id.clone()],
                    max_retries: 1,
                },
                NewTaskSpec {
                    id: gpu_b_id.clone(),
                    task_type: "test.sleep".into(),
                    resource_class: ResourceClass::GpuHeavy,
                    input_json: json!({ "mode": "local_sleep", "duration_ms": 90 }),
                    depends_on: vec![root_id.clone()],
                    max_retries: 1,
                },
                NewTaskSpec {
                    id: merge_id.clone(),
                    task_type: "test.sleep".into(),
                    resource_class: ResourceClass::CpuLight,
                    input_json: json!({ "mode": "local_sleep", "duration_ms": 30 }),
                    depends_on: vec![gpu_a_id.clone(), gpu_b_id.clone()],
                    max_retries: 1,
                },
            ],
        )
        .await
        .unwrap();

    // Poll until merge completes (up to 5s).
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(t) = db.get_task(&merge_id).await.unwrap() {
            if t.status == "completed" {
                break;
            }
        }
        assert!(Instant::now() < deadline, "DAG did not complete in time");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let map = timings.lock().unwrap();
    let (r_start, r_end) = map[&root_id];
    let (a_start, a_end) = map[&gpu_a_id];
    let (b_start, b_end) = map[&gpu_b_id];
    let (m_start, _) = map[&merge_id];

    // Dependency ordering checks
    assert!(a_start.unwrap() >= r_end.unwrap());
    assert!(b_start.unwrap() >= r_end.unwrap());
    assert!(m_start.unwrap() >= a_end.unwrap());
    assert!(m_start.unwrap() >= b_end.unwrap());
    let _ = r_start;

    // Exclusive GPU-heavy slot check: gpu_a and gpu_b intervals must not overlap.
    let no_overlap = a_end.unwrap() <= b_start.unwrap() || b_end.unwrap() <= a_start.unwrap();
    assert!(
        no_overlap,
        "GpuHeavy tasks overlapped! a=({:?},{:?}) b=({:?},{:?})",
        a_start, a_end, b_start, b_end
    );
    drop(map);

    // Cancel and retry check
    let cancel_id = format!("cancel-{}", Uuid::new_v4());
    scheduler
        .enqueue_dag(
            None,
            None,
            vec![NewTaskSpec {
                id: cancel_id.clone(),
                task_type: "test.sleep".into(),
                resource_class: ResourceClass::CpuLight,
                input_json: json!({ "mode": "local_sleep", "duration_ms": 600 }),
                depends_on: vec![],
                max_retries: 2,
            }],
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(scheduler.cancel_task(&cancel_id).await.unwrap());

    tokio::time::sleep(Duration::from_millis(60)).await;
    let cancelled = db.get_task(&cancel_id).await.unwrap().unwrap();
    assert_eq!(cancelled.status, "cancelled");

    // Retry should increment retry_count and finish cleanly if shorter/re-enqueued
    scheduler.retry_task(&cancel_id).await.unwrap();
    let deadline2 = Instant::now() + Duration::from_secs(5);
    loop {
        let t = db.get_task(&cancel_id).await.unwrap().unwrap();
        if t.status == "completed" {
            assert_eq!(t.retry_count, 1);
            break;
        }
        assert!(Instant::now() < deadline2, "Retried task did not complete");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn start_range_test_server(payload: Arc<Vec<u8>>, chunk_delay_ms: u64) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let data = Arc::clone(&payload);
            tokio::spawn(async move {
                let (reader, mut writer) = stream.into_split();
                let mut br = BufReader::new(reader);
                let mut req_line = String::new();
                if br.read_line(&mut req_line).await.unwrap_or(0) == 0 {
                    return;
                }
                let mut range_start = 0usize;
                let mut has_range = false;
                loop {
                    let mut line = String::new();
                    let n = br.read_line(&mut line).await.unwrap_or(0);
                    if n == 0 || line == "\r\n" || line == "\n" {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("range") {
                            if let Some(s) = v.trim().strip_prefix("bytes=") {
                                if let Some((start_s, _)) = s.split_once('-') {
                                    if let Ok(parsed) = start_s.parse::<usize>() {
                                        range_start = parsed.min(data.len());
                                        has_range = true;
                                    }
                                }
                            }
                        }
                    }
                }

                let slice = &data[range_start..];
                let head = if has_range {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        slice.len(),
                        range_start,
                        data.len().saturating_sub(1),
                        data.len()
                    )
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        slice.len()
                    )
                };
                if writer.write_all(head.as_bytes()).await.is_err() {
                    return;
                }

                for chunk in slice.chunks(32 * 1024) {
                    if writer.write_all(chunk).await.is_err() {
                        break;
                    }
                    let _ = writer.flush().await;
                    if chunk_delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(chunk_delay_ms)).await;
                    }
                }
            });
        }
    });

    port
}

#[tokio::test]
async fn download_manager_pause_resume_duplicate_and_sha256_verification() {
    let temp_root = std::env::temp_dir().join(format!("xakustudio_dl_{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_root).await.unwrap();
    let _guard = TempDirGuard(temp_root.clone());

    let db = AppDatabase::connect_in_memory().await.unwrap();
    let mgr = DownloadManager::new(db.clone(), Arc::new(|_| {}));

    // 512 KiB deterministic payload
    let payload: Vec<u8> = (0..(512 * 1024)).map(|i| (i % 251) as u8).collect();
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let expected_sha: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let port = start_range_test_server(Arc::new(payload.clone()), 15).await;
    let url = format!("http://127.0.0.1:{port}/model.bin");
    let dest = temp_root.join("model.bin");

    // 1. Start download, pause mid-stream, verify .part exists, then resume
    let dl = mgr
        .enqueue_download("model", "test-model-1", &url, &dest, Some(expected_sha.clone()))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(mgr.pause_download(&dl.id).await.unwrap());

    let deadline_pause = Instant::now() + Duration::from_secs(3);
    loop {
        let cur = db.get_download(&dl.id).await.unwrap().unwrap();
        if cur.status == "paused" {
            assert!(cur.downloaded_bytes > 0);
            assert!(PathBuf::from(format!("{}.part", dest.display())).exists());
            break;
        }
        assert!(Instant::now() < deadline_pause, "Download did not pause in time");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    mgr.resume_download(&dl.id).await.unwrap();
    let deadline_done = Instant::now() + Duration::from_secs(5);
    loop {
        let cur = db.get_download(&dl.id).await.unwrap().unwrap();
        if cur.status == "completed" {
            assert_eq!(cur.downloaded_bytes, payload.len() as i64);
            break;
        }
        assert_ne!(cur.status, "failed", "Resumed download failed: {:?}", cur.error_message);
        assert!(Instant::now() < deadline_done, "Download did not finish");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), payload);

    // 2. Duplicate resource_id should return completed record immediately
    let dup = mgr
        .enqueue_download("model", "test-model-1", &url, &dest, Some(expected_sha))
        .await
        .unwrap();
    assert_eq!(dup.id, dl.id);
    assert_eq!(dup.status, "completed");

    // 3. Mismatched SHA-256 should fail and clean up .part
    let bad_dest = temp_root.join("bad_model.bin");
    let bad_dl = mgr
        .enqueue_download(
            "model",
            "test-model-bad",
            &url,
            &bad_dest,
            Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
        )
        .await
        .unwrap();

    let deadline_bad = Instant::now() + Duration::from_secs(5);
    loop {
        let cur = db.get_download(&bad_dl.id).await.unwrap().unwrap();
        if cur.status == "failed" {
            assert!(cur
                .error_message
                .unwrap_or_default()
                .contains("SHA-256 mismatch"));
            assert!(!bad_dest.exists());
            break;
        }
        assert!(Instant::now() < deadline_bad, "Bad checksum did not fail");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
