use tauri::menu::{Menu, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Wry};

pub fn build_menu(app: &AppHandle<Wry>) -> tauri::Result<Menu<Wry>> {
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&MenuItemBuilder::with_id("menu:new-project", "New Project…").build(app)?)
        .item(
            &MenuItemBuilder::with_id("menu:open-project", "Open Project…")
                .accelerator("CmdOrCtrl+O")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu:new-file", "New File")
                .accelerator("CmdOrCtrl+N")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu:new-folder", "New Folder")
                .accelerator("CmdOrCtrl+Shift+N")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu:import", "Import…")
                .accelerator("CmdOrCtrl+I")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu:settings", "Settings…")
                .accelerator("CmdOrCtrl+,")
                .build(app)?,
        )
        .separator()
        .close_window()
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(
            &MenuItemBuilder::with_id("menu:undo", "Undo")
                .accelerator("CmdOrCtrl+Z")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("menu:redo", "Redo")
                .accelerator("CmdOrCtrl+Shift+Z")
                .build(app)?,
        )
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .item(&MenuItemBuilder::with_id("menu:toggle-tree", "Toggle Tree Panel").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:toggle-flat", "Toggle Flat Panel").build(app)?)
        .item(&MenuItemBuilder::with_id("menu:toggle-inspector", "Toggle Inspector").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu:palette", "Command Palette…")
                .accelerator("CmdOrCtrl+K")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("menu:rescan", "Rescan Project")
                .accelerator("CmdOrCtrl+Shift+R")
                .build(app)?,
        )
        .build()?;

    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(&MenuItemBuilder::with_id("menu:about", "About Progest").build(app)?)
        .build()?;

    #[cfg(target_os = "macos")]
    let menu = {
        let app_menu = SubmenuBuilder::new(app, "Progest")
            .about(None)
            .separator()
            .item(
                &MenuItemBuilder::with_id("menu:settings-app", "Settings…")
                    .accelerator("CmdOrCtrl+,")
                    .build(app)?,
            )
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()?;
        Menu::with_items(
            app,
            &[&app_menu, &file_menu, &edit_menu, &view_menu, &help_menu],
        )?
    };

    #[cfg(not(target_os = "macos"))]
    let menu = Menu::with_items(app, &[&file_menu, &edit_menu, &view_menu, &help_menu])?;

    Ok(menu)
}

pub fn handle_menu_event(app: &AppHandle<Wry>, id: &str) {
    let _ = app.emit("menu-action", id);
}
