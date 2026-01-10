// DOCX Import Service
// Converts Microsoft Word documents (.docx) to Tiptap JSON format
//
// DOCX files are ZIP archives containing XML files:
// - word/document.xml - Main document content
// - word/media/ - Embedded images
// - word/styles.xml - Style definitions
// - word/_rels/document.xml.rels - Relationships (image references)

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use zip::ZipArchive;

use crate::services::docx_export::{normalize_color_to_hex, TiptapDocument, TiptapMark, TiptapNode};
use crate::services::import_security::ImportConfig;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocxImportResult {
    pub tiptap_json: serde_json::Value,
    pub images: Vec<ExtractedImage>,
    pub warnings: Vec<ImportWarning>,
    pub stats: ImportStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedImage {
    pub id: String,
    pub data: Vec<u8>,
    pub content_type: String,
    pub original_name: String,
    pub rel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportStats {
    pub paragraph_count: usize,
    pub heading_count: usize,
    pub list_count: usize,
    pub image_count: usize,
    pub table_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocxAnalysis {
    pub file_name: String,
    pub file_size: u64,
    pub paragraph_count: usize,
    pub heading_count: usize,
    pub image_count: usize,
    pub table_count: usize,
    pub has_tables: bool,
    pub warnings: Vec<String>,
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum DocxImportError {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("File too large (max 10MB): {0}")]
    FileTooLarge(String),

    #[error("Invalid DOCX format: {0}")]
    InvalidFormat(String),

    #[error("XML parsing error: {0}")]
    XmlParse(String),

    #[error("ZIP error: {0}")]
    ZipError(String),

    #[error("IO error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for DocxImportError {
    fn from(err: std::io::Error) -> Self {
        DocxImportError::IoError(err.to_string())
    }
}

impl From<zip::result::ZipError> for DocxImportError {
    fn from(err: zip::result::ZipError) -> Self {
        DocxImportError::ZipError(err.to_string())
    }
}

impl From<quick_xml::Error> for DocxImportError {
    fn from(err: quick_xml::Error) -> Self {
        DocxImportError::XmlParse(err.to_string())
    }
}

// ============================================================================
// Internal Parsing State
// ============================================================================

#[derive(Debug, Clone, Default)]
struct RunProperties {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<String>,
    highlight: Option<String>,
    font_size: Option<String>,
    font_family: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ParagraphProperties {
    style_id: Option<String>,
    alignment: Option<String>,
    numbering_level: Option<i32>,
    numbering_id: Option<i32>,
}

#[derive(Debug, Clone)]
struct TextRun {
    text: String,
    props: RunProperties,
}

#[derive(Debug, Clone)]
struct ParsedParagraph {
    runs: Vec<TextRun>,
    props: ParagraphProperties,
    images: Vec<String>, // rId references
}

// ============================================================================
// Main Import Function
// ============================================================================

/// Parse a DOCX file and convert to Tiptap JSON
pub fn import_docx(file_path: &Path) -> Result<DocxImportResult, DocxImportError> {
    // Validate file exists
    if !file_path.exists() {
        return Err(DocxImportError::FileNotFound(
            file_path.to_string_lossy().to_string(),
        ));
    }

    // Check file size
    let metadata = std::fs::metadata(file_path)?;
    if metadata.len() > ImportConfig::MAX_CONTENT_SIZE as u64 {
        return Err(DocxImportError::FileTooLarge(
            file_path.to_string_lossy().to_string(),
        ));
    }

    // Validate extension
    if !file_path
        .extension()
        .map_or(false, |ext| ext.eq_ignore_ascii_case("docx"))
    {
        return Err(DocxImportError::InvalidFormat(
            "File must have .docx extension".to_string(),
        ));
    }

    // Open ZIP archive
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;

    // Extract relationships (for image references)
    let relationships = parse_relationships(&mut archive)?;

    // Extract images
    let images = extract_images(&mut archive, &relationships)?;

    // Build image ID map (rId -> our image ID)
    let image_id_map: HashMap<String, String> = images
        .iter()
        .map(|img| (img.rel_id.clone(), img.id.clone()))
        .collect();

    // Parse document.xml
    let mut warnings = Vec::new();
    let (paragraphs, stats) = parse_document_xml(&mut archive, &mut warnings)?;

    // Convert to Tiptap JSON
    let tiptap_doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

    let tiptap_json =
        serde_json::to_value(&tiptap_doc).map_err(|e| DocxImportError::XmlParse(e.to_string()))?;

    Ok(DocxImportResult {
        tiptap_json,
        images,
        warnings,
        stats,
    })
}

/// Analyze a DOCX file without full conversion
pub fn analyze_docx(file_path: &Path) -> Result<DocxAnalysis, DocxImportError> {
    // Validate file exists
    if !file_path.exists() {
        return Err(DocxImportError::FileNotFound(
            file_path.to_string_lossy().to_string(),
        ));
    }

    let metadata = std::fs::metadata(file_path)?;

    // Quick parse for analysis
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;

    let relationships = parse_relationships(&mut archive)?;
    let mut warnings = Vec::new();
    let (_, stats) = parse_document_xml(&mut archive, &mut warnings)?;

    // Count images from relationships
    let image_count = relationships
        .values()
        .filter(|target| {
            target.ends_with(".png")
                || target.ends_with(".jpg")
                || target.ends_with(".jpeg")
                || target.ends_with(".gif")
        })
        .count();

    Ok(DocxAnalysis {
        file_name: file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        file_size: metadata.len(),
        paragraph_count: stats.paragraph_count,
        heading_count: stats.heading_count,
        image_count,
        table_count: stats.table_count,
        has_tables: stats.table_count > 0,
        warnings: warnings.iter().map(|w| w.message.clone()).collect(),
    })
}

// ============================================================================
// ZIP/XML Parsing
// ============================================================================

/// Parse word/_rels/document.xml.rels to get image relationships
fn parse_relationships(
    archive: &mut ZipArchive<BufReader<File>>,
) -> Result<HashMap<String, String>, DocxImportError> {
    let mut relationships = HashMap::new();

    let rels_path = "word/_rels/document.xml.rels";
    let rels_file = match archive.by_name(rels_path) {
        Ok(f) => f,
        Err(_) => return Ok(relationships), // No relationships file is OK
    };

    let mut reader = Reader::from_reader(BufReader::new(rels_file));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) if e.name().as_ref() == b"Relationship" => {
                let mut id = None;
                let mut target = None;
                let mut rel_type = None;

                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"Id" => id = Some(String::from_utf8_lossy(&attr.value).to_string()),
                        b"Target" => target = Some(String::from_utf8_lossy(&attr.value).to_string()),
                        b"Type" => rel_type = Some(String::from_utf8_lossy(&attr.value).to_string()),
                        _ => {}
                    }
                }

                // Only include image relationships
                if let (Some(id), Some(target), Some(rtype)) = (id, target, rel_type) {
                    if rtype.contains("image") {
                        relationships.insert(id, target);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DocxImportError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(relationships)
}

/// Extract images from word/media/ folder
fn extract_images(
    archive: &mut ZipArchive<BufReader<File>>,
    relationships: &HashMap<String, String>,
) -> Result<Vec<ExtractedImage>, DocxImportError> {
    let mut images = Vec::new();

    for (rel_id, target) in relationships {
        // Target is like "media/image1.png"
        let media_path = if target.starts_with("media/") {
            format!("word/{}", target)
        } else {
            format!("word/media/{}", target)
        };

        let mut image_file = match archive.by_name(&media_path) {
            Ok(f) => f,
            Err(_) => continue, // Skip missing images
        };

        let mut data = Vec::new();
        image_file.read_to_end(&mut data)?;

        // Generate ID from content hash
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = format!("{:x}", hasher.finalize());
        let id = format!("img-{}", &hash[..12]);

        // Detect content type
        let content_type = detect_image_type(&data);

        // Get original filename
        let original_name = target.split('/').last().unwrap_or("image").to_string();

        images.push(ExtractedImage {
            id,
            data,
            content_type,
            original_name,
            rel_id: rel_id.clone(),
        });
    }

    Ok(images)
}

/// Detect image content type from magic bytes
fn detect_image_type(data: &[u8]) -> String {
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png".to_string()
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg".to_string()
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        "image/gif".to_string()
    } else if data.len() > 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        "image/webp".to_string()
    } else {
        "image/png".to_string() // Default
    }
}

/// Parse word/document.xml
fn parse_document_xml(
    archive: &mut ZipArchive<BufReader<File>>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(Vec<ParsedParagraph>, ImportStats), DocxImportError> {
    let doc_file = archive.by_name("word/document.xml").map_err(|_| {
        DocxImportError::InvalidFormat("Missing word/document.xml".to_string())
    })?;

    let mut reader = Reader::from_reader(BufReader::new(doc_file));
    reader.config_mut().trim_text(true);

    let mut paragraphs = Vec::new();
    let mut stats = ImportStats::default();
    let mut buf = Vec::new();

    // Parsing state
    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_text = false;
    let mut in_run_props = false;
    let mut in_para_props = false;
    let mut in_table = false;
    let mut in_drawing = false;

    let mut current_paragraph = ParsedParagraph {
        runs: Vec::new(),
        props: ParagraphProperties::default(),
        images: Vec::new(),
    };
    let mut current_run_props = RunProperties::default();
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"w:p" => {
                        in_paragraph = true;
                        current_paragraph = ParsedParagraph {
                            runs: Vec::new(),
                            props: ParagraphProperties::default(),
                            images: Vec::new(),
                        };
                    }
                    b"w:r" => {
                        in_run = true;
                        current_run_props = RunProperties::default();
                        current_text.clear();
                    }
                    b"w:t" => {
                        in_text = true;
                    }
                    b"w:rPr" => {
                        in_run_props = true;
                    }
                    b"w:pPr" => {
                        in_para_props = true;
                    }
                    b"w:tbl" => {
                        in_table = true;
                        stats.table_count += 1;
                        warnings.push(ImportWarning {
                            code: "unsupported_table".to_string(),
                            message: "Tables are not fully supported and may be simplified"
                                .to_string(),
                        });
                    }
                    b"w:drawing" | b"wp:inline" | b"wp:anchor" => {
                        in_drawing = true;
                    }
                    _ => {}
                }

                // Handle run properties
                if in_run_props {
                    handle_run_property(e, &mut current_run_props);
                }

                // Handle paragraph properties
                if in_para_props {
                    handle_para_property(e, &mut current_paragraph.props);
                }

                // Handle drawing/image references
                if in_drawing {
                    if let Some(rel_id) = extract_image_rel_id(e) {
                        current_paragraph.images.push(rel_id);
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name();

                // Handle empty run properties
                if in_run_props {
                    handle_run_property(e, &mut current_run_props);
                }

                // Handle empty paragraph properties
                if in_para_props {
                    handle_para_property(e, &mut current_paragraph.props);
                }

                // Handle image references in drawings
                if in_drawing || in_paragraph {
                    if let Some(rel_id) = extract_image_rel_id(e) {
                        current_paragraph.images.push(rel_id);
                    }
                }

                // Handle break elements
                if name.as_ref() == b"w:br" {
                    current_text.push('\n');
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text {
                    current_text.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"w:p" => {
                        in_paragraph = false;
                        if !in_table {
                            // Update stats based on style
                            if is_heading_style(&current_paragraph.props.style_id) {
                                stats.heading_count += 1;
                            } else if current_paragraph.props.numbering_id.is_some() {
                                stats.list_count += 1;
                            } else {
                                stats.paragraph_count += 1;
                            }
                            paragraphs.push(current_paragraph.clone());
                        }
                    }
                    b"w:r" => {
                        in_run = false;
                        if !current_text.is_empty() {
                            current_paragraph.runs.push(TextRun {
                                text: current_text.clone(),
                                props: current_run_props.clone(),
                            });
                        }
                        current_text.clear();
                    }
                    b"w:t" => {
                        in_text = false;
                    }
                    b"w:rPr" => {
                        in_run_props = false;
                    }
                    b"w:pPr" => {
                        in_para_props = false;
                    }
                    b"w:tbl" => {
                        in_table = false;
                    }
                    b"w:drawing" | b"wp:inline" | b"wp:anchor" => {
                        in_drawing = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DocxImportError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    stats.image_count = paragraphs.iter().map(|p| p.images.len()).sum();

    Ok((paragraphs, stats))
}

/// Handle run property elements
fn handle_run_property(e: &BytesStart, props: &mut RunProperties) {
    let name = e.name();
    match name.as_ref() {
        b"w:b" => {
            // Bold - check for w:val="false" or w:val="0"
            let is_false = e.attributes().flatten().any(|a| {
                a.key.as_ref() == b"w:val"
                    && (a.value.as_ref() == b"false" || a.value.as_ref() == b"0")
            });
            props.bold = !is_false;
        }
        b"w:i" => {
            let is_false = e.attributes().flatten().any(|a| {
                a.key.as_ref() == b"w:val"
                    && (a.value.as_ref() == b"false" || a.value.as_ref() == b"0")
            });
            props.italic = !is_false;
        }
        b"w:u" => {
            props.underline = true;
        }
        b"w:strike" => {
            props.strike = true;
        }
        b"w:color" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:val" {
                    let val = String::from_utf8_lossy(&attr.value).to_string();
                    if val != "auto" {
                        props.color = Some(format!("#{}", val));
                    }
                }
            }
        }
        b"w:highlight" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:val" {
                    let val = String::from_utf8_lossy(&attr.value).to_string();
                    props.highlight = Some(highlight_name_to_hex(&val));
                }
            }
        }
        b"w:sz" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:val" {
                    // Size is in half-points, convert to px (divide by 2)
                    if let Ok(half_points) = String::from_utf8_lossy(&attr.value).parse::<u32>() {
                        let px = half_points / 2;
                        props.font_size = Some(format!("{}px", px));
                    }
                }
            }
        }
        b"w:rFonts" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:ascii" {
                    props.font_family = Some(String::from_utf8_lossy(&attr.value).to_string());
                    break;
                }
            }
        }
        _ => {}
    }
}

/// Handle paragraph property elements
fn handle_para_property(e: &BytesStart, props: &mut ParagraphProperties) {
    let name = e.name();
    match name.as_ref() {
        b"w:pStyle" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:val" {
                    props.style_id = Some(String::from_utf8_lossy(&attr.value).to_string());
                }
            }
        }
        b"w:jc" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:val" {
                    let val = String::from_utf8_lossy(&attr.value).to_string();
                    props.alignment = Some(match val.as_str() {
                        "center" => "center".to_string(),
                        "right" | "end" => "right".to_string(),
                        "both" | "distribute" => "justify".to_string(),
                        _ => "left".to_string(),
                    });
                }
            }
        }
        b"w:numPr" => {
            // Numbering property container - will be handled by child elements
        }
        b"w:ilvl" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:val" {
                    if let Ok(level) = String::from_utf8_lossy(&attr.value).parse::<i32>() {
                        props.numbering_level = Some(level);
                    }
                }
            }
        }
        b"w:numId" => {
            for attr in e.attributes().flatten() {
                if attr.key.as_ref() == b"w:val" {
                    if let Ok(id) = String::from_utf8_lossy(&attr.value).parse::<i32>() {
                        props.numbering_id = Some(id);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Extract image relationship ID from drawing elements
fn extract_image_rel_id(e: &BytesStart) -> Option<String> {
    let name = e.name();
    if name.as_ref() == b"a:blip" {
        for attr in e.attributes().flatten() {
            if attr.key.as_ref() == b"r:embed" {
                return Some(String::from_utf8_lossy(&attr.value).to_string());
            }
        }
    }
    None
}

/// Check if style ID is a heading style
fn is_heading_style(style_id: &Option<String>) -> bool {
    match style_id {
        Some(id) => {
            let lower = id.to_lowercase();
            lower.starts_with("heading")
                || lower == "title"
                || lower == "subtitle"
                || lower.starts_with("h")
                    && lower.len() == 2
                    && lower.chars().nth(1).map_or(false, |c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Convert Word highlight name to hex color
fn highlight_name_to_hex(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "yellow" => "#ffff00".to_string(),
        "green" => "#00ff00".to_string(),
        "cyan" => "#00ffff".to_string(),
        "magenta" => "#ff00ff".to_string(),
        "blue" => "#0000ff".to_string(),
        "red" => "#ff0000".to_string(),
        "darkblue" | "darkBlue" => "#000080".to_string(),
        "darkcyan" | "darkCyan" => "#008080".to_string(),
        "darkgreen" | "darkGreen" => "#008000".to_string(),
        "darkmagenta" | "darkMagenta" => "#800080".to_string(),
        "darkred" | "darkRed" => "#800000".to_string(),
        "darkyellow" | "darkYellow" => "#808000".to_string(),
        "darkgray" | "darkGray" => "#808080".to_string(),
        "lightgray" | "lightGray" => "#c0c0c0".to_string(),
        "black" => "#000000".to_string(),
        "white" => "#ffffff".to_string(),
        _ => "#ffff00".to_string(), // Default to yellow
    }
}

// ============================================================================
// Tiptap Conversion
// ============================================================================

/// Convert parsed paragraphs to Tiptap document
fn convert_to_tiptap(
    paragraphs: Vec<ParsedParagraph>,
    image_id_map: &HashMap<String, String>,
    _warnings: &mut Vec<ImportWarning>,
) -> TiptapDocument {
    let mut content: Vec<TiptapNode> = Vec::new();
    let mut current_list_type: Option<&str> = None;
    let mut list_items: Vec<TiptapNode> = Vec::new();

    for para in paragraphs {
        // Handle images first
        for rel_id in &para.images {
            if let Some(image_id) = image_id_map.get(rel_id) {
                content.push(create_image_node(image_id));
            }
        }

        // Skip empty paragraphs with no content and no images
        if para.runs.is_empty() && para.images.is_empty() {
            // Flush any pending list
            if let Some(list_type) = current_list_type {
                content.push(create_list_node(list_type, std::mem::take(&mut list_items)));
                current_list_type = None;
            }
            // Add empty paragraph
            content.push(TiptapNode {
                node_type: "paragraph".to_string(),
                content: Vec::new(),
                text: None,
                marks: Vec::new(),
                attrs: None,
            });
            continue;
        }

        // Determine node type
        let heading_level = get_heading_level(&para.props.style_id);
        let is_list = para.props.numbering_id.is_some();
        let list_type = if is_list {
            // Determine bullet vs ordered from numbering ID
            // Odd IDs are typically bullet, even are ordered (convention)
            if para.props.numbering_id.unwrap_or(1) % 2 == 1 {
                Some("bulletList")
            } else {
                Some("orderedList")
            }
        } else {
            None
        };

        // Convert runs to text nodes
        let text_content = convert_runs_to_tiptap(&para.runs);

        // Build the node
        if let Some(level) = heading_level {
            // Flush any pending list
            if let Some(lt) = current_list_type {
                content.push(create_list_node(lt, std::mem::take(&mut list_items)));
                current_list_type = None;
            }

            content.push(create_heading_node(level, text_content, &para.props));
        } else if let Some(lt) = list_type {
            // Check if continuing same list type
            if current_list_type != Some(lt) {
                // Flush previous list
                if let Some(prev_lt) = current_list_type {
                    content.push(create_list_node(prev_lt, std::mem::take(&mut list_items)));
                }
                current_list_type = Some(lt);
            }

            // Add list item
            list_items.push(create_list_item_node(text_content, &para.props));
        } else {
            // Regular paragraph - flush any pending list
            if let Some(lt) = current_list_type {
                content.push(create_list_node(lt, std::mem::take(&mut list_items)));
                current_list_type = None;
            }

            content.push(create_paragraph_node(text_content, &para.props));
        }
    }

    // Flush any remaining list
    if let Some(lt) = current_list_type {
        content.push(create_list_node(lt, list_items));
    }

    TiptapDocument {
        doc_type: "doc".to_string(),
        content,
    }
}

/// Get heading level from style ID
fn get_heading_level(style_id: &Option<String>) -> Option<u32> {
    let id = style_id.as_ref()?;
    let lower = id.to_lowercase();

    if lower == "title" {
        return Some(1);
    }
    if lower == "subtitle" {
        return Some(2);
    }

    // Check for "Heading1", "heading1", "H1", etc.
    if lower.starts_with("heading") {
        return lower[7..].parse().ok();
    }
    if lower.starts_with("h") && lower.len() == 2 {
        return lower[1..].parse().ok();
    }

    None
}

/// Convert runs to Tiptap text nodes
fn convert_runs_to_tiptap(runs: &[TextRun]) -> Vec<TiptapNode> {
    runs.iter()
        .filter(|r| !r.text.is_empty())
        .map(|run| {
            let mut marks = Vec::new();

            // Boolean marks
            if run.props.bold {
                marks.push(TiptapMark {
                    mark_type: "bold".to_string(),
                    attrs: None,
                });
            }
            if run.props.italic {
                marks.push(TiptapMark {
                    mark_type: "italic".to_string(),
                    attrs: None,
                });
            }
            if run.props.underline {
                marks.push(TiptapMark {
                    mark_type: "underline".to_string(),
                    attrs: None,
                });
            }
            if run.props.strike {
                marks.push(TiptapMark {
                    mark_type: "strike".to_string(),
                    attrs: None,
                });
            }

            // textStyle mark for color, fontSize, fontFamily
            let mut text_style_attrs = serde_json::Map::new();
            if let Some(ref color) = run.props.color {
                if let Some(normalized) = normalize_color_to_hex(Some(color)) {
                    text_style_attrs.insert("color".to_string(), serde_json::json!(format!("#{}", normalized)));
                }
            }
            if let Some(ref size) = run.props.font_size {
                text_style_attrs.insert("fontSize".to_string(), serde_json::json!(size));
            }
            if let Some(ref family) = run.props.font_family {
                text_style_attrs.insert("fontFamily".to_string(), serde_json::json!(family));
            }
            if !text_style_attrs.is_empty() {
                marks.push(TiptapMark {
                    mark_type: "textStyle".to_string(),
                    attrs: Some(serde_json::Value::Object(text_style_attrs)),
                });
            }

            // Highlight mark
            if let Some(ref highlight) = run.props.highlight {
                marks.push(TiptapMark {
                    mark_type: "highlight".to_string(),
                    attrs: Some(serde_json::json!({ "color": highlight })),
                });
            }

            TiptapNode {
                node_type: "text".to_string(),
                content: Vec::new(),
                text: Some(run.text.clone()),
                marks,
                attrs: None,
            }
        })
        .collect()
}

/// Create a paragraph node
fn create_paragraph_node(content: Vec<TiptapNode>, props: &ParagraphProperties) -> TiptapNode {
    let attrs = if let Some(ref align) = props.alignment {
        Some(serde_json::json!({ "textAlign": align }))
    } else {
        None
    };

    TiptapNode {
        node_type: "paragraph".to_string(),
        content,
        text: None,
        marks: Vec::new(),
        attrs,
    }
}

/// Create a heading node
fn create_heading_node(
    level: u32,
    content: Vec<TiptapNode>,
    props: &ParagraphProperties,
) -> TiptapNode {
    let mut attrs = serde_json::json!({ "level": level });
    if let Some(ref align) = props.alignment {
        attrs["textAlign"] = serde_json::json!(align);
    }

    TiptapNode {
        node_type: "heading".to_string(),
        content,
        text: None,
        marks: Vec::new(),
        attrs: Some(attrs),
    }
}

/// Create a list node (bulletList or orderedList)
fn create_list_node(list_type: &str, items: Vec<TiptapNode>) -> TiptapNode {
    TiptapNode {
        node_type: list_type.to_string(),
        content: items,
        text: None,
        marks: Vec::new(),
        attrs: None,
    }
}

/// Create a list item node
fn create_list_item_node(content: Vec<TiptapNode>, props: &ParagraphProperties) -> TiptapNode {
    // List item contains a paragraph
    let para = create_paragraph_node(content, props);

    TiptapNode {
        node_type: "listItem".to_string(),
        content: vec![para],
        text: None,
        marks: Vec::new(),
        attrs: None,
    }
}

/// Create an image node
fn create_image_node(image_id: &str) -> TiptapNode {
    TiptapNode {
        node_type: "image".to_string(),
        content: Vec::new(),
        text: None,
        marks: Vec::new(),
        attrs: Some(serde_json::json!({
            "src": format!("midlight://{}", image_id),
            "alt": "",
            "title": ""
        })),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_image_type() {
        // PNG
        let png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_image_type(&png), "image/png");

        // JPEG
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_image_type(&jpeg), "image/jpeg");

        // GIF
        let gif = b"GIF89a".to_vec();
        assert_eq!(detect_image_type(&gif), "image/gif");
    }

    #[test]
    fn test_highlight_name_to_hex() {
        assert_eq!(highlight_name_to_hex("yellow"), "#ffff00");
        assert_eq!(highlight_name_to_hex("red"), "#ff0000");
        assert_eq!(highlight_name_to_hex("darkBlue"), "#000080");
    }

    #[test]
    fn test_get_heading_level() {
        assert_eq!(get_heading_level(&Some("Heading1".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("heading2".to_string())), Some(2));
        assert_eq!(get_heading_level(&Some("Title".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("Subtitle".to_string())), Some(2));
        assert_eq!(get_heading_level(&Some("Normal".to_string())), None);
        assert_eq!(get_heading_level(&None), None);
    }

    #[test]
    fn test_is_heading_style() {
        assert!(is_heading_style(&Some("Heading1".to_string())));
        assert!(is_heading_style(&Some("heading2".to_string())));
        assert!(is_heading_style(&Some("Title".to_string())));
        assert!(!is_heading_style(&Some("Normal".to_string())));
        assert!(!is_heading_style(&None));
    }
}
