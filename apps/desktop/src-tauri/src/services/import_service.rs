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

    for entry in WalkDir::new(folder_path).max_depth(2).into_iter().flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if uuid_pattern.is_match(name) {
                return Ok(ImportSourceType::Notion);
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

        // Get relative path first
        let rel_path = match path.strip_prefix(vault_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Skip .obsidian and hidden folders/files (check relative path, not full path)
        if rel_path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }

        if entry.file_type().is_dir() {
            if !rel_path.as_os_str().is_empty() {
                folder_set.insert(rel_path.to_path_buf());
            }
            continue;
        }

        let relative_path = rel_path.to_string_lossy().to_string();

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
        } else {
            // CSV and other data files handled separately
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
    // Use [ \t]* instead of \s* to only match horizontal whitespace, not newlines
    let callout_pattern =
        Regex::new(r"(?m)^>\s*\[!(\w+)\](?:[ \t]*(.*))?$").expect("Invalid callout regex");

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
        .map(sanitize_csv_cell)
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
    table.push('|');
    for _ in &headers {
        table.push_str(" --- |");
    }
    table.push('\n');

    // Data rows
    for result in reader.records() {
        let record = result.map_err(|e| ImportError::CsvParse(e.to_string()))?;
        table.push_str("| ");
        let cells: Vec<String> = record.iter().map(sanitize_csv_cell).collect();
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
    use tempfile::TempDir;

    // ============================================================================
    // Enum Serialization Tests
    // ============================================================================

    #[test]
    fn test_import_source_type_serialization() {
        assert_eq!(
            serde_json::to_string(&ImportSourceType::Obsidian).unwrap(),
            "\"obsidian\""
        );
        assert_eq!(
            serde_json::to_string(&ImportSourceType::Notion).unwrap(),
            "\"notion\""
        );
        assert_eq!(
            serde_json::to_string(&ImportSourceType::Generic).unwrap(),
            "\"generic\""
        );
    }

    #[test]
    fn test_import_file_type_serialization() {
        assert_eq!(
            serde_json::to_string(&ImportFileType::Markdown).unwrap(),
            "\"markdown\""
        );
        assert_eq!(
            serde_json::to_string(&ImportFileType::Attachment).unwrap(),
            "\"attachment\""
        );
        assert_eq!(
            serde_json::to_string(&ImportFileType::Other).unwrap(),
            "\"other\""
        );
    }

    #[test]
    fn test_import_phase_serialization() {
        assert_eq!(
            serde_json::to_string(&ImportPhase::Analyzing).unwrap(),
            "\"analyzing\""
        );
        assert_eq!(
            serde_json::to_string(&ImportPhase::Converting).unwrap(),
            "\"converting\""
        );
        assert_eq!(
            serde_json::to_string(&ImportPhase::Copying).unwrap(),
            "\"copying\""
        );
        assert_eq!(
            serde_json::to_string(&ImportPhase::Finalizing).unwrap(),
            "\"finalizing\""
        );
        assert_eq!(
            serde_json::to_string(&ImportPhase::Complete).unwrap(),
            "\"complete\""
        );
    }

    #[test]
    fn test_untitled_handling_serialization() {
        assert_eq!(
            serde_json::to_string(&UntitledHandling::Number).unwrap(),
            "\"number\""
        );
        assert_eq!(
            serde_json::to_string(&UntitledHandling::Keep).unwrap(),
            "\"keep\""
        );
        assert_eq!(
            serde_json::to_string(&UntitledHandling::Prompt).unwrap(),
            "\"prompt\""
        );
    }

    // ============================================================================
    // Default Options Tests
    // ============================================================================

    #[test]
    fn test_import_options_default() {
        let options = ImportOptions::default();
        assert!(options.convert_wiki_links);
        assert!(options.import_front_matter);
        assert!(options.convert_callouts);
        assert!(options.copy_attachments);
        assert!(options.preserve_folder_structure);
        assert!(options.skip_empty_pages);
        assert!(options.create_midlight_files);
    }

    #[test]
    fn test_notion_import_options_default() {
        let options = NotionImportOptions::default();
        assert!(options.remove_uuids);
        assert!(options.convert_csv_to_tables);
        assert_eq!(options.untitled_handling, UntitledHandling::Number);
        // Check base options
        assert!(options.base.convert_wiki_links);
        assert!(options.base.copy_attachments);
    }

    // ============================================================================
    // Struct Serialization Tests
    // ============================================================================

    #[test]
    fn test_import_file_info_serialization() {
        let info = ImportFileInfo {
            source_path: "/path/to/file.md".to_string(),
            relative_path: "file.md".to_string(),
            name: "file.md".to_string(),
            file_type: ImportFileType::Markdown,
            size: 1024,
            has_wiki_links: true,
            has_front_matter: false,
            has_callouts: true,
            has_dataview: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"sourcePath\""));
        assert!(json.contains("\"relativePath\""));
        assert!(json.contains("\"hasWikiLinks\":true"));
    }

    #[test]
    fn test_access_warning_serialization() {
        let warning = AccessWarning {
            path: "/path/to/file".to_string(),
            message: "Permission denied".to_string(),
        };
        let json = serde_json::to_string(&warning).unwrap();
        assert!(json.contains("Permission denied"));
    }

    #[test]
    fn test_import_error_info_serialization() {
        let error = ImportErrorInfo {
            file: "test.md".to_string(),
            message: "Could not read file".to_string(),
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"file\":\"test.md\""));
        assert!(json.contains("Could not read file"));
    }

    #[test]
    fn test_import_warning_info_serialization() {
        let warning = ImportWarningInfo {
            file: "test.md".to_string(),
            message: "Broken link detected".to_string(),
        };
        let json = serde_json::to_string(&warning).unwrap();
        assert!(json.contains("Broken link"));
    }

    #[test]
    fn test_import_progress_serialization() {
        let progress = ImportProgress {
            phase: ImportPhase::Converting,
            current: 5,
            total: 10,
            current_file: "test.md".to_string(),
            errors: vec![],
            warnings: vec![],
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"phase\":\"converting\""));
        assert!(json.contains("\"current\":5"));
        assert!(json.contains("\"total\":10"));
        assert!(json.contains("\"currentFile\":\"test.md\""));
    }

    #[test]
    fn test_import_result_serialization() {
        let result = ImportResult {
            success: true,
            files_imported: 10,
            links_converted: 5,
            attachments_copied: 3,
            errors: vec![],
            warnings: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"filesImported\":10"));
        assert!(json.contains("\"linksConverted\":5"));
        assert!(json.contains("\"attachmentsCopied\":3"));
    }

    #[test]
    fn test_import_analysis_serialization() {
        let analysis = ImportAnalysis {
            source_type: ImportSourceType::Obsidian,
            source_path: "/vault".to_string(),
            total_files: 100,
            markdown_files: 80,
            attachments: 20,
            folders: 10,
            wiki_links: 50,
            files_with_wiki_links: 30,
            front_matter: 25,
            callouts: 5,
            dataview_blocks: 2,
            csv_databases: 0,
            untitled_pages: vec![],
            empty_pages: vec![],
            files_to_import: vec![],
            access_warnings: vec![],
        };
        let json = serde_json::to_string(&analysis).unwrap();
        assert!(json.contains("\"sourceType\":\"obsidian\""));
        assert!(json.contains("\"totalFiles\":100"));
        assert!(json.contains("\"markdownFiles\":80"));
    }

    // ============================================================================
    // CancellationToken Tests
    // ============================================================================

    #[test]
    fn test_cancellation_token_new() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_cancel() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_default() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_thread_safe() {
        use std::thread;

        let token = CancellationToken::new();
        let token_clone = Arc::clone(&token);

        let handle = thread::spawn(move || {
            token_clone.cancel();
        });

        handle.join().unwrap();
        assert!(token.is_cancelled());
    }

    // ============================================================================
    // build_file_map Tests
    // ============================================================================

    #[test]
    fn test_build_file_map_empty() {
        let files: Vec<ImportFileInfo> = vec![];
        let map = build_file_map(&files);
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_file_map_single_file() {
        let files = vec![ImportFileInfo {
            source_path: "/vault/test.md".to_string(),
            relative_path: "test.md".to_string(),
            name: "test.md".to_string(),
            file_type: ImportFileType::Markdown,
            size: 100,
            has_wiki_links: false,
            has_front_matter: false,
            has_callouts: false,
            has_dataview: false,
        }];
        let map = build_file_map(&files);

        assert_eq!(map.get("test"), Some(&"test.md".to_string()));
        assert_eq!(map.get("test.md"), Some(&"test.md".to_string()));
    }

    #[test]
    fn test_build_file_map_multiple_files() {
        let files = vec![
            ImportFileInfo {
                source_path: "/vault/notes/note1.md".to_string(),
                relative_path: "notes/note1.md".to_string(),
                name: "note1.md".to_string(),
                file_type: ImportFileType::Markdown,
                size: 100,
                has_wiki_links: false,
                has_front_matter: false,
                has_callouts: false,
                has_dataview: false,
            },
            ImportFileInfo {
                source_path: "/vault/note2.md".to_string(),
                relative_path: "note2.md".to_string(),
                name: "note2.md".to_string(),
                file_type: ImportFileType::Markdown,
                size: 100,
                has_wiki_links: false,
                has_front_matter: false,
                has_callouts: false,
                has_dataview: false,
            },
        ];
        let map = build_file_map(&files);

        assert_eq!(map.get("note1"), Some(&"notes/note1.md".to_string()));
        assert_eq!(map.get("note2"), Some(&"note2.md".to_string()));
    }

    #[test]
    fn test_build_file_map_ignores_non_markdown() {
        let files = vec![
            ImportFileInfo {
                source_path: "/vault/note.md".to_string(),
                relative_path: "note.md".to_string(),
                name: "note.md".to_string(),
                file_type: ImportFileType::Markdown,
                size: 100,
                has_wiki_links: false,
                has_front_matter: false,
                has_callouts: false,
                has_dataview: false,
            },
            ImportFileInfo {
                source_path: "/vault/image.png".to_string(),
                relative_path: "image.png".to_string(),
                name: "image.png".to_string(),
                file_type: ImportFileType::Attachment,
                size: 5000,
                has_wiki_links: false,
                has_front_matter: false,
                has_callouts: false,
                has_dataview: false,
            },
        ];
        let map = build_file_map(&files);

        assert!(map.get("note").is_some());
        assert!(map.get("image").is_none());
    }

    #[test]
    fn test_build_file_map_case_insensitive() {
        let files = vec![ImportFileInfo {
            source_path: "/vault/MyNote.md".to_string(),
            relative_path: "MyNote.md".to_string(),
            name: "MyNote.md".to_string(),
            file_type: ImportFileType::Markdown,
            size: 100,
            has_wiki_links: false,
            has_front_matter: false,
            has_callouts: false,
            has_dataview: false,
        }];
        let map = build_file_map(&files);

        // Keys are lowercased
        assert_eq!(map.get("mynote"), Some(&"MyNote.md".to_string()));
        assert_eq!(map.get("mynote.md"), Some(&"MyNote.md".to_string()));
    }

    // ============================================================================
    // strip_notion_uuid Tests
    // ============================================================================

    #[test]
    fn test_strip_notion_uuid_with_uuid() {
        assert_eq!(
            strip_notion_uuid("Page Title abc123def45678901234567890123456.md"),
            "Page Title.md"
        );
    }

    #[test]
    fn test_strip_notion_uuid_no_uuid() {
        assert_eq!(strip_notion_uuid("No UUID.md"), "No UUID.md");
    }

    #[test]
    fn test_strip_notion_uuid_different_extension() {
        assert_eq!(
            strip_notion_uuid("Test 12345678901234567890123456789012.txt"),
            "Test.txt"
        );
    }

    #[test]
    fn test_strip_notion_uuid_partial_uuid() {
        // UUID must be exactly 32 hex chars
        assert_eq!(
            strip_notion_uuid("Short 1234567890123456.md"),
            "Short 1234567890123456.md"
        );
    }

    #[test]
    fn test_strip_notion_uuid_no_extension() {
        // Without extension, pattern won't match
        assert_eq!(
            strip_notion_uuid("File 12345678901234567890123456789012"),
            "File 12345678901234567890123456789012"
        );
    }

    #[test]
    fn test_strip_notion_uuid_preserves_spaces() {
        assert_eq!(
            strip_notion_uuid("My Cool Page 12345678901234567890123456789012.md"),
            "My Cool Page.md"
        );
    }

    // ============================================================================
    // convert_wiki_links Tests
    // ============================================================================

    #[test]
    fn test_convert_wiki_links_basic() {
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
    fn test_convert_wiki_links_with_anchor() {
        let mut file_map = HashMap::new();
        file_map.insert("page".to_string(), "page.md".to_string());

        let content = "Link to [[Page#heading]].";
        let (converted, count, _) = convert_wiki_links(content, &file_map, "test.md");

        assert_eq!(count, 1);
        assert!(converted.contains("[Page](page.md#heading)"));
    }

    #[test]
    fn test_convert_wiki_links_with_anchor_and_alias() {
        let mut file_map = HashMap::new();
        file_map.insert("page".to_string(), "page.md".to_string());

        let content = "Link to [[Page#section|my link]].";
        let (converted, count, _) = convert_wiki_links(content, &file_map, "test.md");

        assert_eq!(count, 1);
        assert!(converted.contains("[my link](page.md#section)"));
    }

    #[test]
    fn test_convert_wiki_links_multiple_on_same_line() {
        let mut file_map = HashMap::new();
        file_map.insert("note1".to_string(), "note1.md".to_string());
        file_map.insert("note2".to_string(), "note2.md".to_string());

        let content = "See [[Note1]] and also [[Note2]].";
        let (converted, count, _) = convert_wiki_links(content, &file_map, "test.md");

        assert_eq!(count, 2);
        assert!(converted.contains("[Note1](note1.md)"));
        assert!(converted.contains("[Note2](note2.md)"));
    }

    #[test]
    fn test_convert_wiki_links_empty_content() {
        let file_map = HashMap::new();
        let (converted, count, broken) = convert_wiki_links("", &file_map, "test.md");

        assert_eq!(count, 0);
        assert_eq!(converted, "");
        assert!(broken.is_empty());
    }

    #[test]
    fn test_convert_wiki_links_no_links() {
        let file_map = HashMap::new();
        let content = "Just some regular text.";
        let (converted, count, broken) = convert_wiki_links(content, &file_map, "test.md");

        assert_eq!(count, 0);
        assert_eq!(converted, content);
        assert!(broken.is_empty());
    }

    #[test]
    fn test_convert_wiki_links_resolves_with_md_extension() {
        let mut file_map = HashMap::new();
        file_map.insert("note.md".to_string(), "note.md".to_string());

        let content = "Link to [[note]].";
        let (converted, count, _) = convert_wiki_links(content, &file_map, "test.md");

        assert_eq!(count, 1);
        assert!(converted.contains("[note](note.md)"));
    }

    // ============================================================================
    // convert_callouts Tests
    // ============================================================================

    #[test]
    fn test_convert_callouts_with_title() {
        let content = "> [!note] Important info\n> Content here";
        let converted = convert_callouts(content);
        assert!(converted.contains("> **NOTE:** Important info"));
    }

    #[test]
    fn test_convert_callouts_without_title() {
        let content = "> [!warning]\n> Be careful";
        let converted = convert_callouts(content);
        assert!(converted.contains("> **WARNING**"));
        // Verify the next line is preserved
        assert!(converted.contains("> Be careful"));
    }

    #[test]
    fn test_convert_callouts_different_types() {
        let types = vec!["note", "warning", "tip", "important", "caution", "info"];
        for callout_type in types {
            let content = format!("> [!{}] Title", callout_type);
            let converted = convert_callouts(&content);
            assert!(converted.contains(&format!("> **{}:** Title", callout_type.to_uppercase())));
        }
    }

    #[test]
    fn test_convert_callouts_preserves_content() {
        let content = "> [!note] Title\n> Line 1\n> Line 2";
        let converted = convert_callouts(content);
        assert!(converted.contains("> Line 1"));
        assert!(converted.contains("> Line 2"));
    }

    #[test]
    fn test_convert_callouts_multiple() {
        let content = "> [!note] First\n\nText\n\n> [!warning] Second";
        let converted = convert_callouts(content);
        assert!(converted.contains("> **NOTE:** First"));
        assert!(converted.contains("> **WARNING:** Second"));
    }

    #[test]
    fn test_convert_callouts_no_callouts() {
        let content = "> Regular blockquote\n> More text";
        let converted = convert_callouts(content);
        assert_eq!(converted, content);
    }

    // ============================================================================
    // remove_dataview Tests
    // ============================================================================

    #[test]
    fn test_remove_dataview_block() {
        let content = "# Title\n\n```dataview\nTABLE file.name\n```\n\nMore content";
        let result = remove_dataview(content);
        assert!(!result.contains("dataview"));
        assert!(result.contains("# Title"));
        assert!(result.contains("More content"));
    }

    #[test]
    fn test_remove_dataview_js_block() {
        let content = "# Title\n\n```dataviewjs\nconst pages = dv.pages();\n```\n\nText";
        let result = remove_dataview(content);
        assert!(!result.contains("dataviewjs"));
        assert!(!result.contains("dv.pages"));
    }

    #[test]
    fn test_remove_dataview_inline() {
        let content = "The value is `=this.field` inline.";
        let result = remove_dataview(content);
        assert!(!result.contains("`=this.field`"));
        assert!(result.contains("The value is"));
        assert!(result.contains("inline."));
    }

    #[test]
    fn test_remove_dataview_multiple() {
        let content = "```dataview\nTABLE\n```\n\nText\n\n```dataview\nLIST\n```";
        let result = remove_dataview(content);
        assert!(!result.contains("TABLE"));
        assert!(!result.contains("LIST"));
        assert!(result.contains("Text"));
    }

    #[test]
    fn test_remove_dataview_preserves_other_code_blocks() {
        let content = "```javascript\nconst x = 1;\n```\n\n```dataview\nTABLE\n```";
        let result = remove_dataview(content);
        assert!(result.contains("```javascript"));
        assert!(result.contains("const x = 1"));
        assert!(!result.contains("TABLE"));
    }

    // ============================================================================
    // csv_to_markdown_table Tests
    // ============================================================================

    #[test]
    fn test_csv_to_markdown_table_basic() {
        let csv = "Name,Age,City\nAlice,30,NYC\nBob,25,LA";
        let table = csv_to_markdown_table(csv).unwrap();

        assert!(table.contains("| Name | Age | City |"));
        assert!(table.contains("| --- | --- | --- |"));
        assert!(table.contains("| Alice | 30 | NYC |"));
        assert!(table.contains("| Bob | 25 | LA |"));
    }

    #[test]
    fn test_csv_to_markdown_table_single_column() {
        let csv = "Name\nAlice\nBob";
        let table = csv_to_markdown_table(csv).unwrap();

        assert!(table.contains("| Name |"));
        assert!(table.contains("| --- |"));
        assert!(table.contains("| Alice |"));
    }

    #[test]
    fn test_csv_to_markdown_table_empty() {
        let csv = "";
        let table = csv_to_markdown_table(csv).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn test_csv_to_markdown_table_headers_only() {
        let csv = "Col1,Col2,Col3";
        let table = csv_to_markdown_table(csv).unwrap();
        assert!(table.contains("| Col1 | Col2 | Col3 |"));
        assert!(table.contains("| --- | --- | --- |"));
    }

    #[test]
    fn test_csv_to_markdown_table_special_characters() {
        // Pipe characters should be escaped/sanitized
        let csv = "Name,Description\nTest,A | B";
        let table = csv_to_markdown_table(csv).unwrap();
        // sanitize_csv_cell should handle pipes
        assert!(table.contains("| Name | Description |"));
    }

    #[test]
    fn test_csv_to_markdown_table_quoted_values() {
        let csv = "Name,Quote\nAlice,\"Hello, World\"";
        let table = csv_to_markdown_table(csv).unwrap();
        assert!(table.contains("Hello, World"));
    }

    // ============================================================================
    // detect_source_type Tests
    // ============================================================================

    #[test]
    fn test_detect_source_type_not_found() {
        let result = detect_source_type(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ImportError::FileNotFound(_)));
    }

    #[test]
    fn test_detect_source_type_not_directory() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();

        let result = detect_source_type(&file_path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ImportError::InvalidPath(_)));
    }

    #[test]
    fn test_detect_source_type_obsidian() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();

        let result = detect_source_type(temp.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ImportSourceType::Obsidian);
    }

    #[test]
    fn test_detect_source_type_notion() {
        let temp = TempDir::new().unwrap();
        // Create a file with UUID pattern
        let filename = "Page abc123def45678901234567890123456.md";
        std::fs::write(temp.path().join(filename), "content").unwrap();

        let result = detect_source_type(temp.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ImportSourceType::Notion);
    }

    #[test]
    fn test_detect_source_type_generic() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("regular.md"), "content").unwrap();

        let result = detect_source_type(temp.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ImportSourceType::Generic);
    }

    // ============================================================================
    // analyze_obsidian_vault Tests
    // ============================================================================

    #[test]
    fn test_analyze_obsidian_vault_not_found() {
        let result = analyze_obsidian_vault(Path::new("/nonexistent/vault"));
        assert!(result.is_err());
    }

    #[test]
    fn test_analyze_obsidian_vault_empty() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();

        let result = analyze_obsidian_vault(temp.path());
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert_eq!(analysis.source_type, ImportSourceType::Obsidian);
        assert_eq!(analysis.total_files, 0);
        assert_eq!(analysis.markdown_files, 0);
    }

    #[test]
    fn test_analyze_obsidian_vault_with_markdown() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::write(temp.path().join("note.md"), "# Hello\nWorld").unwrap();

        let result = analyze_obsidian_vault(temp.path());
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert_eq!(analysis.total_files, 1);
        assert_eq!(analysis.markdown_files, 1);
    }

    #[test]
    fn test_analyze_obsidian_vault_with_wiki_links() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::write(temp.path().join("note.md"), "Link to [[other note]]").unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.wiki_links, 1);
        assert_eq!(analysis.files_with_wiki_links, 1);
        assert!(analysis.files_to_import[0].has_wiki_links);
    }

    #[test]
    fn test_analyze_obsidian_vault_with_callouts() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::write(temp.path().join("note.md"), "> [!note] Title\n> Content").unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.callouts, 1);
        assert!(analysis.files_to_import[0].has_callouts);
    }

    #[test]
    fn test_analyze_obsidian_vault_with_dataview() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::write(
            temp.path().join("note.md"),
            "# Note\n```dataview\nTABLE file.name\n```",
        )
        .unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.dataview_blocks, 1);
        assert!(analysis.files_to_import[0].has_dataview);
    }

    #[test]
    fn test_analyze_obsidian_vault_with_front_matter() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::write(
            temp.path().join("note.md"),
            "---\ntitle: Test\ntags: [a, b]\n---\n# Content",
        )
        .unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.front_matter, 1);
        assert!(analysis.files_to_import[0].has_front_matter);
    }

    #[test]
    fn test_analyze_obsidian_vault_skips_hidden() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::create_dir(temp.path().join(".hidden")).unwrap();
        std::fs::write(temp.path().join(".hidden/secret.md"), "secret").unwrap();
        std::fs::write(temp.path().join("visible.md"), "visible").unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.total_files, 1);
        assert_eq!(analysis.files_to_import[0].name, "visible.md".to_string());
    }

    #[test]
    fn test_analyze_obsidian_vault_counts_attachments() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::write(temp.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();
        std::fs::write(temp.path().join("doc.pdf"), "PDF content").unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.attachments, 2);
    }

    #[test]
    fn test_analyze_obsidian_vault_empty_pages() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::write(temp.path().join("empty.md"), "").unwrap();
        std::fs::write(temp.path().join("whitespace.md"), "   \n\n   ").unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.empty_pages.len(), 2);
    }

    #[test]
    fn test_analyze_obsidian_vault_untitled_pages() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        // Put untitled files in different locations to avoid case-insensitive filesystem issues on macOS
        std::fs::write(temp.path().join("Untitled.md"), "content").unwrap();
        std::fs::create_dir(temp.path().join("subfolder")).unwrap();
        std::fs::write(temp.path().join("subfolder/untitled.md"), "content").unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.untitled_pages.len(), 2);
    }

    #[test]
    fn test_analyze_obsidian_vault_folder_count() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join(".obsidian")).unwrap();
        std::fs::create_dir_all(temp.path().join("folder1/subfolder")).unwrap();
        std::fs::create_dir(temp.path().join("folder2")).unwrap();

        let result = analyze_obsidian_vault(temp.path());
        let analysis = result.unwrap();

        assert!(analysis.folders >= 2); // At least folder1, folder1/subfolder, folder2
    }

    // ============================================================================
    // analyze_notion_export Tests
    // ============================================================================

    #[test]
    fn test_analyze_notion_export_not_found() {
        let result = analyze_notion_export(Path::new("/nonexistent/export"));
        assert!(result.is_err());
    }

    #[test]
    fn test_analyze_notion_export_empty() {
        let temp = TempDir::new().unwrap();

        let result = analyze_notion_export(temp.path());
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert_eq!(analysis.source_type, ImportSourceType::Notion);
        assert_eq!(analysis.total_files, 0);
    }

    #[test]
    fn test_analyze_notion_export_with_csv() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("database.csv"), "Col1,Col2\na,b").unwrap();

        let result = analyze_notion_export(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.csv_databases, 1);
    }

    #[test]
    fn test_analyze_notion_export_untitled() {
        let temp = TempDir::new().unwrap();
        let filename = "Untitled 12345678901234567890123456789012.md";
        std::fs::write(temp.path().join(filename), "content").unwrap();

        let result = analyze_notion_export(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.untitled_pages.len(), 1);
    }

    #[test]
    fn test_analyze_notion_export_empty_file() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("empty.md"), "").unwrap();

        let result = analyze_notion_export(temp.path());
        let analysis = result.unwrap();

        assert_eq!(analysis.empty_pages.len(), 1);
    }

    // ============================================================================
    // Import Execution Tests
    // ============================================================================

    #[test]
    fn test_import_obsidian_vault_empty() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let options = ImportOptions::default();

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert!(import_result.success);
        assert_eq!(import_result.files_imported, 0);
    }

    #[test]
    fn test_import_obsidian_vault_simple_file() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("note.md"), "# Hello World").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let options = ImportOptions::default();

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert!(import_result.success);
        assert_eq!(import_result.files_imported, 1);

        // Check file was created
        assert!(dest.path().join("note.md").exists());
    }

    #[test]
    fn test_import_obsidian_vault_with_wiki_link_conversion() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("main.md"), "Link to [[other]]").unwrap();
        std::fs::write(source.path().join("other.md"), "Other page").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.convert_wiki_links = true;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        let import_result = result.unwrap();

        assert!(import_result.links_converted > 0);
    }

    #[test]
    fn test_import_obsidian_vault_with_cancellation() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("note.md"), "content").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let options = ImportOptions::default();
        let token = CancellationToken::new();
        token.cancel(); // Cancel immediately

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, Some(token));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ImportError::Cancelled));
    }

    #[test]
    fn test_import_obsidian_vault_skip_empty_pages() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("empty.md"), "").unwrap();
        std::fs::write(source.path().join("nonempty.md"), "content").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.skip_empty_pages = true;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        let import_result = result.unwrap();

        assert_eq!(import_result.files_imported, 1);
        assert!(!dest.path().join("empty.md").exists());
        assert!(dest.path().join("nonempty.md").exists());
    }

    #[test]
    fn test_import_obsidian_vault_preserves_folder_structure() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::create_dir(source.path().join("subfolder")).unwrap();
        std::fs::write(source.path().join("subfolder/note.md"), "content").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.preserve_folder_structure = true;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());
        assert!(dest.path().join("subfolder/note.md").exists());
    }

    #[test]
    fn test_import_obsidian_vault_flattens_structure() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::create_dir(source.path().join("subfolder")).unwrap();
        std::fs::write(source.path().join("subfolder/note.md"), "content").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.preserve_folder_structure = false;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());
        assert!(dest.path().join("note.md").exists());
    }

    #[test]
    fn test_import_obsidian_vault_with_attachment() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.copy_attachments = true;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        let import_result = result.unwrap();

        assert_eq!(import_result.attachments_copied, 1);
        assert!(dest.path().join("image.png").exists());
    }

    #[test]
    fn test_import_obsidian_vault_skip_attachments() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.copy_attachments = false;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        let import_result = result.unwrap();

        assert_eq!(import_result.attachments_copied, 0);
        assert!(!dest.path().join("image.png").exists());
    }

    #[test]
    fn test_import_obsidian_vault_converts_callouts() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(
            source.path().join("note.md"),
            "> [!note] Important\n> Content",
        )
        .unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.convert_callouts = true;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(dest.path().join("note.md")).unwrap();
        assert!(content.contains("> **NOTE:**"));
    }

    #[test]
    fn test_import_obsidian_vault_removes_dataview() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(
            source.path().join("note.md"),
            "# Title\n```dataview\nTABLE\n```\nEnd",
        )
        .unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let options = ImportOptions::default();

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(dest.path().join("note.md")).unwrap();
        assert!(!content.contains("dataview"));
        assert!(content.contains("# Title"));
    }

    // ============================================================================
    // Notion Import Tests
    // ============================================================================

    #[test]
    fn test_import_notion_export_empty() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let options = NotionImportOptions::default();

        let result = import_notion_export(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert!(import_result.success);
    }

    #[test]
    fn test_import_notion_export_removes_uuids() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let filename = "My Page 12345678901234567890123456789012.md";
        std::fs::write(source.path().join(filename), "content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.remove_uuids = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        // Should be renamed without UUID
        assert!(dest.path().join("My Page.md").exists());
    }

    #[test]
    fn test_import_notion_export_keeps_uuids() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let filename = "My Page 12345678901234567890123456789012.md";
        std::fs::write(source.path().join(filename), "content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.remove_uuids = false;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        // Should keep original name
        assert!(dest.path().join(filename).exists());
    }

    #[test]
    fn test_import_notion_export_converts_csv() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(source.path().join("data.csv"), "Name,Value\nA,1").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.convert_csv_to_tables = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());

        // CSV should be converted to markdown
        assert!(dest.path().join("data.md").exists());
        let content = std::fs::read_to_string(dest.path().join("data.md")).unwrap();
        assert!(content.contains("| Name | Value |"));
    }

    #[test]
    fn test_import_notion_export_with_cancellation() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(source.path().join("note.md"), "content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let options = NotionImportOptions::default();
        let token = CancellationToken::new();
        token.cancel();

        let result = import_notion_export(&analysis, dest.path(), &options, None, Some(token));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ImportError::Cancelled));
    }

    // ============================================================================
    // Progress Callback Tests
    // ============================================================================

    #[test]
    fn test_import_with_progress_callback() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        // Create multiple files to trigger progress
        for i in 0..5 {
            std::fs::write(source.path().join(format!("note{}.md", i)), "content").unwrap();
        }

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let options = ImportOptions::default();

        // We can't easily test the callback because it requires Send + Sync
        // and RefCell isn't Sync. Just verify import works with None callback.
        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None);
        assert!(result.is_ok());
    }

    // ============================================================================
    // Source Detection Edge Cases
    // ============================================================================

    #[test]
    fn test_detect_source_type_generic_with_multiple_files() {
        // Folder with no .obsidian and no UUID-named files
        let source = TempDir::new().unwrap();
        std::fs::write(source.path().join("regular_file.md"), "# Test").unwrap();
        std::fs::write(source.path().join("another.txt"), "text").unwrap();
        std::fs::write(source.path().join("data.csv"), "a,b").unwrap();

        let result = detect_source_type(source.path()).unwrap();
        assert!(matches!(result, ImportSourceType::Generic));
    }

    #[test]
    fn test_detect_source_type_hidden_obsidian_in_subdir() {
        // .obsidian deeper than max_depth(2) shouldn't be detected
        let source = TempDir::new().unwrap();
        std::fs::create_dir_all(source.path().join("a/b/c/.obsidian")).unwrap();

        let result = detect_source_type(source.path()).unwrap();
        // Should not detect because .obsidian is too deep
        assert!(matches!(result, ImportSourceType::Generic));
    }

    // ============================================================================
    // Obsidian Analysis Error Paths
    // ============================================================================

    #[cfg(unix)]
    #[test]
    fn test_obsidian_analysis_unreadable_directory() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("note.md"), "content").unwrap();

        // Create an unreadable directory
        let unreadable = source.path().join("unreadable");
        std::fs::create_dir(&unreadable).unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();

        // Restore permissions for cleanup
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Should have an access warning for the unreadable directory
        assert!(!analysis.access_warnings.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_obsidian_analysis_unreadable_file() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        // Create a readable note
        std::fs::write(source.path().join("readable.md"), "[[link]]").unwrap();

        // Create an unreadable markdown file
        let unreadable = source.path().join("unreadable.md");
        std::fs::write(&unreadable, "content").unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();

        // Restore permissions for cleanup
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Should have recorded an access warning for the unreadable file
        assert!(analysis
            .access_warnings
            .iter()
            .any(|w| w.path.contains("unreadable.md")));
    }

    #[test]
    fn test_obsidian_analysis_other_file_type() {
        // Test that non-markdown, non-attachment files are counted as "Other"
        let source = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("note.md"), "content").unwrap();
        std::fs::write(source.path().join("config.json"), "{}").unwrap();
        std::fs::write(source.path().join("script.py"), "print('hi')").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();

        // Should have 3 total files (note.md, config.json, script.py)
        assert_eq!(analysis.total_files, 3);
        // Only one markdown file
        assert_eq!(analysis.markdown_files, 1);
        // The other files are "Other" type
        let other_count = analysis
            .files_to_import
            .iter()
            .filter(|f| matches!(f.file_type, ImportFileType::Other))
            .count();
        assert_eq!(other_count, 2);
    }

    #[test]
    fn test_obsidian_analysis_attachments() {
        let source = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();
        std::fs::write(source.path().join("note.md"), "content").unwrap();
        std::fs::write(source.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();
        std::fs::write(source.path().join("doc.pdf"), b"PDF content").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();

        assert_eq!(analysis.attachments, 2);
    }

    // ============================================================================
    // Notion Analysis Error Paths
    // ============================================================================

    #[cfg(unix)]
    #[test]
    fn test_notion_analysis_unreadable_directory() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        // Create a notion-style file
        std::fs::write(
            source
                .path()
                .join("note 12345678901234567890123456789012.md"),
            "content",
        )
        .unwrap();

        // Create an unreadable directory
        let unreadable = source.path().join("unreadable");
        std::fs::create_dir(&unreadable).unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();

        // Restore permissions for cleanup
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Should have an access warning
        assert!(!analysis.access_warnings.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_notion_analysis_metadata_error() {
        let source = TempDir::new().unwrap();

        // Create files that can be listed but not stat'd (difficult to simulate)
        // Instead we test the folder counting path
        let subdir = source.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("note.md"), "content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();

        // Should count the folder
        assert_eq!(analysis.folders, 1);
    }

    #[test]
    fn test_notion_analysis_attachments() {
        let source = TempDir::new().unwrap();
        std::fs::write(source.path().join("note.md"), "content").unwrap();
        std::fs::write(source.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();
        std::fs::write(source.path().join("doc.pdf"), b"PDF content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();

        assert_eq!(analysis.attachments, 2);
        assert_eq!(analysis.markdown_files, 1);
    }

    #[test]
    fn test_notion_analysis_empty_page() {
        let source = TempDir::new().unwrap();
        // Create an empty file
        std::fs::write(source.path().join("empty.md"), "").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();

        assert_eq!(analysis.empty_pages.len(), 1);
        assert!(analysis.empty_pages[0].contains("empty.md"));
    }

    // ============================================================================
    // Obsidian Import Error Paths
    // ============================================================================

    #[cfg(unix)]
    #[test]
    fn test_obsidian_import_read_error() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        // Create a file that will be in the analysis but unreadable during import
        let file_path = source.path().join("note.md");
        std::fs::write(&file_path, "content").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();

        // Make file unreadable after analysis
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let options = ImportOptions::default();
        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None).unwrap();

        // Restore permissions for cleanup
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Should have an error for the unreadable file
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].message.contains("Could not read file"));
    }

    #[test]
    fn test_obsidian_import_broken_wiki_links() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        // Create a file with wiki link to non-existent file
        std::fs::write(
            source.path().join("note.md"),
            "Check out [[nonexistent page]]!",
        )
        .unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.convert_wiki_links = true;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None).unwrap();

        // Should have a warning for the broken link
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].message.contains("Broken link"));
    }

    #[test]
    fn test_obsidian_import_skips_other_file_types() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        std::fs::write(source.path().join("note.md"), "content").unwrap();
        std::fs::write(source.path().join("config.json"), "{}").unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let options = ImportOptions::default();

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None).unwrap();

        // Only the markdown file should be imported
        assert_eq!(result.files_imported, 1);
        assert!(dest.path().join("note.md").exists());
        assert!(!dest.path().join("config.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_obsidian_import_attachment_error() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        std::fs::write(source.path().join("note.md"), "content").unwrap();
        let img_path = source.path().join("image.png");
        std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();

        // Make attachment unreadable
        std::fs::set_permissions(&img_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let mut options = ImportOptions::default();
        options.copy_attachments = true;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None).unwrap();

        // Restore permissions
        std::fs::set_permissions(&img_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Should have an error for the attachment
        assert!(!result.errors.is_empty());
    }

    // ============================================================================
    // Notion Import Error Paths
    // ============================================================================

    #[cfg(unix)]
    #[test]
    fn test_notion_import_read_error() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        let file_path = source.path().join("note.md");
        std::fs::write(&file_path, "content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();

        // Make file unreadable after analysis
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let options = NotionImportOptions::default();
        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        // Restore permissions
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!result.errors.is_empty());
        assert!(result.errors[0].message.contains("Could not read file"));
    }

    #[test]
    fn test_notion_import_with_folder_structure() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        // Create nested structure
        let subdir = source.path().join("Projects");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(
            subdir.join("note 12345678901234567890123456789012.md"),
            "content",
        )
        .unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.base.preserve_folder_structure = true;
        options.remove_uuids = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        assert!(result.success);
        // File should be at Projects/note.md (UUID removed, folder preserved)
        assert!(dest.path().join("Projects/note.md").exists());
    }

    #[test]
    fn test_notion_import_skips_empty_pages() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        // Create empty file
        std::fs::write(source.path().join("empty.md"), "").unwrap();
        std::fs::write(source.path().join("nonempty.md"), "content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.base.skip_empty_pages = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        // Only non-empty file should be imported
        assert_eq!(result.files_imported, 1);
        assert!(!dest.path().join("empty.md").exists());
        assert!(dest.path().join("nonempty.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_notion_import_attachment_error() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        std::fs::write(source.path().join("note.md"), "content").unwrap();
        let img_path = source.path().join("image.png");
        std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();

        // Make attachment unreadable
        std::fs::set_permissions(&img_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let mut options = NotionImportOptions::default();
        options.base.copy_attachments = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        // Restore permissions
        std::fs::set_permissions(&img_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!result.errors.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_notion_import_csv_read_error() {
        use std::os::unix::fs::PermissionsExt;

        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        let csv_path = source.path().join("data.csv");
        std::fs::write(&csv_path, "Name,Value\nA,1").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();

        // Make CSV unreadable
        std::fs::set_permissions(&csv_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let mut options = NotionImportOptions::default();
        options.convert_csv_to_tables = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        // Restore permissions
        std::fs::set_permissions(&csv_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!result.errors.is_empty());
        assert!(result.errors[0].message.contains("Could not read CSV"));
    }

    #[test]
    fn test_notion_import_csv_empty_produces_empty_table() {
        // Empty CSV produces empty output (valid case - exercises lines 739-741)
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        std::fs::write(source.path().join("empty.csv"), "").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.convert_csv_to_tables = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        // Empty CSV should still succeed
        assert!(result.success);
    }

    #[test]
    fn test_notion_import_csv_with_folder_structure() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        // Create nested CSV
        let subdir = source.path().join("Data");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("table.csv"), "Name,Value\nA,1").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.convert_csv_to_tables = true;
        options.base.preserve_folder_structure = true;

        let _result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        // CSV should be converted and placed in Data/table.md
        assert!(dest.path().join("Data/table.md").exists());
    }

    // ============================================================================
    // Progress Callback Tests
    // ============================================================================

    #[test]
    fn test_obsidian_import_progress_callback_phases() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        for i in 0..3 {
            std::fs::write(source.path().join(format!("note{}.md", i)), "content").unwrap();
        }

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let options = ImportOptions::default();

        let callback_count = Arc::new(AtomicUsize::new(0));
        let count_clone = callback_count.clone();

        let callback: ProgressCallback = Box::new(move |_progress| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let result =
            import_obsidian_vault(&analysis, dest.path(), &options, Some(callback), None).unwrap();

        assert!(result.success);
        // Should have been called multiple times (Converting, Finalizing, Complete phases)
        assert!(callback_count.load(Ordering::SeqCst) >= 3);
    }

    #[test]
    fn test_notion_import_progress_callback_phases() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        for i in 0..3 {
            std::fs::write(source.path().join(format!("note{}.md", i)), "content").unwrap();
        }

        let analysis = analyze_notion_export(source.path()).unwrap();
        let options = NotionImportOptions::default();

        let callback_count = Arc::new(AtomicUsize::new(0));
        let count_clone = callback_count.clone();

        let callback: ProgressCallback = Box::new(move |_progress| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let result =
            import_notion_export(&analysis, dest.path(), &options, Some(callback), None).unwrap();

        assert!(result.success);
        assert!(callback_count.load(Ordering::SeqCst) >= 3);
    }

    #[test]
    fn test_notion_import_updates_links_when_removing_uuids() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        // Create a file that links to another UUID-named file
        let target_name = "Target Page 12345678901234567890123456789012.md";
        std::fs::write(source.path().join(target_name), "Target content").unwrap();
        std::fs::write(
            source.path().join("linker.md"),
            format!("See [Target Page]({})", target_name),
        )
        .unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.remove_uuids = true;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        assert!(result.success);
        // Link should be updated to point to cleaned filename
        let content = std::fs::read_to_string(dest.path().join("linker.md")).unwrap();
        assert!(content.contains("](Target Page.md)"));
    }

    #[test]
    fn test_notion_import_without_folder_structure() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        // Create nested structure
        let subdir = source.path().join("Nested/Deep");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("note.md"), "content").unwrap();

        let analysis = analyze_notion_export(source.path()).unwrap();
        let mut options = NotionImportOptions::default();
        options.base.preserve_folder_structure = false;

        let result = import_notion_export(&analysis, dest.path(), &options, None, None).unwrap();

        assert!(result.success);
        // File should be flattened to root
        assert!(dest.path().join("note.md").exists());
    }

    #[test]
    fn test_import_attachments_disabled() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join(".obsidian")).unwrap();

        std::fs::write(source.path().join("note.md"), "content").unwrap();
        std::fs::write(source.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let analysis = analyze_obsidian_vault(source.path()).unwrap();
        let mut options = ImportOptions::default();
        options.copy_attachments = false;

        let result = import_obsidian_vault(&analysis, dest.path(), &options, None, None).unwrap();

        assert!(result.success);
        assert_eq!(result.attachments_copied, 0);
        assert!(!dest.path().join("image.png").exists());
    }
}
