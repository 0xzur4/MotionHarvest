pub mod blender_bridge;
pub mod commands;
pub mod db;
pub mod kinematics;
pub mod model_manager;
pub mod models;
pub mod pipeline;

#[cfg(not(test))]
use crate::commands::AppState;
#[cfg(not(test))]
use crate::db::{default_vault_dir, VaultDb};
#[cfg(not(test))]
use crate::pipeline::queue::QueueController;
#[cfg(not(test))]
use std::sync::{Arc, Mutex};
#[cfg(not(test))]
use tauri::Manager;

#[cfg(not(test))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let vault_dir = default_vault_dir();
            let vault_db = VaultDb::open(&vault_dir)
                .expect("Failed to open MotionHarvest Vault SQLite database");

            // Crash-safe startup recovery (Step 2)
            let _ = vault_db.recover_interrupted_jobs();

            let db_arc = Arc::new(Mutex::new(vault_db));
            let res_dir = app.path().resource_dir().ok();
            let queue = Arc::new(QueueController::new(db_arc.clone(), res_dir));

            queue.clone().spawn_worker(app.handle().clone());

            app.manage(AppState {
                db: db_arc,
                queue,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::check_models,
            commands::download_models,
            commands::enqueue_sources,
            commands::list_jobs,
            commands::delete_job,
            commands::queue_control,
            commands::query_vault,
            commands::get_clip_preview,
            commands::recompile_clip,
            commands::blend_clips,
            commands::toggle_favorite,
            commands::rename_clip,
            commands::delete_clips,
            commands::manage_collections,
            commands::export_clips,
            commands::ping_blender,
            commands::send_to_blender,
            commands::copy_blender_addon,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MotionHarvest Studio");
}
