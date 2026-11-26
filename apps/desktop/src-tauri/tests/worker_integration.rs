use std::sync::{Arc, Mutex};

use serde_json::json;
use xakustudio_lib::worker::protocol::{
    TaskProgressEvent, WorkerConnectionState, WorkerStatus, PROTOCOL_VERSION,
};
use xakustudio_lib::worker::{WorkerConfig, WorkerEventSink, WorkerManager};

#[derive(Default)]
struct RecordingSink {
    progress_events: Mutex<Vec<TaskProgressEvent>>,
}

impl WorkerEventSink for RecordingSink {
    fn on_status_changed(&self, _status: &WorkerStatus) {}

    fn on_task_progress(&self, progress: &TaskProgressEvent) {
        if let Ok(mut guard) = self.progress_events.lock() {
            guard.push(progress.clone());
        }
    }
}

#[tokio::test]
async fn rust_worker_handshake_health_self_test_and_cancel() {
    let sink = Arc::new(RecordingSink::default());
    let config = WorkerConfig::discover_default();
    let manager = WorkerManager::new(config, sink.clone());

    let status = manager
        .start()
        .await
        .expect("Python 3.12.10 worker should start and complete handshake");

    assert!(matches!(status.state, WorkerConnectionState::Ready));
    assert_eq!(status.protocol_version.as_deref(), Some(PROTOCOL_VERSION));
    assert!(
        status
            .python_version
            .as_deref()
            .unwrap_or_default()
            .starts_with("3.12."),
        "Expected Python 3.12.x, got {:?}",
        status.python_version
    );

    let health = manager.health().await.expect("Health check should succeed");
    assert!(matches!(health.state, WorkerConnectionState::Ready));

    // Execute self-test and confirm progress events arrive.
    let self_test_res = manager
        .call(
            "task.run",
            json!({
                "task_id": "rust-it-1",
                "task_type": "system.self_test",
                "payload": { "steps": 3, "step_delay_ms": 15 }
            }),
        )
        .await
        .expect("Self-test task should complete");

    assert_eq!(
        self_test_res["output"]["status"].as_str(),
        Some("passed")
    );
    {
        let events = sink.progress_events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].percent, 100.0);
    }

    // Spawn a longer self-test and cancel it via task.cancel.
    let manager_clone = Arc::clone(&manager);
    let long_task = tokio::spawn(async move {
        manager_clone
            .call_rpc(
                "task.run",
                json!({
                    "task_id": "rust-it-cancel",
                    "task_type": "system.self_test",
                    "payload": { "steps": 25, "step_delay_ms": 100 }
                }),
            )
            .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    let cancel_res = manager
        .call("task.cancel", json!({ "task_id": "rust-it-cancel" }))
        .await
        .expect("Cancel call should succeed");
    assert_eq!(cancel_res["cancelled"].as_bool(), Some(true));

    let err = long_task
        .await
        .unwrap()
        .expect_err("Cancelled task should return RPC error");
    assert_eq!(err.code, -32001);

    manager.stop().await;
    let stopped = manager.status().await;
    assert!(matches!(stopped.state, WorkerConnectionState::Stopped));
}
