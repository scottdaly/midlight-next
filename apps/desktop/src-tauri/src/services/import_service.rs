// Import service for Obsidian and Notion
// Provides vault/export analysis and import with content conversion

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use walkdir::WalkDir;

use super::error::ImportError;
use super::import_security::{
    safe_parse_front_matter, sanitize_csv_cell, sanitize_relative_path, AllowedExtension,
    ImportConfig,
};
use super::import_transaction::ImportTransaction;

/// Type of import source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportSourceType {
    Obsidian,
    Notion,
    Generic,
}

/// Type of file for import
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportFileType {
    Markdown,
    Attachment,
    Other,
}

/// Information about a file to import
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileInfo {
    pub source_path: String,
    pub relative_path: String,
    pub name: String,
    pub file_type: ImportFileType,
    pub size: u64,
    pub has_wiki_links: bool,
    pub has_front_matter: bool,
    pub has_callouts: bool,
    pub has_dataview: bool,
}

/// Warning about file access during analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessWarning {
    pub path: String,
    pub message: String,
}

/// Analysis of an import source
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAnalysis {
    pub source_type: ImportSourceType,
    pub source_path: String,
    pub total_files: usize,
    pub markdown_files: usize,
    pub attachments: usize,
    pub folders: usize,
    pub wiki_links: usize,
    pub files_with_wiki_links: usize,
    pub front_matter: usize,
    pub callouts: usize,
    pub dataview_blocks: usize,
    pub csv_databases: usize,
    pub untitled_pages: Vec<String>,
    pub empty_pages: Vec<String>,
    pub files_to_import: Vec<ImportFileInfo>,
    pub access_warnings: Vec<AccessWarning>,
}

/// Import options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub convert_wiki_links: bool,
    pub import_front_matter: bool,
    pub convert_callouts: bool,
    pub copy_attachments: bool,
    pub preserve_folder_structure: bool,
    pub skip_empty_pages: bool,
    pub create_midlight_files: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            convert_wiki_links: true,
            import_front_matter: true,
            convert_callouts: true,
            copy_attachments: true,
            preserve_folder_structure: true,
            skip_empty_pages: true,
            create_midlight_files: true,
        }
    }
}

/// How to handle untitled pages from Notion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UntitledHandling {
    Number,
    Keep,
    Prompt,
}

/// Notion-specific import options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionImportOptions {
    #[serde(flatten)]
    pub base: ImportOptions,
    pub remove_uuids: bool,
    pub convert_csv_to_tables: bool,
    pub untitled_handling: UntitledHandling,
}

impl Default for NotionImportOptions {
    fn default() -> Self {
        Self {
            base: ImportOptions::default(),
            remove_uuids: true,
            convert_csv_to_tables: true,
            untitled_handling: UntitledHandling::Number,
        }
    }
}

/// Phase of the import process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportPhase {
    Analyzing,
    Converting,
    Copying,
    Finalizing,
    Complete,
}

/// Import error details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportErrorInfo {
    pub file: String,
    pub message: String,
}

/// Import warning details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWarningInfo {
    pub file: String,
    pub message: String,
}

/// Progress update during import
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub phase: ImportPhase,
    pub current: usize,
    pub total: usize,
    pub current_file: String,
    pub errors: Vec<ImportErrorInfo>,
    pub warnings: Vec<ImportWarningInfo>,
}

/// Result of an import operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub success: bool,
    pub files_imported: usize,
    pub links_converted: usize,
    pub attachments_copied: usize,
    pub errors: Vec<ImportErrorInfo>,
    pub warnings: Vec<ImportWarningInfo>,
}

/// Broken link found during import
#[derive(Debug, Clone)]
pub struct BrokenLink {
    pub original: String,
    pub file: String,
}

// ============================================================================
// Source Detection
// ============================================================================

/// Detect the type of import source
pub fn detect_source_type(folder_path: &Path) -> Result<ImportSourceType, ImportError> {
    if !folder_path.exists() {
        return Err(ImportError::FileNotFound(format!(
            "Folder not found: {:?}",
            folder_path
        )));
    }

    if !folder_path.is_dir() {
        return Err(ImportError::InvalidPath("Path is not a directory".into()));
    }

    // Check for .obsidian folder (Obsidian vault)
    if folder_path.join(".obsidian").exists() {
        return Ok(ImportSourceType::Obsidian);
    }

    // Check for UUID-suffixed files (Notion export)
    // Notion exports have filenames like "Page Title abc123def456.md"
    let uuid_pattern = Regex::new(r" [0-9a-f]{32}\.").expect("Invalid UUID regex");

    for entry in WalkDir::new(folder_path).max_depth(2) {
        if let Ok(entry) = entry {
            if let Some(name) = entry.file_name().to_str() {
                if uuid_pattern.is_match(name) {
                    return Ok(ImportSourceType::Notion);
                }
            }
        }
    }

    Ok(ImportSourceType::Generic)
}

// ============================================================================
// Obsidian Analysis
// ============================================================================

/// Analyze an Obsidian vault
pub fn analyze_obsidian_vault(vault_path: &Path) -> Result<ImportAnalysis, ImportError> {
    if !vault_path.exists() {
        return Err(ImportError::FileNotFound(format!(
            "Vault not found: {:?}",
            vault_path
        )));
    }

    let wiki_link_pattern = Regex::new(r"\[\[([^\]]+)\]\]").expect("Invalid wiki link regex");
    let callout_pattern = Regex::new(r"(?m)^>\s*\[!(\w+)\]").expect("Invalid callout regex");
    let dataview_pattern =
        Regex::new(r"```(?:dataview|dataviewjs)[\s\S]*?```").expect("Invalid dataview regex");

    let mut analysis = ImportAnalysis {
        source_type: ImportSourceType::Obsidian,
        source_path: vault_path.to_string_lossy().to_string(),
        total_files: 0,
        markdown_files: 0,
        attachments: 0,
        folders: 0,
        wiki_links: 0,
        files_with_wiki_links: 0,
        front_matter: 0,
        callouts: 0,
        dataview_blocks: 0,
        csv_databases: 0,
        untitled_pages: Vec::new(),
        empty_pages: Vec::new(),
        files_to_import: Vec::new(),
        access_warnings: Vec::new(),
    };

    let mut folder_set = std::collections::HashSet::new();

    for entry in WalkDir::new(vault_path) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                analysis.access_warnings.push(AccessWarning {
                    path: err
                        .path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    message: err.to_string(),
                });
                continue;
            }
        };

        let path = entry.path();

        // Skip .obsidian and hidden folders
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }

        if entry.file_type().is_dir() {
            let rel_path = path.strip_prefix(vault_path).unwrap_or(path);
            if !rel_path.as_os_str().is_empty() {
                folder_set.insert(rel_path.to_path_buf());
            }
            continue;
        }

        let relative_path = match path.strip_prefix(vault_path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        let file_name = entry.file_name().to_string_lossy().to_string();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(err) => {
                analysis.access_warnings.push(AccessWarning {
                    path: relative_path,
                    message: err.to_string(),
                });
                continue;
            }
        };

        let size = metadata.len();
        analysis.total_files += 1;

        // Determine file type
        let file_type = if AllowedExtension::Markdown.matches(&file_name) {
            ImportFileType::Markdown
        } else if AllowedExtension::Image.matches(&file_name)
            || AllowedExtension::Attachment.matches(&file_name)
        {
            ImportFileType::Attachment
        } else {
            ImportFileType::Other
        };

        let mut file_info = ImportFileInfo {
            source_path: path.to_string_lossy().to_string(),
            relative_path: relative_path.clone(),
            name: file_name.clone(),
            file_type,
            size,
            has_wiki_links: false,
            has_front_matter: false,
            has_callouts: false,
            has_dataview: false,
        };

        match file_type {
            ImportFileType::Markdown => {
                analysis.markdown_files += 1;

                // Read and analyze content
                if size < ImportConfig::MAX_CONTENT_SIZE as u64 {
                    match fs::read_to_string(path) {
                        Ok(content) => {
                            // Check for empty content
                            if content.trim().is_empty() {
                                analysis.empty_pages.push(relative_path.clone());
                            }

                            // Check for untitled files
                            let stem = Path::new(&file_name)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("");
                            if stem.to_lowercase() == "untitled" {
                                analysis.untitled_pages.push(relative_path.clone());
                            }

                            // Count wiki links
                            let wiki_link_count = wiki_link_pattern.find_iter(&content).count();
                            if wiki_link_count > 0 {
                                analysis.wiki_links += wiki_link_count;
                                analysis.files_with_wiki_links += 1;
                                file_info.has_wiki_links = true;
                            }

                            // Check for front matter
                            if safe_parse_front_matter(&content).ok().flatten().is_some() {
                                analysis.front_matter += 1;
                                file_info.has_front_matter = true;
                            }

                            // Count callouts
                            let callout_count = callout_pattern.find_iter(&content).count();
                            if callout_count > 0 {
                                analysis.callouts += callout_count;
                                file_info.has_callouts = true;
                            }

                            // Count dataview blocks
                            let dataview_count = dataview_pattern.find_iter(&content).count();
                            if dataview_count > 0 {
                                analysis.dataview_blocks += dataview_count;
                                file_info.has_dataview = true;
                            }
                        }
                        Err(err) => {
                            analysis.access_warnings.push(AccessWarning {
                                path: relative_path.clone(),
                                message: format!("Could not read file: {}", err),
                            });
                        }
                    }
                }
            }
            ImportFileType::Attachment => {
                analysis.attachments += 1;
            }
            ImportFileType::Other => {}
        }

        analysis.files_to_import.push(file_info);
    }

    analysis.folders = folder_set.len();

    Ok(analysis)
}

// ============================================================================
// Notion Analysis
// ============================================================================

/// Analyze a Notion export
pub fn analyze_notion_export(export_path: &Path) -> Result<ImportAnalysis, ImportError> {
    if !export_path.exists() {
        return Err(ImportError::FileNotFound(format!(
            "Export not found: {:?}",
            export_path
        )));
    }

    let _uuid_pattern = Regex::new(r" [0-9a-f]{32}(\.|\/)").expect("Invalid UUID regex");

    let mut analysis = ImportAnalysis {
        source_type: ImportSourceType::Notion,
        source_path: export_path.to_string_lossy().to_string(),
        total_files: 0,
        markdown_files: 0,
        attachments: 0,
        folders: 0,
        wiki_links: 0,
        files_with_wiki_links: 0,
        front_matter: 0,
        callouts: 0,
        dataview_blocks: 0,
        csv_databases: 0,
        untitled_pages: Vec::new(),
        empty_pages: Vec::new(),
        files_to_import: Vec::new(),
        access_warnings: Vec::new(),
    };

    let mut folder_set = std::collections::HashSet::new();

    for entry in WalkDir::new(export_path) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                analysis.access_warnings.push(AccessWarning {
                    path: err
                        .path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    message: err.to_string(),
                });
                continue;
            }
        };

        let path = entry.path();

        if entry.file_type().is_dir() {
            let rel_path = path.strip_prefix(export_path).unwrap_or(path);
            if !rel_path.as_os_str().is_empty() {
                folder_set.insert(rel_path.to_path_buf());
            }
            continue;
        }

        let relative_path = match path.strip_prefix(export_path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        let file_name = entry.file_name().to_string_lossy().to_string();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(err) => {
                analysis.access_warnings.push(AccessWarning {
                    path: relative_path,
                    message: err.to_string(),
                });
                continue;
            }
        };

        let size = metadata.len();
        analysis.total_files += 1;

        // Check for CSV databases
        if file_name.to_lowercase().ends_with(".csv") {
            analysis.csv_databases += 1;
        }

        // Determine file type
        let file_type = if AllowedExtension::Markdown.matches(&file_name) {
            ImportFileType::Markdown
        } else if AllowedExtension::Image.matches(&file_name)
            || AllowedExtension::Attachment.matches(&file_name)
        {
            ImportFileType::Attachment
        } else if AllowedExtension::Data.matches(&file_name) {
            ImportFileType::Other // CSV files handled separately
        } else {
            ImportFileType::Other
        };

        let file_info = ImportFileInfo {
            source_path: path.to_string_lossy().to_string(),
            relative_path: relative_path.clone(),
            name: file_name.clone(),
            file_type,
            size,
            has_wiki_links: false, // Notion doesn't use wiki links
            has_front_matter: false,
            has_callouts: false,
            has_dataview: false,
        };

        match file_type {
            ImportFileType::Markdown => {
                analysis.markdown_files += 1;

                // Check for untitled
                let name_without_uuid = strip_notion_uuid(&file_name);
                let stem = Path::new(&name_without_uuid)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if stem.to_lowercase() == "untitled" {
                    analysis.untitled_pages.push(relative_path.clone());
                }

                // Check for empty
                if size == 0 {
                    analysis.empty_pages.push(relative_path.clone());
                }
            }
            ImportFileType::Attachment => {
                analysis.attachments += 1;
            }
            ImportFileType::Other => {}
        }

        analysis.files_to_import.push(file_info);
    }

    analysis.folders = folder_set.len();

    Ok(analysis)
}

// ============================================================================
// Content Conversion
// ============================================================================

/// Build a file map for wiki-link resolution
pub fn build_file_map(files: &[ImportFileInfo]) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for file in files {
        if file.file_type != ImportFileType::Markdown {
            continue;
        }

        // Add exact name (without extension)
        let stem = Path::new(&file.name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&file.name);
        map.insert(stem.to_lowercase(), file.relative_path.clone());

        // Add name with extension
        map.insert(file.name.to_lowercase(), file.relative_path.clone());
    }

    map
}

/// Convert wiki links to standard markdown links
///
/// Returns (converted_content, conversion_count, broken_links)
pub fn convert_wiki_links(
    content: &str,
    file_map: &HashMap<String, String>,
    current_file: &str,
) -> (String, usize, Vec<BrokenLink>) {
    let wiki_link_pattern =
        Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").expect("Invalid wiki link regex");

    let mut result = content.to_string();
    let mut conversion_count = 0;
    let mut broken_links = Vec::new();

    // Find all matches first to avoid borrow issues
    let matches: Vec<_> = wiki_link_pattern
        .captures_iter(content)
        .map(|cap| {
            let full_match = cap.get(0).unwrap();
            let link_target = cap.get(1).unwrap().as_str();
            let display_text = cap.get(2).map(|m| m.as_str());
            (
                full_match.start(),
                full_match.end(),
                link_target.to_string(),
                display_text.map(|s| s.to_string()),
            )
        })
        .collect();

    // Process in reverse order to maintain positions
    for (start, end, link_target, display_text) in matches.into_iter().rev() {
        // Parse link target (might have heading anchor)
        let (file_part, anchor) = if let Some(hash_pos) = link_target.find('#') {
            (&link_target[..hash_pos], Some(&link_target[hash_pos..]))
        } else {
            (link_target.as_str(), None)
        };

        // Try to resolve the link
        let resolved = file_map
            .get(&file_part.to_lowercase())
            .or_else(|| file_map.get(&format!("{}.md", file_part.to_lowercase())));

        let replacement = if let Some(target_path) = resolved {
            let display = display_text.as_deref().unwrap_or(file_part);
            let link = if let Some(anchor) = anchor {
                format!("{}{}", target_path, anchor)
            } else {
                target_path.clone()
            };
            format!("[{}]({})", display, link)
        } else {
            // Broken link - keep display text but mark as broken
            broken_links.push(BrokenLink {
                original: link_target.clone(),
                file: current_file.to_string(),
            });
            let display = display_text.as_deref().unwrap_or(file_part);
            display.to_string()
        };

        result.replace_range(start..end, &replacement);
        conversion_count += 1;
    }

    (result, conversion_count, broken_links)
}

/// Convert Obsidian callouts to blockquotes
pub fn convert_callouts(content: &str) -> String {
    let callout_pattern =
        Regex::new(r"(?m)^>\s*\[!(\w+)\](?:\s*(.*))?$").expect("Invalid callout regex");

    let mut result = content.to_string();
    let mut offset: i64 = 0;

    for cap in callout_pattern.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let callout_type = cap.get(1).unwrap().as_str();
        let title = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");

        // Convert callout type to styled blockquote
        let replacement = if title.is_empty() {
            format!("> **{}**", callout_type.to_uppercase())
        } else {
            format!("> **{}:** {}", callout_type.to_uppercase(), title)
        };

        let start = (full_match.start() as i64 + offset) as usize;
        let end = (full_match.end() as i64 + offset) as usize;
        let old_len = end - start;
        let new_len = replacement.len();

        result.replace_range(start..end, &replacement);
        offset += new_len as i64 - old_len as i64;
    }

    result
}

/// Remove dataview blocks from content
pub fn remove_dataview(content: &str) -> String {
    let dataview_block_pattern =
        Regex::new(r"```(?:dataview|dataviewjs)[\s\S]*?```").expect("Invalid dataview regex");

    let result = dataview_block_pattern.replace_all(content, "");

    // Also remove inline dataview
    let inline_pattern = Regex::new(r"`=.*?`").expect("Invalid inline dataview regex");
    inline_pattern.replace_all(&result, "").to_string()
}

/// Strip Notion UUID from filename
pub fn strip_notion_uuid(filename: &str) -> String {
    let uuid_pattern = Regex::new(r" [0-9a-f]{32}(\.[^.]+)$").expect("Invalid UUID regex");

    if let Some(caps) = uuid_pattern.captures(filename) {
        let ext = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let without_uuid = &filename[..caps.get(0).unwrap().start()];
        format!("{}{}", without_uuid, ext)
    } else {
        filename.to_string()
    }
}

/// Convert CSV content to a Markdown table
pub fn csv_to_markdown_table(csv_content: &str) -> Result<String, ImportError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_content.as_bytes());

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| ImportError::CsvParse(e.to_string()))?
        .iter()
        .map(|h| sanitize_csv_cell(h))
        .collect();

    if headers.is_empty() {
        return Ok(String::new());
    }

    let mut table = String::new();

    // Header row
    table.push_str("| ");
    table.push_str(&headers.join(" | "));
    table.push_str(" |\n");

    // Separator row
    table.push_str("|");
    for _ in &headers {
        table.push_str(" --- |");
    }
    table.push('\n');

    // Data rows
    for result in reader.records() {
        let record = result.map_err(|e| ImportError::CsvParse(e.to_string()))?;
        table.push_str("| ");
        let cells: Vec<String> = record.iter().map(|c| sanitize_csv_cell(c)).collect();
        table.push_str(&cells.join(" | "));
        table.push_str(" |\n");
    }

    Ok(table)
}

// ============================================================================
// Import Execution
// ============================================================================

/// Cancellation token for import operations
pub struct CancellationToken {
    cancelled: AtomicBool,
}

impl CancellationToken {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
        })
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
        }
    }
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(ImportProgress) + Send + Sync>;

/// Import an Obsidian vault
pub fn import_obsidian_vault(
    analysis: &ImportAnalysis,
    dest_path: &Path,
    options: &ImportOptions,
    progress_callback: Option<ProgressCallback>,
    cancel_token: Option<Arc<CancellationToken>>,
) -> Result<ImportResult, ImportError> {
    let _source_path = PathBuf::from(&analysis.source_path);
    let mut transaction = ImportTransaction::new(dest_path.to_path_buf())?;

    let file_map = build_file_map(&analysis.files_to_import);
    let total_files = analysis.files_to_import.len();

    let mut files_imported = 0;
    let mut links_converted = 0;
    let mut attachments_copied = 0;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let mut last_progress_time = Instant::now();

    let send_progress = |phase: ImportPhase,
                         current: usize,
                         current_file: &str,
                         errors: &[ImportErrorInfo],
                         warnings: &[ImportWarningInfo]| {
        if let Some(ref callback) = progress_callback {
            callback(ImportProgress {
                phase,
                current,
                total: total_files,
                current_file: current_file.to_string(),
                errors: errors.to_vec(),
                warnings: warnings.to_vec(),
            });
        }
    };

    // Phase 1: Converting markdown files
    send_progress(ImportPhase::Converting, 0, "", &errors, &warnings);

    for (idx, file_info) in analysis.files_to_import.iter().enumerate() {
        // Check for cancellation
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                transaction.rollback()?;
                return Err(ImportError::Cancelled);
            }
        }

        // Throttle progress updates
        if last_progress_time.elapsed().as_millis() >= ImportConfig::PROGRESS_THROTTLE_MS as u128 {
            send_progress(
                ImportPhase::Converting,
                idx,
                &file_info.name,
                &errors,
                &warnings,
            );
            last_progress_time = Instant::now();
        }

        // Determine destination path
        let dest_relative = if options.preserve_folder_structure {
            file_info.relative_path.clone()
        } else {
            file_info.name.clone()
        };

        let dest_relative_path = match sanitize_relative_path(&dest_relative) {
            Ok(p) => p,
            Err(e) => {
                errors.push(ImportErrorInfo {
                    file: file_info.relative_path.clone(),
                    message: e.to_string(),
                });
                continue;
            }
        };

        match file_info.file_type {
            ImportFileType::Markdown => {
                // Skip empty pages if option set
                if options.skip_empty_pages
                    && analysis.empty_pages.contains(&file_info.relative_path)
                {
                    continue;
                }

                // Read source file
                let content = match fs::read_to_string(&file_info.source_path) {
                    Ok(c) => c,
                    Err(e) => {
                        errors.push(ImportErrorInfo {
                            file: file_info.relative_path.clone(),
                            message: format!("Could not read file: {}", e),
                        });
                        continue;
                    }
                };

                let mut converted = content;

                // Convert wiki links
                if options.convert_wiki_links && file_info.has_wiki_links {
                    let (new_content, count, broken) =
                        convert_wiki_links(&converted, &file_map, &file_info.relative_path);
                    converted = new_content;
                    links_converted += count;

                    for link in broken {
                        warnings.push(ImportWarningInfo {
                            file: link.file,
                            message: format!("Broken link: {}", link.original),
                        });
                    }
                }

                // Convert callouts
                if options.convert_callouts && file_info.has_callouts {
                    converted = convert_callouts(&converted);
                }

                // Remove dataview
                if file_info.has_dataview {
                    converted = remove_dataview(&converted);
                }

                // Stage the file
                if let Err(e) = transaction.stage_file(&dest_relative_path, converted.as_bytes()) {
                    errors.push(ImportErrorInfo {
                        file: file_info.relative_path.clone(),
                        message: e.to_string(),
                    });
                    continue;
                }

                files_imported += 1;
            }
            ImportFileType::Attachment => {
                if !options.copy_attachments {
                    continue;
                }

                // Copy attachment
                if let Err(e) =
                    transaction.stage_copy(Path::new(&file_info.source_path), &dest_relative_path)
                {
                    errors.push(ImportErrorInfo {
                        file: file_info.relative_path.clone(),
                        message: e.to_string(),
                    });
                    continue;
                }

                attachments_copied += 1;
            }
            ImportFileType::Other => {
                // Skip other file types
            }
        }
    }

    // Check for cancellation before commit
    if let Some(ref token) = cancel_token {
        if token.is_cancelled() {
            transaction.rollback()?;
            return Err(ImportError::Cancelled);
        }
    }

    // Phase 2: Finalizing
    send_progress(
        ImportPhase::Finalizing,
        total_files,
        "Committing changes...",
        &errors,
        &warnings,
    );

    // Commit the transaction
    transaction.commit()?;

    // Phase 3: Complete
    send_progress(ImportPhase::Complete, total_files, "", &errors, &warnings);

    Ok(ImportResult {
        success: errors.is_empty(),
        files_imported,
        links_converted,
        attachments_copied,
        errors,
        warnings,
    })
}

/// Import a Notion export
pub fn import_notion_export(
    analysis: &ImportAnalysis,
    dest_path: &Path,
    options: &NotionImportOptions,
    progress_callback: Option<ProgressCallback>,
    cancel_token: Option<Arc<CancellationToken>>,
) -> Result<ImportResult, ImportError> {
    let mut transaction = ImportTransaction::new(dest_path.to_path_buf())?;

    let total_files = analysis.files_to_import.len();

    let mut files_imported = 0;
    let mut links_converted = 0;
    let mut attachments_copied = 0;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Build filename map for link updates (UUID -> clean name)
    let mut filename_map: HashMap<String, String> = HashMap::new();
    if options.remove_uuids {
        for file_info in &analysis.files_to_import {
            let clean_name = strip_notion_uuid(&file_info.name);
            if clean_name != file_info.name {
                filename_map.insert(file_info.name.clone(), clean_name);
            }
        }
    }

    let mut last_progress_time = Instant::now();

    let send_progress = |phase: ImportPhase,
                         current: usize,
                         current_file: &str,
                         errors: &[ImportErrorInfo],
                         warnings: &[ImportWarningInfo]| {
        if let Some(ref callback) = progress_callback {
            callback(ImportProgress {
                phase,
                current,
                total: total_files,
                current_file: current_file.to_string(),
                errors: errors.to_vec(),
                warnings: warnings.to_vec(),
            });
        }
    };

    send_progress(ImportPhase::Converting, 0, "", &errors, &warnings);

    for (idx, file_info) in analysis.files_to_import.iter().enumerate() {
        // Check for cancellation
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                transaction.rollback()?;
                return Err(ImportError::Cancelled);
            }
        }

        // Throttle progress updates
        if last_progress_time.elapsed().as_millis() >= ImportConfig::PROGRESS_THROTTLE_MS as u128 {
            send_progress(
                ImportPhase::Converting,
                idx,
                &file_info.name,
                &errors,
                &warnings,
            );
            last_progress_time = Instant::now();
        }

        // Determine destination path
        let dest_name = if options.remove_uuids {
            filename_map
                .get(&file_info.name)
                .cloned()
                .unwrap_or_else(|| file_info.name.clone())
        } else {
            file_info.name.clone()
        };

        let dest_relative = if options.base.preserve_folder_structure {
            // Replace filename in relative path
            let parent = Path::new(&file_info.relative_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            if parent.is_empty() {
                dest_name.clone()
            } else {
                format!("{}/{}", parent, dest_name)
            }
        } else {
            dest_name.clone()
        };

        let dest_relative_path = match sanitize_relative_path(&dest_relative) {
            Ok(p) => p,
            Err(e) => {
                errors.push(ImportErrorInfo {
                    file: file_info.relative_path.clone(),
                    message: e.to_string(),
                });
                continue;
            }
        };

        match file_info.file_type {
            ImportFileType::Markdown => {
                // Skip empty pages if option set
                if options.base.skip_empty_pages && file_info.size == 0 {
                    continue;
                }

                // Read source file
                let content = match fs::read_to_string(&file_info.source_path) {
                    Ok(c) => c,
                    Err(e) => {
                        errors.push(ImportErrorInfo {
                            file: file_info.relative_path.clone(),
                            message: format!("Could not read file: {}", e),
                        });
                        continue;
                    }
                };

                let mut converted = content;

                // Update links if UUIDs are being removed
                if options.remove_uuids && !filename_map.is_empty() {
                    for (old_name, new_name) in &filename_map {
                        // Replace in markdown links
                        let old_escaped = regex::escape(old_name);
                        let link_pattern = format!(r"\]\({}\)", old_escaped);
                        if let Ok(re) = Regex::new(&link_pattern) {
                            converted = re
                                .replace_all(&converted, format!("]({})", new_name))
                                .to_string();
                            links_converted += 1;
                        }
                    }
                }

                // Stage the file
                if let Err(e) = transaction.stage_file(&dest_relative_path, converted.as_bytes()) {
                    errors.push(ImportErrorInfo {
                        file: file_info.relative_path.clone(),
                        message: e.to_string(),
                    });
                    continue;
                }

                files_imported += 1;
            }
            ImportFileType::Attachment => {
                if !options.base.copy_attachments {
                    continue;
                }

                if let Err(e) =
                    transaction.stage_copy(Path::new(&file_info.source_path), &dest_relative_path)
                {
                    errors.push(ImportErrorInfo {
                        file: file_info.relative_path.clone(),
                        message: e.to_string(),
                    });
                    continue;
                }

                attachments_copied += 1;
            }
            ImportFileType::Other => {
                // Handle CSV files
                if options.convert_csv_to_tables && file_info.name.to_lowercase().ends_with(".csv")
                {
                    let content = match fs::read_to_string(&file_info.source_path) {
                        Ok(c) => c,
                        Err(e) => {
                            errors.push(ImportErrorInfo {
                                file: file_info.relative_path.clone(),
                                message: format!("Could not read CSV: {}", e),
                            });
                            continue;
                        }
                    };

                    match csv_to_markdown_table(&content) {
                        Ok(table) => {
                            // Create markdown file from CSV
                            let md_name = dest_name.replace(".csv", ".md").replace(".CSV", ".md");
                            let md_path = if options.base.preserve_folder_structure {
                                let parent = Path::new(&file_info.relative_path)
                                    .parent()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                if parent.is_empty() {
                                    md_name
                                } else {
                                    format!("{}/{}", parent, md_name)
                                }
                            } else {
                                md_name
                            };

                            if let Ok(safe_path) = sanitize_relative_path(&md_path) {
                                if let Err(e) = transaction.stage_file(&safe_path, table.as_bytes())
                                {
                                    errors.push(ImportErrorInfo {
                                        file: file_info.relative_path.clone(),
                                        message: e.to_string(),
                                    });
                                }
                                files_imported += 1;
                            }
                        }
                        Err(e) => {
                            warnings.push(ImportWarningInfo {
                                file: file_info.relative_path.clone(),
                                message: format!("Could not convert CSV: {}", e),
                            });
                        }
                    }
                }
            }
        }
    }

    // Check for cancellation before commit
    if let Some(ref token) = cancel_token {
        if token.is_cancelled() {
            transaction.rollback()?;
            return Err(ImportError::Cancelled);
        }
    }

    // Commit
    send_progress(
        ImportPhase::Finalizing,
        total_files,
        "Committing changes...",
        &errors,
        &warnings,
    );

    transaction.commit()?;

    send_progress(ImportPhase::Complete, total_files, "", &errors, &warnings);

    Ok(ImportResult {
        success: errors.is_empty(),
        files_imported,
        links_converted,
        attachments_copied,
        errors,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_source_type() {
        // Would need temp directories with actual structures to test properly
    }

    #[test]
    fn test_strip_notion_uuid() {
        assert_eq!(
            strip_notion_uuid("Page Title abc123def456789012345678901234.md"),
            "Page Title.md"
        );
        assert_eq!(strip_notion_uuid("No UUID.md"), "No UUID.md");
        assert_eq!(
            strip_notion_uuid("Test 12345678901234567890123456789012.txt"),
            "Test.txt"
        );
    }

    #[test]
    fn test_convert_wiki_links() {
        let mut file_map = HashMap::new();
        file_map.insert("other note".to_string(), "other note.md".to_string());
        file_map.insert("target".to_string(), "folder/target.md".to_string());

        let content = "Link to [[Other Note]] and [[Target|custom text]].";
        let (converted, count, broken) = convert_wiki_links(content, &file_map, "test.md");

        assert_eq!(count, 2);
        assert!(converted.contains("[Other Note](other note.md)"));
        assert!(converted.contains("[custom text](folder/target.md)"));
        assert!(broken.is_empty());
    }

    #[test]
    fn test_convert_wiki_links_with_broken() {
        let file_map = HashMap::new();

        let content = "Link to [[Missing Page]].";
        let (converted, count, broken) = convert_wiki_links(content, &file_map, "test.md");

        assert_eq!(count, 1);
        assert_eq!(converted, "Link to Missing Page.");
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].original, "Missing Page");
    }

    #[test]
    fn test_convert_callouts() {
        let content = "> [!note] Important info\n> Content here";
        let converted = convert_callouts(content);
        assert!(converted.contains("> **NOTE:** Important info"));
    }

    #[test]
    fn test_remove_dataview() {
        let content = "# Title\n\n```dataview\nTABLE file.name\n```\n\nMore content";
        let result = remove_dataview(content);
        assert!(!result.contains("dataview"));
        assert!(result.contains("# Title"));
        assert!(result.contains("More content"));
    }

    #[test]
    fn test_csv_to_markdown_table() {
        let csv = "Name,Age,City\nAlice,30,NYC\nBob,25,LA";
        let table = csv_to_markdown_table(csv).unwrap();

        assert!(table.contains("| Name | Age | City |"));
        assert!(table.contains("| --- | --- | --- |"));
        assert!(table.contains("| Alice | 30 | NYC |"));
        assert!(table.contains("| Bob | 25 | LA |"));
    }
}
