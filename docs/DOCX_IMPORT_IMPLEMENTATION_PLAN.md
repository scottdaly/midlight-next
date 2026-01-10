# DOCX Import Implementation Plan

## ✅ Implementation Status: COMPLETE (January 2026)

| Component | Status | Location |
|-----------|--------|----------|
| Core Parser (Rust) | ✅ Complete | `src/services/docx_import.rs` |
| Tiptap Conversion | ✅ Complete | `src/services/docx_import.rs` |
| Image Extraction | ✅ Complete | `src/services/docx_import.rs` |
| Tauri Commands | ✅ Complete | `src/commands/import.rs` |
| TypeScript Client | ✅ Complete | `apps/desktop/src/lib/import.ts` |
| DocxImportDialog UI | ✅ Complete | `apps/desktop/src/lib/components/DocxImportDialog.svelte` |
| Menu Integration | ✅ Complete | `src/menu.rs` + `App.svelte` |
| Compilation | ✅ Passing | Rust + TypeScript both compile |

### Implementation Notes

- Used `zip` + `quick-xml` crates instead of `docx-rs` for reading (docx-rs is primarily a writing library)
- Created dedicated `DocxImportDialog.svelte` component (simpler flow than folder-based ImportWizard)
- Menu integration via "File > Import Word Document..." on macOS

---

## Overview

This document outlines the implementation plan for DOCX import functionality in Midlight Next. The feature allows users to import Microsoft Word documents into their workspace with full formatting preservation.

**Goal:** Parse `.docx` files and convert them to Midlight's internal format (Tiptap JSON) with proper handling of text formatting, headings, lists, and images.

---

## Architecture

### Conversion Pipeline

```
.docx file (ZIP containing XML)
    ↓
[zip crate - extract archive]
    ↓
word/document.xml + word/media/* + word/_rels/document.xml.rels
    ↓
[quick-xml crate - parse XML]
    ↓
ParsedParagraph[] with runs, formatting, images
    ↓
[Rust converter - docx_import.rs::convert_to_tiptap()]
    ↓
Tiptap JSON + extracted images
    ↓
[Store in workspace via App.svelte handler]
    ↓
Ready to edit
```

### Key Design Decisions

1. **Direct to Tiptap JSON:** Convert DOCX directly to Tiptap JSON in Rust (skip Markdown intermediate step) for maximum fidelity
2. **zip + quick-xml:** Used instead of docx-rs for reading (docx-rs is primarily a writing library with limited read support)
3. **Separate Dialog:** Created dedicated `DocxImportDialog.svelte` instead of extending ImportWizard (simpler single-file flow vs folder import)
4. **Image Extraction:** Extract embedded images from word/media/ with relationship ID mapping from word/_rels/document.xml.rels

---

## Implementation Tasks

### Phase 1: Core Parser (Rust)

#### 1.1 Create `docx_import.rs` Service

**Location:** `apps/desktop/src-tauri/src/services/docx_import.rs`

```rust
use docx_rs::*;
use std::path::Path;
use crate::services::import_security::{sanitize_filename, ImportConfig};

pub struct DocxImportResult {
    pub tiptap_json: serde_json::Value,
    pub images: Vec<ExtractedImage>,
    pub warnings: Vec<ImportWarning>,
}

pub struct ExtractedImage {
    pub id: String,           // Generated hash-based ID
    pub data: Vec<u8>,        // Image bytes
    pub content_type: String, // image/png, image/jpeg, etc.
    pub original_name: String,
}

pub struct ImportWarning {
    pub code: String,
    pub message: String,
    pub context: Option<String>,
}

/// Parse a DOCX file and convert to Tiptap JSON
pub fn import_docx(file_path: &Path) -> Result<DocxImportResult, ImportError> {
    // 1. Validate file
    validate_docx_file(file_path)?;

    // 2. Parse DOCX
    let docx = read_docx(file_path)?;

    // 3. Extract images
    let images = extract_images(&docx)?;

    // 4. Convert to Tiptap JSON
    let (tiptap_json, warnings) = convert_to_tiptap(&docx, &images)?;

    Ok(DocxImportResult {
        tiptap_json,
        images,
        warnings,
    })
}
```

#### 1.2 DOCX Parsing (`read_docx`)

```rust
use std::fs::File;
use std::io::Read;
use docx_rs::read_docx;

fn parse_docx_file(path: &Path) -> Result<Docx, ImportError> {
    // Read file bytes
    let mut file = File::open(path)
        .map_err(|e| ImportError::IoError(format!("Failed to open file: {}", e)))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| ImportError::IoError(format!("Failed to read file: {}", e)))?;

    // Validate file size
    if buffer.len() > ImportConfig::MAX_CONTENT_SIZE {
        return Err(ImportError::FileTooLarge(path.to_string_lossy().to_string()));
    }

    // Parse with docx-rs
    read_docx(&buffer)
        .map_err(|e| ImportError::DocxParse(format!("Failed to parse DOCX: {}", e)))
}
```

#### 1.3 Image Extraction

```rust
use sha2::{Sha256, Digest};

fn extract_images(docx: &Docx) -> Result<Vec<ExtractedImage>, ImportError> {
    let mut images = Vec::new();

    // Iterate through document relationships
    for (rel_id, image_data) in docx.images.iter() {
        // Generate unique ID from content hash
        let mut hasher = Sha256::new();
        hasher.update(&image_data.data);
        let hash = format!("{:x}", hasher.finalize());
        let id = format!("img-{}", &hash[..12]);

        // Detect content type
        let content_type = detect_image_type(&image_data.data)?;

        images.push(ExtractedImage {
            id,
            data: image_data.data.clone(),
            content_type,
            original_name: image_data.name.clone().unwrap_or_default(),
        });
    }

    Ok(images)
}

fn detect_image_type(data: &[u8]) -> Result<String, ImportError> {
    // Check magic bytes
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Ok("image/png".to_string())
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Ok("image/jpeg".to_string())
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Ok("image/gif".to_string())
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        Ok("image/webp".to_string())
    } else {
        // Default to PNG for unknown
        Ok("image/png".to_string())
    }
}
```

### Phase 2: Tiptap Conversion

#### 2.1 Main Converter

```rust
fn convert_to_tiptap(
    docx: &Docx,
    images: &[ExtractedImage],
) -> Result<(serde_json::Value, Vec<ImportWarning>), ImportError> {
    let mut content: Vec<serde_json::Value> = Vec::new();
    let mut warnings: Vec<ImportWarning> = Vec::new();
    let mut list_context = ListContext::default();

    // Build image ID map (relationship ID → extracted image ID)
    let image_map: HashMap<String, &ExtractedImage> = images
        .iter()
        .map(|img| (img.original_rel_id.clone(), img))
        .collect();

    for element in &docx.document.body.children {
        match element {
            DocumentChild::Paragraph(p) => {
                if let Some(node) = convert_paragraph(p, &image_map, &mut list_context, &mut warnings)? {
                    content.push(node);
                }
            }
            DocumentChild::Table(t) => {
                // Tables not yet supported
                warnings.push(ImportWarning {
                    code: "unsupported_table".to_string(),
                    message: "Tables are not yet supported and will be skipped".to_string(),
                    context: None,
                });
            }
            _ => {}
        }
    }

    let tiptap_doc = serde_json::json!({
        "type": "doc",
        "content": content
    });

    Ok((tiptap_doc, warnings))
}
```

#### 2.2 Paragraph Conversion

```rust
#[derive(Default)]
struct ListContext {
    current_list_type: Option<ListType>,
    current_level: usize,
    list_stack: Vec<serde_json::Value>,
}

fn convert_paragraph(
    para: &Paragraph,
    image_map: &HashMap<String, &ExtractedImage>,
    list_context: &mut ListContext,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Option<serde_json::Value>, ImportError> {
    // Check if this is a heading (via style)
    if let Some(style) = &para.property.style {
        if let Some(heading_level) = parse_heading_style(&style.val) {
            return Ok(Some(create_heading_node(para, heading_level, warnings)?));
        }
    }

    // Check if this is a list item
    if let Some(numbering) = &para.property.numbering {
        return convert_list_item(para, numbering, list_context, warnings);
    }

    // Regular paragraph
    let content = convert_runs(&para.children, image_map, warnings)?;

    if content.is_empty() {
        // Empty paragraph
        return Ok(Some(serde_json::json!({
            "type": "paragraph"
        })));
    }

    let mut node = serde_json::json!({
        "type": "paragraph",
        "content": content
    });

    // Add alignment if specified
    if let Some(align) = &para.property.alignment {
        node["attrs"] = serde_json::json!({
            "textAlign": convert_alignment(align)
        });
    }

    Ok(Some(node))
}
```

#### 2.3 Text Run Conversion

```rust
fn convert_runs(
    children: &[ParagraphChild],
    image_map: &HashMap<String, &ExtractedImage>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Vec<serde_json::Value>, ImportError> {
    let mut result: Vec<serde_json::Value> = Vec::new();

    for child in children {
        match child {
            ParagraphChild::Run(run) => {
                for run_child in &run.children {
                    match run_child {
                        RunChild::Text(text) => {
                            let mut node = serde_json::json!({
                                "type": "text",
                                "text": text.text
                            });

                            // Collect marks
                            let marks = collect_marks(&run.run_property)?;
                            if !marks.is_empty() {
                                node["marks"] = serde_json::Value::Array(marks);
                            }

                            result.push(node);
                        }
                        RunChild::Drawing(drawing) => {
                            // Handle inline images
                            if let Some(image_node) = convert_drawing(drawing, image_map, warnings)? {
                                result.push(image_node);
                            }
                        }
                        _ => {}
                    }
                }
            }
            ParagraphChild::Hyperlink(hyperlink) => {
                // Handle hyperlinks
                result.extend(convert_hyperlink(hyperlink, warnings)?);
            }
            _ => {}
        }
    }

    Ok(result)
}
```

#### 2.4 Mark Collection

```rust
fn collect_marks(props: &RunProperty) -> Result<Vec<serde_json::Value>, ImportError> {
    let mut marks: Vec<serde_json::Value> = Vec::new();

    // Bold
    if props.bold.as_ref().map_or(false, |b| b.val) {
        marks.push(serde_json::json!({"type": "bold"}));
    }

    // Italic
    if props.italic.as_ref().map_or(false, |i| i.val) {
        marks.push(serde_json::json!({"type": "italic"}));
    }

    // Underline
    if props.underline.is_some() {
        marks.push(serde_json::json!({"type": "underline"}));
    }

    // Strikethrough
    if props.strike.as_ref().map_or(false, |s| s.val) {
        marks.push(serde_json::json!({"type": "strike"}));
    }

    // Text color
    if let Some(color) = &props.color {
        if let Some(hex) = &color.val {
            marks.push(serde_json::json!({
                "type": "textStyle",
                "attrs": {
                    "color": format!("#{}", hex)
                }
            }));
        }
    }

    // Highlight/background color
    if let Some(highlight) = &props.highlight {
        let color = convert_highlight_color(&highlight.val);
        marks.push(serde_json::json!({
            "type": "highlight",
            "attrs": {
                "color": color
            }
        }));
    }

    // Font size (half-points to px: divide by 2)
    if let Some(sz) = &props.sz {
        let font_size_px = sz.val / 2;
        marks.push(serde_json::json!({
            "type": "textStyle",
            "attrs": {
                "fontSize": format!("{}px", font_size_px)
            }
        }));
    }

    // Font family
    if let Some(fonts) = &props.fonts {
        if let Some(ascii_font) = &fonts.ascii {
            marks.push(serde_json::json!({
                "type": "textStyle",
                "attrs": {
                    "fontFamily": ascii_font
                }
            }));
        }
    }

    Ok(marks)
}
```

#### 2.5 Heading Conversion

```rust
fn parse_heading_style(style_id: &str) -> Option<u8> {
    match style_id {
        "Heading1" | "heading1" | "Title" => Some(1),
        "Heading2" | "heading2" | "Subtitle" => Some(2),
        "Heading3" | "heading3" => Some(3),
        "Heading4" | "heading4" => Some(4),
        "Heading5" | "heading5" => Some(5),
        "Heading6" | "heading6" => Some(6),
        _ => None,
    }
}

fn create_heading_node(
    para: &Paragraph,
    level: u8,
    warnings: &mut Vec<ImportWarning>,
) -> Result<serde_json::Value, ImportError> {
    let content = convert_runs(&para.children, &HashMap::new(), warnings)?;

    Ok(serde_json::json!({
        "type": "heading",
        "attrs": {
            "level": level
        },
        "content": content
    }))
}
```

#### 2.6 List Conversion

```rust
fn convert_list_item(
    para: &Paragraph,
    numbering: &NumberingProperty,
    list_context: &mut ListContext,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Option<serde_json::Value>, ImportError> {
    let level = numbering.ilvl.as_ref().map_or(0, |l| l.val as usize);
    let num_id = numbering.num_id.as_ref().map_or(0, |n| n.val);

    // Determine list type from numbering definition
    let list_type = determine_list_type(num_id);

    let content = convert_runs(&para.children, &HashMap::new(), warnings)?;

    let list_item = serde_json::json!({
        "type": "listItem",
        "content": [{
            "type": "paragraph",
            "content": content
        }]
    });

    // Build appropriate list structure
    let list_type_str = match list_type {
        ListType::Bullet => "bulletList",
        ListType::Ordered => "orderedList",
    };

    Ok(Some(serde_json::json!({
        "type": list_type_str,
        "content": [list_item]
    })))
}

#[derive(Clone, Copy)]
enum ListType {
    Bullet,
    Ordered,
}

fn determine_list_type(num_id: usize) -> ListType {
    // In practice, need to look up numbering definition
    // Simplified: odd = bullet, even = ordered
    if num_id % 2 == 1 {
        ListType::Bullet
    } else {
        ListType::Ordered
    }
}
```

### Phase 3: Tauri Commands

#### 3.1 Create Command Handler

**Location:** `apps/desktop/src-tauri/src/commands/import.rs` (extend existing)

```rust
use crate::services::docx_import::{import_docx, DocxImportResult};

#[derive(Serialize)]
pub struct DocxAnalysis {
    pub file_name: String,
    pub file_size: u64,
    pub paragraph_count: usize,
    pub image_count: usize,
    pub has_tables: bool,
    pub warnings: Vec<String>,
}

/// Analyze a DOCX file without importing
#[tauri::command]
pub async fn import_analyze_docx(file_path: String) -> Result<DocxAnalysis, String> {
    let path = PathBuf::from(&file_path);

    // Validate path
    if !path.exists() {
        return Err("File not found".to_string());
    }

    if !path.extension().map_or(false, |ext| ext == "docx") {
        return Err("File must be a .docx file".to_string());
    }

    // Quick analysis without full conversion
    let metadata = std::fs::metadata(&path)
        .map_err(|e| e.to_string())?;

    let result = import_docx(&path)
        .map_err(|e| e.to_string())?;

    Ok(DocxAnalysis {
        file_name: path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        file_size: metadata.len(),
        paragraph_count: count_paragraphs(&result.tiptap_json),
        image_count: result.images.len(),
        has_tables: false, // Tables not yet supported
        warnings: result.warnings.iter().map(|w| w.message.clone()).collect(),
    })
}

/// Import a DOCX file into the workspace
#[tauri::command]
pub async fn import_docx_file(
    app: tauri::AppHandle,
    file_path: String,
    workspace_root: String,
    dest_filename: Option<String>,
) -> Result<ImportResult, String> {
    let path = PathBuf::from(&file_path);
    let workspace = PathBuf::from(&workspace_root);

    // Parse DOCX
    let result = import_docx(&path)
        .map_err(|e| e.to_string())?;

    // Determine destination filename
    let base_name = dest_filename.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    });
    let dest_path = workspace.join(format!("{}.md", base_name));

    // Use workspace manager to save
    let workspace_manager = app.state::<WorkspaceManager>();

    // Save images first
    for image in &result.images {
        workspace_manager
            .save_image(&workspace, &image.id, &image.data)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Save document
    workspace_manager
        .save_document(&workspace, &dest_path, &result.tiptap_json, "import")
        .await
        .map_err(|e| e.to_string())?;

    // Emit progress complete
    app.emit_all("import-docx-complete", serde_json::json!({
        "path": dest_path.to_string_lossy(),
        "warnings": result.warnings.len()
    })).ok();

    Ok(ImportResult {
        success: true,
        files_imported: 1,
        errors: vec![],
        warnings: result.warnings.iter().map(|w| w.message.clone()).collect(),
        dest_path: dest_path.to_string_lossy().to_string(),
    })
}
```

#### 3.2 Register Commands in `lib.rs`

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::import::import_analyze_docx,
    commands::import::import_docx_file,
])
```

### Phase 4: Frontend Integration

#### 4.1 Update Import Client

**Location:** `apps/desktop/src/lib/import.ts`

```typescript
export interface DocxAnalysis {
  fileName: string;
  fileSize: number;
  paragraphCount: number;
  imageCount: number;
  hasTables: boolean;
  warnings: string[];
}

export async function analyzeDocx(filePath: string): Promise<DocxAnalysis> {
  return invoke('import_analyze_docx', { filePath });
}

export async function importDocxFile(
  filePath: string,
  workspaceRoot: string,
  destFilename?: string
): Promise<ImportResult> {
  return invoke('import_docx_file', {
    filePath,
    workspaceRoot,
    destFilename,
  });
}
```

#### 4.2 Update ImportWizard.svelte

Add DOCX option to the wizard:

```svelte
<script lang="ts">
  // Add to existing imports
  import { analyzeDocx, importDocxFile } from '$lib/import';

  // Add DOCX to source type detection
  async function handleSelectFile() {
    const selected = await open({
      filters: [
        { name: 'Word Documents', extensions: ['docx'] },
        { name: 'All Files', extensions: ['*'] }
      ]
    });

    if (selected && typeof selected === 'string') {
      sourcePath = selected;
      sourceType = 'docx';
      await handleAnalyze();
    }
  }

  async function handleAnalyze() {
    if (sourceType === 'docx') {
      isAnalyzing = true;
      try {
        analysis = await analyzeDocx(sourcePath!);
        step = 'options';
      } catch (err) {
        error = err instanceof Error ? err.message : 'Analysis failed';
      } finally {
        isAnalyzing = false;
      }
    }
    // ... existing Obsidian/Notion handling
  }

  async function handleImport() {
    if (sourceType === 'docx') {
      isImporting = true;
      try {
        result = await importDocxFile(sourcePath!, workspaceRoot);
        step = 'complete';
      } catch (err) {
        error = err instanceof Error ? err.message : 'Import failed';
      } finally {
        isImporting = false;
      }
    }
    // ... existing handling
  }
</script>

<!-- Add DOCX button to Select step -->
{#if step === 'select'}
  <div class="flex flex-col gap-4">
    <Button onclick={handleSelectFolder}>
      Import Obsidian/Notion Folder
    </Button>
    <Button onclick={handleSelectFile} variant="outline">
      Import Word Document (.docx)
    </Button>
  </div>
{/if}
```

### Phase 5: Testing

#### 5.1 Unit Tests (Rust)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading_style() {
        assert_eq!(parse_heading_style("Heading1"), Some(1));
        assert_eq!(parse_heading_style("heading2"), Some(2));
        assert_eq!(parse_heading_style("Normal"), None);
    }

    #[test]
    fn test_detect_image_type() {
        // PNG magic bytes
        let png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_image_type(&png).unwrap(), "image/png");

        // JPEG magic bytes
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_image_type(&jpeg).unwrap(), "image/jpeg");
    }

    #[test]
    fn test_convert_alignment() {
        assert_eq!(convert_alignment(&JustificationVal::Center), "center");
        assert_eq!(convert_alignment(&JustificationVal::Left), "left");
        assert_eq!(convert_alignment(&JustificationVal::Right), "right");
        assert_eq!(convert_alignment(&JustificationVal::Both), "justify");
    }

    #[test]
    fn test_collect_marks_bold() {
        let props = RunProperty {
            bold: Some(Bold { val: true }),
            ..Default::default()
        };
        let marks = collect_marks(&props).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0]["type"], "bold");
    }
}
```

#### 5.2 Integration Tests

Create test DOCX files covering:
- Simple paragraphs
- Headings (H1-H6)
- Bold, italic, underline formatting
- Bullet and numbered lists
- Nested lists
- Embedded images
- Text colors and highlights
- Mixed formatting

---

## Supported Elements

### Phase 1 (Core)

| Element | Support | Notes |
|---------|---------|-------|
| Paragraph | ✅ Full | Text alignment preserved |
| Heading 1-6 | ✅ Full | Via Word styles |
| Bold | ✅ Full | |
| Italic | ✅ Full | |
| Underline | ✅ Full | |
| Strikethrough | ✅ Full | |
| Text color | ✅ Full | Hex color preserved |
| Highlight | ✅ Full | Word highlight colors mapped |
| Font size | ✅ Full | Half-points → px |
| Font family | ✅ Full | |
| Bullet list | ✅ Full | Single level |
| Ordered list | ✅ Full | Single level |
| Images | ✅ Full | Embedded images extracted |

### Phase 2 (Enhanced)

| Element | Support | Notes |
|---------|---------|-------|
| Nested lists | ✅ Full | Multi-level bullets/numbers |
| Hyperlinks | ✅ Full | URL and text preserved |
| Horizontal rule | ⚠️ Partial | Basic support |
| Code blocks | ❌ Skip | Not common in Word docs |
| Tables | ❌ Skip | Complex, future enhancement |

### Not Supported (Future)

| Element | Reason |
|---------|--------|
| Tables | Requires table extension in Tiptap |
| Headers/Footers | Document-level metadata |
| Footnotes | Would need custom extension |
| Comments | Would need annotation system |
| Track changes | Out of scope |
| Embedded objects | Charts, SmartArt too complex |

---

## Error Handling

### Error Types

```rust
#[derive(Error, Debug)]
pub enum DocxImportError {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("File too large (max 10MB): {0}")]
    FileTooLarge(String),

    #[error("Invalid DOCX format: {0}")]
    InvalidFormat(String),

    #[error("Corrupted DOCX file: {0}")]
    CorruptedFile(String),

    #[error("Unsupported DOCX version: {0}")]
    UnsupportedVersion(String),

    #[error("Image extraction failed: {0}")]
    ImageExtractionFailed(String),

    #[error("IO error: {0}")]
    IoError(String),
}
```

### Warning Types

```rust
pub enum ImportWarningCode {
    UnsupportedTable,
    UnsupportedFootnote,
    UnsupportedComment,
    UnsupportedHeaderFooter,
    ImageConversionFailed,
    UnknownElement,
}
```

---

## File Structure (Actual Implementation)

```
apps/desktop/src-tauri/src/
├── services/
│   ├── mod.rs                    # exports docx_import
│   └── docx_import.rs            # ~850 lines: parser + converter + types
├── commands/
│   └── import.rs                 # DOCX commands added
├── menu.rs                       # "Import Word Document..." menu item
└── lib.rs                        # Commands registered

apps/desktop/src/
├── lib/
│   ├── import.ts                 # DOCX types + client methods
│   └── components/
│       └── DocxImportDialog.svelte  # NEW: Dedicated DOCX import dialog
└── App.svelte                    # Menu listener + dialog integration
```

---

## Dependencies

### Cargo.toml Additions (Actual)

```toml
[dependencies]
# DOCX import dependencies (added for import)
zip = "2.2"          # Extract DOCX archive
quick-xml = "0.37"   # Parse Word XML (document.xml, relationships)

# docx-rs is kept for export only
docx-rs = "0.4"
```

Note: `docx-rs` was not used for import because it's primarily a writing library. We parse DOCX directly using `zip` + `quick-xml` for full control over the XML structure.

---

## Progress Milestones

### Milestone 1: Basic Text Import ✅ COMPLETE
- [x] Parse DOCX structure (zip + quick-xml)
- [x] Convert paragraphs to Tiptap
- [x] Handle basic formatting (bold, italic)
- [x] Tauri command working

### Milestone 2: Full Formatting ✅ COMPLETE
- [x] Headings via styles (H1-H6)
- [x] Text color and highlights
- [x] Font size and family
- [x] Text alignment

### Milestone 3: Lists ✅ COMPLETE
- [x] Bullet lists
- [x] Numbered lists
- [x] Nested lists (via numPr level tracking)

### Milestone 4: Images ✅ COMPLETE
- [x] Extract embedded images from word/media/
- [x] Save to workspace .midlight/images/
- [x] Reference in Tiptap JSON via image nodes

### Milestone 5: Integration ✅ COMPLETE
- [x] Created DocxImportDialog.svelte UI
- [x] Analysis preview (paragraph/heading/image counts)
- [x] Error handling and warnings
- [x] Menu integration (File > Import Word Document...)

---

## Success Criteria ✅ ALL MET

DOCX import is complete when:

1. ✅ **Basic documents** import correctly with paragraphs and formatting
2. ✅ **Headings** are detected from Word styles (Heading 1-6)
3. ✅ **Lists** (bullet and numbered) preserve structure
4. ✅ **Images** are extracted and embedded properly
5. ✅ **Warnings** are shown for unsupported elements (tables converted to text)
6. ✅ **DocxImportDialog** provides dedicated DOCX import flow
7. ✅ **Compilation** passes for both Rust and TypeScript

---

## References

- [docx-rs documentation](https://docs.rs/docx-rs)
- [Office Open XML specification](http://officeopenxml.com/)
- [Tiptap JSON format](https://tiptap.dev/guide/output#option-1-json)
- Existing `docx_export.rs` implementation in this codebase
