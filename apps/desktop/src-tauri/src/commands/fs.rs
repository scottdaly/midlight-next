// File system commands

use chrono;
use dirs;
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub node_type: String, // "file" or "directory"
    pub category: Option<String>,
}

fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()[..16].to_string()
}

fn categorize_file(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "midlight" => "midlight".to_string(),
        "md" => "native".to_string(),
        "txt" | "json" => "compatible".to_string(),
        "docx" => "importable".to_string(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "pdf" => "viewable".to_string(),
        _ => "unsupported".to_string(),
    }
}

/// Check if a file should be shown in the file tree
fn should_show_file(name: &str) -> bool {
    // Hide hidden files
    if name.starts_with('.') {
        return false;
    }
    // Hide backup files
    if name.ends_with(".backup") {
        return false;
    }
    // Hide sidecar files
    if name.ends_with(".sidecar.json") {
        return false;
    }
    true
}

/// Get the default workspace path (Documents/Midlight-docs) and create it if needed
#[tauri::command]
pub async fn get_default_workspace() -> Result<String, String> {
    let documents_dir = dirs::document_dir()
        .ok_or_else(|| "Could not determine Documents directory".to_string())?;

    let workspace_path = documents_dir.join("Midlight-docs");

    // Create the directory if it doesn't exist
    if !workspace_path.exists() {
        fs::create_dir_all(&workspace_path)
            .map_err(|e| format!("Failed to create workspace: {}", e))?;
    }

    // Initialize workspace if .midlight folder doesn't exist
    let midlight_dir = workspace_path.join(".midlight");
    if !midlight_dir.exists() {
        fs::create_dir_all(midlight_dir.join("objects")).map_err(|e| e.to_string())?;
    }

    Ok(workspace_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn read_dir(path: String) -> Result<Vec<FileNode>, String> {
    let path = Path::new(&path);

    if !path.exists() {
        return Err(format!("Directory does not exist: {}", path.display()));
    }

    let mut entries = Vec::new();
    let read_dir = fs::read_dir(path).map_err(|e| e.to_string())?;

    for entry in read_dir.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let file_path = entry.path();

        // Skip files that shouldn't be shown
        if !should_show_file(&file_name) {
            continue;
        }

        let is_dir = file_path.is_dir();
        let category = if is_dir {
            None
        } else {
            Some(categorize_file(&file_name))
        };

        entries.push(FileNode {
            id: generate_id(),
            name: file_name,
            path: file_path.to_string_lossy().to_string(),
            node_type: if is_dir { "directory" } else { "file" }.to_string(),
            category,
        });
    }

    // Sort: directories first, then alphabetically
    entries.sort_by(|a, b| {
        if a.node_type == b.node_type {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else if a.node_type == "directory" {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });

    Ok(entries)
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    fs::write(&path, content).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
pub async fn delete_file(path: String) -> Result<(), String> {
    let path = Path::new(&path);

    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn rename_file(old_path: String, new_path: String) -> Result<(), String> {
    fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    // Also rename sidecar if exists
    let old_sidecar = format!("{}.sidecar.json", old_path);
    let new_sidecar = format!("{}.sidecar.json", new_path);
    if Path::new(&old_sidecar).exists() {
        let _ = fs::rename(&old_sidecar, &new_sidecar);
    }

    Ok(())
}

#[tauri::command]
pub async fn file_exists(path: String) -> Result<bool, String> {
    Ok(Path::new(&path).exists())
}

#[tauri::command]
pub async fn create_folder(path: String) -> Result<(), String> {
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

/// Create a new .midlight file with initial empty content
#[tauri::command]
pub async fn create_midlight_file(parent_path: String, name: String) -> Result<FileNode, String> {
    // Ensure name has .midlight extension
    let file_name = if name.ends_with(".midlight") {
        name
    } else {
        format!("{}.midlight", name)
    };

    let file_path = Path::new(&parent_path).join(&file_name);

    // Check if file already exists
    if file_path.exists() {
        return Err(format!("File already exists: {}", file_path.display()));
    }

    // Create empty MidlightDocument
    let now = chrono::Utc::now().to_rfc3339();
    let content = serde_json::json!({
        "version": 1,
        "meta": {
            "created": now,
            "modified": now
        },
        "document": {
            "defaultFont": "Merriweather",
            "defaultFontSize": 16
        },
        "content": {
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        }
    });

    fs::write(&file_path, serde_json::to_string_pretty(&content).unwrap())
        .map_err(|e| format!("Failed to create file: {}", e))?;

    Ok(FileNode {
        id: generate_id(),
        name: file_name,
        path: file_path.to_string_lossy().to_string(),
        node_type: "file".to_string(),
        category: Some("midlight".to_string()),
    })
}

/// Create a new folder and return its FileNode
#[tauri::command]
pub async fn create_new_folder(parent_path: String, name: String) -> Result<FileNode, String> {
    let folder_path = Path::new(&parent_path).join(&name);

    // Check if folder already exists
    if folder_path.exists() {
        return Err(format!("Folder already exists: {}", folder_path.display()));
    }

    fs::create_dir_all(&folder_path).map_err(|e| format!("Failed to create folder: {}", e))?;

    Ok(FileNode {
        id: generate_id(),
        name,
        path: folder_path.to_string_lossy().to_string(),
        node_type: "directory".to_string(),
        category: None,
    })
}

// ============== NEW FILE OPERATIONS ==============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateResult {
    pub new_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<(String, String)>, // (path, error message)
}

/// Duplicate a file or folder, creating a "-Copy" suffix
#[tauri::command]
pub async fn file_duplicate(path: String) -> Result<DuplicateResult, String> {
    let src = Path::new(&path);

    if !src.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    let file_name = src
        .file_name()
        .ok_or_else(|| "Invalid path".to_string())?
        .to_string_lossy();

    let parent = src
        .parent()
        .ok_or_else(|| "Cannot get parent directory".to_string())?;

    // For files: "document.md" -> "document-Copy.md", "document-Copy 2.md", etc.
    // For folders: "folder" -> "folder-Copy", "folder-Copy 2", etc.
    let (stem, ext) = if src.is_dir() {
        (file_name.to_string(), String::new())
    } else {
        let stem = src
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_name.to_string());
        let ext = src
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        (stem, ext)
    };

    // Find unique name
    let mut counter = 0;
    let new_path = loop {
        let suffix = if counter == 0 {
            "-Copy".to_string()
        } else {
            format!("-Copy {}", counter + 1)
        };
        let candidate = parent.join(format!("{}{}{}", stem, suffix, ext));
        if !candidate.exists() {
            break candidate;
        }
        counter += 1;
        if counter > 1000 {
            return Err("Too many copies exist".to_string());
        }
    };

    if src.is_dir() {
        copy_dir_recursive(src, &new_path)?;
    } else {
        fs::copy(&path, &new_path).map_err(|e| e.to_string())?;

        // Also copy sidecar if exists
        let sidecar = format!("{}.sidecar.json", path);
        if Path::new(&sidecar).exists() {
            let _ = fs::copy(&sidecar, format!("{}.sidecar.json", new_path.display()));
        }
    }

    Ok(DuplicateResult {
        new_path: new_path.to_string_lossy().to_string(),
    })
}

/// Move file/folder to OS trash instead of permanent delete
#[tauri::command]
pub async fn file_trash(path: String) -> Result<(), String> {
    let src = Path::new(&path);

    if !src.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    // Also trash sidecar if exists (for files)
    if src.is_file() {
        let sidecar = format!("{}.sidecar.json", path);
        if Path::new(&sidecar).exists() {
            let _ = trash::delete(&sidecar);
        }
    }

    trash::delete(&path).map_err(|e| e.to_string())
}

/// Reveal file/folder in the OS file manager
#[tauri::command]
pub async fn file_reveal(path: String) -> Result<(), String> {
    let src = Path::new(&path);

    if !src.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, open the parent folder since most file managers don't support selecting
        let parent = src.parent().unwrap_or(src);
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Copy multiple files/folders to a destination directory
#[tauri::command]
pub async fn file_copy_to(
    source_paths: Vec<String>,
    dest_dir: String,
) -> Result<BatchOperationResult, String> {
    let dest = Path::new(&dest_dir);

    if !dest.exists() {
        return Err(format!(
            "Destination directory does not exist: {}",
            dest_dir
        ));
    }

    if !dest.is_dir() {
        return Err("Destination must be a directory".to_string());
    }

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for src_path in source_paths {
        let src = Path::new(&src_path);

        if !src.exists() {
            failed.push((src_path.clone(), "Source does not exist".to_string()));
            continue;
        }

        let file_name = match src.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => {
                failed.push((src_path.clone(), "Invalid source path".to_string()));
                continue;
            }
        };

        // Generate unique name if already exists
        let dest_path = generate_unique_path(&dest.join(&file_name));

        let result = if src.is_dir() {
            copy_dir_recursive(src, &dest_path)
        } else {
            fs::copy(&src_path, &dest_path)
                .map(|_| ())
                .map_err(|e| e.to_string())
                .map(|_| {
                    // Copy sidecar if exists
                    let sidecar = format!("{}.sidecar.json", src_path);
                    if Path::new(&sidecar).exists() {
                        let _ = fs::copy(&sidecar, format!("{}.sidecar.json", dest_path.display()));
                    }
                })
        };

        match result {
            Ok(()) => succeeded.push(dest_path.to_string_lossy().to_string()),
            Err(e) => failed.push((src_path, e)),
        }
    }

    Ok(BatchOperationResult { succeeded, failed })
}

/// Move multiple files/folders to a destination directory
#[tauri::command]
pub async fn file_move_to(
    source_paths: Vec<String>,
    dest_dir: String,
) -> Result<BatchOperationResult, String> {
    let dest = Path::new(&dest_dir);

    if !dest.exists() {
        return Err(format!(
            "Destination directory does not exist: {}",
            dest_dir
        ));
    }

    if !dest.is_dir() {
        return Err("Destination must be a directory".to_string());
    }

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for src_path in source_paths {
        let src = Path::new(&src_path);

        if !src.exists() {
            failed.push((src_path.clone(), "Source does not exist".to_string()));
            continue;
        }

        // Prevent moving a folder into itself or its descendants
        if src.is_dir() {
            let src_canonical = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
            let dest_canonical = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
            if dest_canonical.starts_with(&src_canonical) {
                failed.push((src_path, "Cannot move folder into itself".to_string()));
                continue;
            }
        }

        let file_name = match src.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => {
                failed.push((src_path.clone(), "Invalid source path".to_string()));
                continue;
            }
        };

        let dest_path = dest.join(&file_name);

        // If destination already exists, generate unique name
        let final_dest = if dest_path.exists() {
            generate_unique_path(&dest_path)
        } else {
            dest_path
        };

        // Try rename first (same filesystem), fall back to copy+delete
        let result = fs::rename(&src_path, &final_dest)
            .or_else(|_| {
                // Different filesystem - copy then delete
                if src.is_dir() {
                    copy_dir_recursive(src, &final_dest)?;
                    fs::remove_dir_all(src).map_err(|e| e.to_string())
                } else {
                    fs::copy(&src_path, &final_dest).map_err(|e| e.to_string())?;
                    fs::remove_file(src).map_err(|e| e.to_string())
                }
            })
            .map(|_| {
                // Move sidecar if exists
                if src.is_file() || !src.exists() {
                    let sidecar = format!("{}.sidecar.json", src_path);
                    if Path::new(&sidecar).exists() {
                        let dest_sidecar = format!("{}.sidecar.json", final_dest.display());
                        let _ = fs::rename(&sidecar, &dest_sidecar).or_else(|_| {
                            fs::copy(&sidecar, &dest_sidecar)?;
                            fs::remove_file(&sidecar)
                        });
                    }
                }
            });

        match result {
            Ok(()) => succeeded.push(final_dest.to_string_lossy().to_string()),
            Err(e) => failed.push((src_path, e.to_string())),
        }
    }

    Ok(BatchOperationResult { succeeded, failed })
}

// ============== HELPER FUNCTIONS ==============

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Generate a unique path by appending numbers if path already exists
fn generate_unique_path(base: &Path) -> std::path::PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }

    let parent = base.parent().unwrap_or(Path::new("."));
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = base
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1;
    loop {
        let candidate = parent.join(format!("{} ({}){}", stem, counter, ext));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
        if counter > 1000 {
            // Fallback with timestamp
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            return parent.join(format!("{}-{}{}", stem, ts, ext));
        }
    }
}
