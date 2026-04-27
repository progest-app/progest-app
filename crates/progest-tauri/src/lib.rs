//! Tauri IPC shell for Progest.
//!
//! This crate is intentionally thin: it wires `progest-core` APIs to
//! Tauri commands that the React frontend calls. Business logic does
//! not live here.

mod commands;
mod recent;
mod state;

use state::AppState;

/// Initializes logging and runs the Tauri application.
///
/// # Panics
///
/// Panics if the Tauri runtime fails to build or run. Tauri's own error
/// reporting is surfaced before the panic.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let app_state = AppState::default();
    match state::discover_initial_project() {
        Ok(Some(ctx)) => {
            tracing::info!("attached to project at {}", ctx.root.root().display());
            *app_state
                .project
                .lock()
                .expect("project mutex poisoned at startup") = Some(ctx);
        }
        Ok(None) => {
            tracing::info!(
                "no Progest project found from CWD or PROGEST_PROJECT — launching empty state"
            );
        }
        Err(e) => {
            tracing::error!("failed to attach project: {e}");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::project_open,
            commands::project_recent_list,
            commands::project_recent_clear,
            commands::search_execute,
            commands::search_history_list,
            commands::search_history_clear,
            commands::view_list,
            commands::view_save,
            commands::view_delete,
            commands::files_list_dir,
        ])
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
