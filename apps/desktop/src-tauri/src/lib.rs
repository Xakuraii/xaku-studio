pub mod commands;
pub mod worker;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use worker::protocol::{TaskProgressEvent, WorkerStatus};
use worker::{WorkerConfig, WorkerEventSink, WorkerManager};

struct TauriWorkerEventSink {
    app: AppHandle,
}

impl WorkerEventSink for TauriWorkerEventSink {
    fn on_status_changed(&self, status: &WorkerStatus) {
        let _ = self.app.emit("worker:status", status);
    }

    fn on_task_progress(&self, progress: &TaskProgressEvent) {
        let _ = self.app.emit("worker:progress", progress);
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("xakustudio=info,python_worker=info")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let sink = Arc::new(TauriWorkerEventSink {
                app: app.handle().clone(),
            });
            let worker = WorkerManager::new(WorkerConfig::discover_default(), sink);
            app.manage(Arc::clone(&worker));

            tauri::async_runtime::spawn(async move {
                info!("Starting persistent Python AI worker");
                if let Err(err) = worker.start().await {
                    error!("Initial Python worker startup failed: {err}");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::worker::get_worker_status,
            commands::worker::ping_worker,
            commands::worker::restart_worker,
            commands::worker::run_worker_self_test,
            commands::worker::cancel_worker_task,
        ])
        .build(tauri::generate_context!())
        .expect("error while building XakuStudio Tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(worker) = app_handle.try_state::<Arc<WorkerManager>>() {
                    let worker = Arc::clone(&worker);
                    tauri::async_runtime::block_on(async move {
                        worker.stop().await;
                    });
                }
            }
        });
}
