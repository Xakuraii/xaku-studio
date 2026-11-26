pub mod commands;
pub mod core;
pub mod database;
pub mod media;
pub mod worker;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use core::projects::ProjectManager;
use database::AppDatabase;
use media::preview_server::MediaPreviewServer;
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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve Tauri app data directory");
            let db_path = app_data_dir.join("xakustudio.db");

            let (db, preview_server) = tauri::async_runtime::block_on(async {
                let db = AppDatabase::connect(&db_path)
                    .await
                    .expect("failed to initialize XakuStudio SQLite database");
                let preview_server = MediaPreviewServer::start()
                    .await
                    .expect("failed to start local media preview server");
                (db, preview_server)
            });

            let project_manager = Arc::new(ProjectManager::new(db, preview_server));
            let pm_init = Arc::clone(&project_manager);
            tauri::async_runtime::spawn(async move {
                if let Err(err) = pm_init.register_existing_assets().await {
                    error!("Failed to register existing media assets: {err}");
                }
            });
            app.manage(project_manager);

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
            commands::projects::get_default_projects_dir,
            commands::projects::get_media_engine_status,
            commands::projects::list_projects,
            commands::projects::create_project,
            commands::projects::open_project_manifest,
            commands::projects::get_project_summary,
            commands::projects::rename_project,
            commands::projects::delete_project,
            commands::projects::import_project_media,
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
