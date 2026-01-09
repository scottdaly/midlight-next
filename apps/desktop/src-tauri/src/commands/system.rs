// System commands - platform operations like reveal in finder, open URLs

use std::path::Path;
use std::process::Command;

/// Show a file or folder in the system file manager (Finder/Explorer)
#[tauri::command]
pub fn show_in_folder(path: String) -> Result<(), String> {
    let path = Path::new(&path);

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", path.to_str().unwrap_or("")])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .args(["/select,", path.to_str().unwrap_or("")])
            .spawn()
            .map_err(|e| format!("Failed to open Explorer: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, we open the parent directory since most file managers
        // don't support selecting a specific file
        let parent = path.parent().unwrap_or(path);
        Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {}", e))?;
    }

    Ok(())
}

/// Open a URL in the default browser
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

/// Get the current app version from Cargo.toml
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get platform information
#[tauri::command]
pub fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[derive(serde::Serialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
}
