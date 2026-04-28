//! Tauri IPC shell for Progest.
//!
//! This crate is intentionally thin: it wires `progest-core` APIs to
//! Tauri commands that the React frontend calls. Business logic does
//! not live here.

mod accepts_commands;
mod commands;
mod lint_commands;
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
            commands::files_list_all,
            accepts_commands::accepts_read,
            accepts_commands::accepts_write,
            lint_commands::lint_run,
        ])
        .setup(|app| {
            // We build the main window programmatically rather than via
            // tauri.conf.json so we can call `traffic_light_position`
            // on the builder. As of Tauri 2.10.x the JSON-config path
            // doesn't reliably apply that field when titleBarStyle is
            // "Overlay" — only the WebviewWindowBuilder fluent API
            // does. Capabilities reference the "main" label, which we
            // preserve here.
            //
            // Math: titlebar is 40 px tall; macOS traffic light cluster
            // is ~14 px; ((40 − 14) / 2) ≈ 13 vertically. x = 14 keeps
            // the cluster off the left edge by roughly the same amount
            // macOS's Big Sur+ default title bars use.
            let builder = tauri::webview::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::default(),
            )
            .title("Progest")
            .inner_size(1280.0, 800.0)
            .min_inner_size(800.0, 600.0);
            // Shadow the binding inside the cfg branch so non-macOS
            // targets don't see an unused `mut` (clippy `-D warnings`
            // failed CI on Linux otherwise).
            #[cfg(target_os = "macos")]
            let builder = builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true)
                .traffic_light_position(tauri::LogicalPosition::new(14.0, 16.0));
            builder.build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
