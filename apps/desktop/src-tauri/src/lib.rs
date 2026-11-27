pub mod commands;
pub mod core;
pub mod database;
pub mod downloads;
pub mod media;
pub mod resources;
pub mod scheduler;
pub mod worker;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use core::projects::ProjectManager;
use database::AppDatabase;
use downloads::DownloadManager;
use media::preview_server::MediaPreviewServer;
use resources::probe_system_resources;
use scheduler::TaskScheduler;
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

            let sink = Arc::new(TauriWorkerEventSink {
                app: app.handle().clone(),
            });
            let worker = WorkerManager::new(WorkerConfig::discover_default(), sink);
            app.manage(Arc::clone(&worker));

            let app_handle_dl = app.handle().clone();
            let app_handle_sched = app.handle().clone();
            let worker_for_sched = Arc::clone(&worker);

            let (db, preview_server, sys_res, download_mgr, task_scheduler) =
                tauri::async_runtime::block_on(async move {
                    let db = AppDatabase::connect(&db_path)
                        .await
                        .expect("failed to initialize XakuStudio SQLite database");
                    let preview_server = MediaPreviewServer::start()
                        .await
                        .expect("failed to start local media preview server");
                    let sys_res = probe_system_resources().await;

                    let download_mgr = DownloadManager::new(
                        db.clone(),
                        Arc::new(move |dl| {
                            let _ = app_handle_dl.emit("download:updated", dl);
                        }),
                    );

                    let task_scheduler = TaskScheduler::start(
                        db.clone(),
                        worker_for_sched,
                        sys_res.gpu_heavy_slots,
                        sys_res.cpu_light_slots,
                        sys_res.network_io_slots,
                        Arc::new(move |task| {
                            let _ = app_handle_sched.emit("scheduler:task-updated", task);
                        }),
                    )
                    .await
                    .expect("failed to initialize task scheduler");

                    (db, preview_server, sys_res, download_mgr, task_scheduler)
                });

            info!(
                gpu_slots = sys_res.gpu_heavy_slots,
                cpu_slots = sys_res.cpu_light_slots,
                preset = %sys_res.recommended_preset,
                "Initialized resource-aware task scheduler"
            );

            let project_manager = Arc::new(ProjectManager::new(db, preview_server));
            let pm_init = Arc::clone(&project_manager);
            tauri::async_runtime::spawn(async move {
                if let Err(err) = pm_init.register_existing_assets().await {
                    error!("Failed to register existing media assets: {err}");
                }
            });
            app.manage(project_manager);
            app.manage(download_mgr);
            app.manage(task_scheduler);

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
            commands::scheduler::get_system_resources,
            commands::scheduler::list_providers_status,
            commands::scheduler::set_provider_credential,
            commands::scheduler::delete_provider_credential,
            commands::scheduler::list_scheduled_tasks,
            commands::scheduler::enqueue_diagnostic_dag,
            commands::scheduler::cancel_scheduled_task,
            commands::scheduler::retry_scheduled_task,
            commands::scheduler::list_downloads,
            commands::scheduler::start_asset_download,
            commands::scheduler::pause_asset_download,
            commands::scheduler::resume_asset_download,
            commands::scheduler::cancel_asset_download,
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
