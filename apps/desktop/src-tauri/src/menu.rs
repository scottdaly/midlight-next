// Native macOS menu implementation
// This provides the standard macOS menu bar for the application

use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    AppHandle, Emitter, Runtime, Wry,
};

/// Create the native macOS menu bar
pub fn create_menu(app: &AppHandle<Wry>) -> Result<Menu<Wry>, tauri::Error> {
    // App menu (Midlight)
    let app_menu = SubmenuBuilder::new(app, "Midlight")
        .item(&PredefinedMenuItem::about(
            app,
            Some("About Midlight"),
            None,
        )?)
        .separator()
        .item(&MenuItemBuilder::with_id("check_for_updates", "Check for Updates...").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("settings", "Settings...")
                .accelerator("CmdOrCtrl+,")
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    // File menu
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(
            &MenuItemBuilder::with_id("new_document", "New Document")
                .accelerator("CmdOrCtrl+N")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("open_workspace", "Open Workspace...")
                .accelerator("CmdOrCtrl+O")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id("import_docx", "Import Word Document...").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("save", "Save")
                .accelerator("CmdOrCtrl+S")
                .build(app)?,
        )
        .separator()
        .item(&MenuItemBuilder::with_id("export_docx", "Export as Word Document...").build(app)?)
        .item(&MenuItemBuilder::with_id("export_pdf", "Export as PDF...").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("close_tab", "Close Tab")
                .accelerator("CmdOrCtrl+W")
                .build(app)?,
        )
        .build()?;

    // Edit menu
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id("find", "Find...")
                .accelerator("CmdOrCtrl+F")
                .build(app)?,
        )
        .build()?;

    // View menu
    let view_menu = SubmenuBuilder::new(app, "View")
        .item(
            &MenuItemBuilder::with_id("toggle_ai_panel", "Toggle AI Panel")
                .accelerator("CmdOrCtrl+Shift+A")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("toggle_versions_panel", "Toggle Versions Panel")
                .accelerator("CmdOrCtrl+Shift+V")
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;

    // Window menu
    let window_menu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    // Help menu
    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(&MenuItemBuilder::with_id("documentation", "Documentation").build(app)?)
        .item(&MenuItemBuilder::with_id("report_issue", "Report an Issue").build(app)?)
        .build()?;

    // Build the complete menu bar
    MenuBuilder::new(app)
        .item(&app_menu)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&window_menu)
        .item(&help_menu)
        .build()
}

/// Handle menu events by emitting to the frontend
pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event_id: &str) {
    // Map menu IDs to frontend events
    let frontend_event = match event_id {
        // App menu
        "settings" => Some("menu:settings"),
        "check_for_updates" => Some("menu:check-for-updates"),

        // File menu
        "new_document" => Some("menu:new-document"),
        "open_workspace" => Some("menu:open-workspace"),
        "import_docx" => Some("menu:import-docx"),
        "save" => Some("menu:save"),
        "export_docx" => Some("menu:export-docx"),
        "export_pdf" => Some("menu:export-pdf"),
        "close_tab" => Some("menu:close-tab"),

        // Edit menu
        "find" => Some("menu:find"),

        // View menu
        "toggle_ai_panel" => Some("menu:toggle-ai-panel"),
        "toggle_versions_panel" => Some("menu:toggle-versions-panel"),

        // Help menu
        "documentation" => Some("menu:documentation"),
        "report_issue" => Some("menu:report-issue"),

        // Predefined menu items are handled by Tauri automatically
        _ => None,
    };

    if let Some(event) = frontend_event {
        // Emit to all windows
        let _ = app.emit(event, ());
    }
}
