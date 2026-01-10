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

use crate::services::docx_export::{
    normalize_color_to_hex, TiptapDocument, TiptapMark, TiptapNode,
};
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
        .is_some_and(|ext| ext.eq_ignore_ascii_case("docx"))
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
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e))
                if e.name().as_ref() == b"Relationship" =>
            {
                let mut id = None;
                let mut target = None;
                let mut rel_type = None;

                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"Id" => id = Some(String::from_utf8_lossy(&attr.value).to_string()),
                        b"Target" => {
                            target = Some(String::from_utf8_lossy(&attr.value).to_string())
                        }
                        b"Type" => {
                            rel_type = Some(String::from_utf8_lossy(&attr.value).to_string())
                        }
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
        let original_name = target.split('/').next_back().unwrap_or("image").to_string();

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
    let doc_file = archive
        .by_name("word/document.xml")
        .map_err(|_| DocxImportError::InvalidFormat("Missing word/document.xml".to_string()))?;

    let mut reader = Reader::from_reader(BufReader::new(doc_file));
    reader.config_mut().trim_text(true);

    let mut paragraphs = Vec::new();
    let mut stats = ImportStats::default();
    let mut buf = Vec::new();

    // Parsing state
    let mut in_paragraph = false;
    let mut _in_run = false;
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
                        _in_run = true;
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
                        _in_run = false;
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
                    && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
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
#[allow(clippy::ptr_arg)]
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
                    text_style_attrs.insert(
                        "color".to_string(),
                        serde_json::json!(format!("#{}", normalized)),
                    );
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
    let attrs = props
        .alignment
        .as_ref()
        .map(|align| serde_json::json!({ "textAlign": align }));

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

    // ============================================================================
    // Image Type Detection Tests
    // ============================================================================

    #[test]
    fn test_detect_image_type_png() {
        let png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_image_type(&png), "image/png");
    }

    #[test]
    fn test_detect_image_type_jpeg() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_image_type(&jpeg), "image/jpeg");

        // JPEG with different marker
        let jpeg2 = vec![0xFF, 0xD8, 0xFF, 0xE1];
        assert_eq!(detect_image_type(&jpeg2), "image/jpeg");
    }

    #[test]
    fn test_detect_image_type_gif() {
        let gif89a = b"GIF89a".to_vec();
        assert_eq!(detect_image_type(&gif89a), "image/gif");

        let gif87a = b"GIF87a".to_vec();
        assert_eq!(detect_image_type(&gif87a), "image/gif");
    }

    #[test]
    fn test_detect_image_type_webp() {
        // WEBP magic: "RIFF" + size + "WEBP" + extra bytes (must be > 12 bytes)
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // size placeholder
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(&[0x00]); // Extra byte to make it > 12 bytes
        assert_eq!(detect_image_type(&webp), "image/webp");
    }

    #[test]
    fn test_detect_image_type_unknown() {
        let unknown = vec![0x00, 0x01, 0x02, 0x03];
        assert_eq!(detect_image_type(&unknown), "image/png"); // Default fallback

        let empty = vec![];
        assert_eq!(detect_image_type(&empty), "image/png");
    }

    #[test]
    fn test_detect_image_type_too_short_for_webp() {
        // Less than 12 bytes - can't be WEBP
        let short = b"RIFF1234".to_vec();
        assert_eq!(detect_image_type(&short), "image/png");
    }

    // ============================================================================
    // Highlight Color Tests
    // ============================================================================

    #[test]
    fn test_highlight_name_to_hex_basic_colors() {
        assert_eq!(highlight_name_to_hex("yellow"), "#ffff00");
        assert_eq!(highlight_name_to_hex("green"), "#00ff00");
        assert_eq!(highlight_name_to_hex("cyan"), "#00ffff");
        assert_eq!(highlight_name_to_hex("magenta"), "#ff00ff");
        assert_eq!(highlight_name_to_hex("blue"), "#0000ff");
        assert_eq!(highlight_name_to_hex("red"), "#ff0000");
    }

    #[test]
    fn test_highlight_name_to_hex_dark_colors() {
        assert_eq!(highlight_name_to_hex("darkBlue"), "#000080");
        assert_eq!(highlight_name_to_hex("darkblue"), "#000080");
        assert_eq!(highlight_name_to_hex("darkCyan"), "#008080");
        assert_eq!(highlight_name_to_hex("darkcyan"), "#008080");
        assert_eq!(highlight_name_to_hex("darkGreen"), "#008000");
        assert_eq!(highlight_name_to_hex("darkgreen"), "#008000");
        assert_eq!(highlight_name_to_hex("darkMagenta"), "#800080");
        assert_eq!(highlight_name_to_hex("darkmagenta"), "#800080");
        assert_eq!(highlight_name_to_hex("darkRed"), "#800000");
        assert_eq!(highlight_name_to_hex("darkred"), "#800000");
        assert_eq!(highlight_name_to_hex("darkYellow"), "#808000");
        assert_eq!(highlight_name_to_hex("darkyellow"), "#808000");
    }

    #[test]
    fn test_highlight_name_to_hex_gray_colors() {
        assert_eq!(highlight_name_to_hex("darkGray"), "#808080");
        assert_eq!(highlight_name_to_hex("darkgray"), "#808080");
        assert_eq!(highlight_name_to_hex("lightGray"), "#c0c0c0");
        assert_eq!(highlight_name_to_hex("lightgray"), "#c0c0c0");
    }

    #[test]
    fn test_highlight_name_to_hex_bw() {
        assert_eq!(highlight_name_to_hex("black"), "#000000");
        assert_eq!(highlight_name_to_hex("white"), "#ffffff");
    }

    #[test]
    fn test_highlight_name_to_hex_unknown() {
        assert_eq!(highlight_name_to_hex("unknown"), "#ffff00"); // Default to yellow
        assert_eq!(highlight_name_to_hex(""), "#ffff00");
        assert_eq!(highlight_name_to_hex("notacolor"), "#ffff00");
    }

    #[test]
    fn test_highlight_name_to_hex_case_insensitive() {
        assert_eq!(highlight_name_to_hex("YELLOW"), "#ffff00");
        assert_eq!(highlight_name_to_hex("Yellow"), "#ffff00");
        assert_eq!(highlight_name_to_hex("RED"), "#ff0000");
    }

    // ============================================================================
    // Heading Level Tests
    // ============================================================================

    #[test]
    fn test_get_heading_level_heading_prefix() {
        assert_eq!(get_heading_level(&Some("Heading1".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("Heading2".to_string())), Some(2));
        assert_eq!(get_heading_level(&Some("Heading3".to_string())), Some(3));
        assert_eq!(get_heading_level(&Some("heading1".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("heading2".to_string())), Some(2));
        assert_eq!(get_heading_level(&Some("HEADING1".to_string())), Some(1));
    }

    #[test]
    fn test_get_heading_level_h_prefix() {
        assert_eq!(get_heading_level(&Some("H1".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("H2".to_string())), Some(2));
        assert_eq!(get_heading_level(&Some("h1".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("h3".to_string())), Some(3));
    }

    #[test]
    fn test_get_heading_level_title_subtitle() {
        assert_eq!(get_heading_level(&Some("Title".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("title".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("TITLE".to_string())), Some(1));
        assert_eq!(get_heading_level(&Some("Subtitle".to_string())), Some(2));
        assert_eq!(get_heading_level(&Some("subtitle".to_string())), Some(2));
    }

    #[test]
    fn test_get_heading_level_non_heading() {
        assert_eq!(get_heading_level(&Some("Normal".to_string())), None);
        assert_eq!(get_heading_level(&Some("BodyText".to_string())), None);
        assert_eq!(get_heading_level(&Some("ListParagraph".to_string())), None);
        assert_eq!(get_heading_level(&None), None);
    }

    #[test]
    fn test_get_heading_level_invalid_h_style() {
        // H prefix but not exactly 2 chars or not followed by digit
        assert_eq!(get_heading_level(&Some("H10".to_string())), None);
        assert_eq!(get_heading_level(&Some("Ha".to_string())), None);
        assert_eq!(get_heading_level(&Some("H".to_string())), None);
    }

    // ============================================================================
    // Is Heading Style Tests
    // ============================================================================

    #[test]
    fn test_is_heading_style_heading_prefix() {
        assert!(is_heading_style(&Some("Heading1".to_string())));
        assert!(is_heading_style(&Some("Heading2".to_string())));
        assert!(is_heading_style(&Some("heading3".to_string())));
        assert!(is_heading_style(&Some("HeadingCustom".to_string())));
    }

    #[test]
    fn test_is_heading_style_title_subtitle() {
        assert!(is_heading_style(&Some("Title".to_string())));
        assert!(is_heading_style(&Some("title".to_string())));
        assert!(is_heading_style(&Some("Subtitle".to_string())));
        assert!(is_heading_style(&Some("subtitle".to_string())));
    }

    #[test]
    fn test_is_heading_style_h_prefix() {
        assert!(is_heading_style(&Some("H1".to_string())));
        assert!(is_heading_style(&Some("H2".to_string())));
        assert!(is_heading_style(&Some("h3".to_string())));
        assert!(is_heading_style(&Some("h9".to_string())));
    }

    #[test]
    fn test_is_heading_style_non_heading() {
        assert!(!is_heading_style(&Some("Normal".to_string())));
        assert!(!is_heading_style(&Some("BodyText".to_string())));
        assert!(!is_heading_style(&Some("ListParagraph".to_string())));
        assert!(!is_heading_style(&None));
    }

    #[test]
    fn test_is_heading_style_edge_cases() {
        // H prefix but invalid
        assert!(!is_heading_style(&Some("H10".to_string())));
        assert!(!is_heading_style(&Some("Ha".to_string())));
        assert!(!is_heading_style(&Some("H".to_string())));
        assert!(!is_heading_style(&Some("Headline".to_string()))); // Starts with h but isn't h#
    }

    // ============================================================================
    // Convert Runs to Tiptap Tests
    // ============================================================================

    #[test]
    fn test_convert_runs_to_tiptap_empty() {
        let runs: Vec<TextRun> = vec![];
        let result = convert_runs_to_tiptap(&runs);
        assert!(result.is_empty());
    }

    #[test]
    fn test_convert_runs_to_tiptap_plain_text() {
        let runs = vec![TextRun {
            text: "Hello world".to_string(),
            props: RunProperties::default(),
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].node_type, "text");
        assert_eq!(result[0].text, Some("Hello world".to_string()));
        assert!(result[0].marks.is_empty());
    }

    #[test]
    fn test_convert_runs_to_tiptap_bold() {
        let runs = vec![TextRun {
            text: "Bold text".to_string(),
            props: RunProperties {
                bold: true,
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 1);
        assert_eq!(result[0].marks[0].mark_type, "bold");
    }

    #[test]
    fn test_convert_runs_to_tiptap_italic() {
        let runs = vec![TextRun {
            text: "Italic text".to_string(),
            props: RunProperties {
                italic: true,
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 1);
        assert_eq!(result[0].marks[0].mark_type, "italic");
    }

    #[test]
    fn test_convert_runs_to_tiptap_underline() {
        let runs = vec![TextRun {
            text: "Underlined".to_string(),
            props: RunProperties {
                underline: true,
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 1);
        assert_eq!(result[0].marks[0].mark_type, "underline");
    }

    #[test]
    fn test_convert_runs_to_tiptap_strike() {
        let runs = vec![TextRun {
            text: "Strikethrough".to_string(),
            props: RunProperties {
                strike: true,
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 1);
        assert_eq!(result[0].marks[0].mark_type, "strike");
    }

    #[test]
    fn test_convert_runs_to_tiptap_multiple_marks() {
        let runs = vec![TextRun {
            text: "Bold and italic".to_string(),
            props: RunProperties {
                bold: true,
                italic: true,
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 2);
        let mark_types: Vec<&str> = result[0]
            .marks
            .iter()
            .map(|m| m.mark_type.as_str())
            .collect();
        assert!(mark_types.contains(&"bold"));
        assert!(mark_types.contains(&"italic"));
    }

    #[test]
    fn test_convert_runs_to_tiptap_with_color() {
        let runs = vec![TextRun {
            text: "Red text".to_string(),
            props: RunProperties {
                color: Some("#FF0000".to_string()),
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 1);
        assert_eq!(result[0].marks[0].mark_type, "textStyle");
        let attrs = result[0].marks[0].attrs.as_ref().unwrap();
        assert!(attrs.get("color").is_some());
    }

    #[test]
    fn test_convert_runs_to_tiptap_with_highlight() {
        let runs = vec![TextRun {
            text: "Highlighted".to_string(),
            props: RunProperties {
                highlight: Some("#FFFF00".to_string()),
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        let mark_types: Vec<&str> = result[0]
            .marks
            .iter()
            .map(|m| m.mark_type.as_str())
            .collect();
        assert!(mark_types.contains(&"highlight"));
    }

    #[test]
    fn test_convert_runs_to_tiptap_with_font_size() {
        let runs = vec![TextRun {
            text: "Large text".to_string(),
            props: RunProperties {
                font_size: Some("24px".to_string()),
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 1);
        assert_eq!(result[0].marks[0].mark_type, "textStyle");
        let attrs = result[0].marks[0].attrs.as_ref().unwrap();
        assert!(attrs.get("fontSize").is_some());
    }

    #[test]
    fn test_convert_runs_to_tiptap_with_font_family() {
        let runs = vec![TextRun {
            text: "Custom font".to_string(),
            props: RunProperties {
                font_family: Some("Arial".to_string()),
                ..Default::default()
            },
        }];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].marks.len(), 1);
        assert_eq!(result[0].marks[0].mark_type, "textStyle");
        let attrs = result[0].marks[0].attrs.as_ref().unwrap();
        assert!(attrs.get("fontFamily").is_some());
    }

    #[test]
    fn test_convert_runs_to_tiptap_multiple_runs() {
        let runs = vec![
            TextRun {
                text: "Normal ".to_string(),
                props: RunProperties::default(),
            },
            TextRun {
                text: "bold".to_string(),
                props: RunProperties {
                    bold: true,
                    ..Default::default()
                },
            },
            TextRun {
                text: " text".to_string(),
                props: RunProperties::default(),
            },
        ];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 3);
        assert!(result[0].marks.is_empty());
        assert_eq!(result[1].marks[0].mark_type, "bold");
        assert!(result[2].marks.is_empty());
    }

    #[test]
    fn test_convert_runs_to_tiptap_empty_text_filtered() {
        let runs = vec![
            TextRun {
                text: "".to_string(),
                props: RunProperties::default(),
            },
            TextRun {
                text: "actual text".to_string(),
                props: RunProperties::default(),
            },
        ];
        let result = convert_runs_to_tiptap(&runs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, Some("actual text".to_string()));
    }

    // ============================================================================
    // Node Creation Tests
    // ============================================================================

    #[test]
    fn test_create_paragraph_node_simple() {
        let content = vec![TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Hello".to_string()),
            marks: vec![],
            attrs: None,
        }];
        let props = ParagraphProperties::default();
        let node = create_paragraph_node(content, &props);

        assert_eq!(node.node_type, "paragraph");
        assert_eq!(node.content.len(), 1);
        assert!(node.attrs.is_none());
    }

    #[test]
    fn test_create_paragraph_node_with_alignment() {
        let content = vec![];
        let props = ParagraphProperties {
            alignment: Some("center".to_string()),
            ..Default::default()
        };
        let node = create_paragraph_node(content, &props);

        assert_eq!(node.node_type, "paragraph");
        let attrs = node.attrs.unwrap();
        assert_eq!(attrs["textAlign"], "center");
    }

    #[test]
    fn test_create_heading_node() {
        let content = vec![TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Heading".to_string()),
            marks: vec![],
            attrs: None,
        }];
        let props = ParagraphProperties::default();
        let node = create_heading_node(2, content, &props);

        assert_eq!(node.node_type, "heading");
        let attrs = node.attrs.unwrap();
        assert_eq!(attrs["level"], 2);
    }

    #[test]
    fn test_create_heading_node_with_alignment() {
        let content = vec![];
        let props = ParagraphProperties {
            alignment: Some("right".to_string()),
            ..Default::default()
        };
        let node = create_heading_node(1, content, &props);

        let attrs = node.attrs.unwrap();
        assert_eq!(attrs["level"], 1);
        assert_eq!(attrs["textAlign"], "right");
    }

    #[test]
    fn test_create_list_node_bullet() {
        let items = vec![TiptapNode {
            node_type: "listItem".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        }];
        let node = create_list_node("bulletList", items);

        assert_eq!(node.node_type, "bulletList");
        assert_eq!(node.content.len(), 1);
    }

    #[test]
    fn test_create_list_node_ordered() {
        let items = vec![
            TiptapNode {
                node_type: "listItem".to_string(),
                content: vec![],
                text: None,
                marks: vec![],
                attrs: None,
            },
            TiptapNode {
                node_type: "listItem".to_string(),
                content: vec![],
                text: None,
                marks: vec![],
                attrs: None,
            },
        ];
        let node = create_list_node("orderedList", items);

        assert_eq!(node.node_type, "orderedList");
        assert_eq!(node.content.len(), 2);
    }

    #[test]
    fn test_create_list_item_node() {
        let content = vec![TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("List item".to_string()),
            marks: vec![],
            attrs: None,
        }];
        let props = ParagraphProperties::default();
        let node = create_list_item_node(content, &props);

        assert_eq!(node.node_type, "listItem");
        assert_eq!(node.content.len(), 1);
        assert_eq!(node.content[0].node_type, "paragraph");
    }

    #[test]
    fn test_create_image_node() {
        let node = create_image_node("img-abc123");

        assert_eq!(node.node_type, "image");
        let attrs = node.attrs.unwrap();
        assert_eq!(attrs["src"], "midlight://img-abc123");
        assert_eq!(attrs["alt"], "");
        assert_eq!(attrs["title"], "");
    }

    // ============================================================================
    // Error Type Tests
    // ============================================================================

    #[test]
    fn test_docx_import_error_display() {
        let err = DocxImportError::FileNotFound("/path/to/file.docx".to_string());
        assert!(err.to_string().contains("File not found"));
        assert!(err.to_string().contains("/path/to/file.docx"));

        let err = DocxImportError::FileTooLarge("large.docx".to_string());
        assert!(err.to_string().contains("too large"));

        let err = DocxImportError::InvalidFormat("Bad format".to_string());
        assert!(err.to_string().contains("Invalid DOCX format"));

        let err = DocxImportError::XmlParse("Parse error".to_string());
        assert!(err.to_string().contains("XML parsing error"));

        let err = DocxImportError::ZipError("Zip error".to_string());
        assert!(err.to_string().contains("ZIP error"));

        let err = DocxImportError::IoError("IO error".to_string());
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: DocxImportError = io_err.into();
        assert!(matches!(err, DocxImportError::IoError(_)));
    }

    #[test]
    fn test_error_from_quick_xml() {
        // Create a quick_xml error via the Io variant
        let io_error = std::io::Error::new(std::io::ErrorKind::Other, "test xml error");
        let xml_err = quick_xml::Error::Io(std::sync::Arc::new(io_error));
        let err: DocxImportError = xml_err.into();
        assert!(matches!(err, DocxImportError::XmlParse(_)));
    }

    // ============================================================================
    // Tiptap Conversion Integration Tests
    // ============================================================================

    #[test]
    fn test_convert_to_tiptap_empty() {
        let paragraphs: Vec<ParsedParagraph> = vec![];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.doc_type, "doc");
        assert!(doc.content.is_empty());
    }

    #[test]
    fn test_convert_to_tiptap_simple_paragraph() {
        let paragraphs = vec![ParsedParagraph {
            runs: vec![TextRun {
                text: "Hello world".to_string(),
                props: RunProperties::default(),
            }],
            props: ParagraphProperties::default(),
            images: vec![],
        }];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.content.len(), 1);
        assert_eq!(doc.content[0].node_type, "paragraph");
    }

    #[test]
    fn test_convert_to_tiptap_heading() {
        let paragraphs = vec![ParsedParagraph {
            runs: vec![TextRun {
                text: "Title".to_string(),
                props: RunProperties::default(),
            }],
            props: ParagraphProperties {
                style_id: Some("Heading1".to_string()),
                ..Default::default()
            },
            images: vec![],
        }];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.content.len(), 1);
        assert_eq!(doc.content[0].node_type, "heading");
    }

    #[test]
    fn test_convert_to_tiptap_empty_paragraph() {
        let paragraphs = vec![ParsedParagraph {
            runs: vec![],
            props: ParagraphProperties::default(),
            images: vec![],
        }];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.content.len(), 1);
        assert_eq!(doc.content[0].node_type, "paragraph");
        assert!(doc.content[0].content.is_empty());
    }

    #[test]
    fn test_convert_to_tiptap_with_image() {
        let paragraphs = vec![ParsedParagraph {
            runs: vec![TextRun {
                text: "Text with image".to_string(),
                props: RunProperties::default(),
            }],
            props: ParagraphProperties::default(),
            images: vec!["rId1".to_string()],
        }];
        let mut image_id_map: HashMap<String, String> = HashMap::new();
        image_id_map.insert("rId1".to_string(), "img-abc123".to_string());
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        // Should have image node first, then paragraph
        assert_eq!(doc.content.len(), 2);
        assert_eq!(doc.content[0].node_type, "image");
        assert_eq!(doc.content[1].node_type, "paragraph");
    }

    #[test]
    fn test_convert_to_tiptap_bullet_list() {
        let paragraphs = vec![
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Item 1".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(1), // Odd = bullet
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Item 2".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(1),
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
        ];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.content.len(), 1);
        assert_eq!(doc.content[0].node_type, "bulletList");
        assert_eq!(doc.content[0].content.len(), 2);
    }

    #[test]
    fn test_convert_to_tiptap_ordered_list() {
        let paragraphs = vec![
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "First".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(2), // Even = ordered
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Second".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(2),
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
        ];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.content.len(), 1);
        assert_eq!(doc.content[0].node_type, "orderedList");
    }

    #[test]
    fn test_convert_to_tiptap_mixed_content() {
        let paragraphs = vec![
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Title".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    style_id: Some("Heading1".to_string()),
                    ..Default::default()
                },
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Regular paragraph".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties::default(),
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "List item".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(1),
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
        ];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.content.len(), 3);
        assert_eq!(doc.content[0].node_type, "heading");
        assert_eq!(doc.content[1].node_type, "paragraph");
        assert_eq!(doc.content[2].node_type, "bulletList");
    }

    // ============================================================================
    // Type and Struct Tests
    // ============================================================================

    #[test]
    fn test_import_stats_default() {
        let stats = ImportStats::default();
        assert_eq!(stats.paragraph_count, 0);
        assert_eq!(stats.heading_count, 0);
        assert_eq!(stats.list_count, 0);
        assert_eq!(stats.image_count, 0);
        assert_eq!(stats.table_count, 0);
    }

    #[test]
    fn test_run_properties_default() {
        let props = RunProperties::default();
        assert!(!props.bold);
        assert!(!props.italic);
        assert!(!props.underline);
        assert!(!props.strike);
        assert!(props.color.is_none());
        assert!(props.highlight.is_none());
        assert!(props.font_size.is_none());
        assert!(props.font_family.is_none());
    }

    #[test]
    fn test_paragraph_properties_default() {
        let props = ParagraphProperties::default();
        assert!(props.style_id.is_none());
        assert!(props.alignment.is_none());
        assert!(props.numbering_level.is_none());
        assert!(props.numbering_id.is_none());
    }

    #[test]
    fn test_import_warning_serialization() {
        let warning = ImportWarning {
            code: "test_code".to_string(),
            message: "Test message".to_string(),
        };
        let json = serde_json::to_string(&warning).unwrap();
        assert!(json.contains("test_code"));
        assert!(json.contains("Test message"));
    }

    #[test]
    fn test_extracted_image_serialization() {
        let image = ExtractedImage {
            id: "img-123".to_string(),
            data: vec![1, 2, 3],
            content_type: "image/png".to_string(),
            original_name: "test.png".to_string(),
            rel_id: "rId1".to_string(),
        };
        let json = serde_json::to_string(&image).unwrap();
        assert!(json.contains("img-123"));
        assert!(json.contains("image/png"));
    }

    // ============================================================================
    // Import/Analyze Function Tests
    // ============================================================================

    #[test]
    fn test_import_docx_file_not_found() {
        let result = import_docx(Path::new("/nonexistent/file.docx"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DocxImportError::FileNotFound(_)));
    }

    #[test]
    fn test_import_docx_wrong_extension() {
        use tempfile::NamedTempFile;
        let temp = NamedTempFile::with_suffix(".txt").unwrap();
        std::fs::write(temp.path(), "content").unwrap();

        let result = import_docx(temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DocxImportError::InvalidFormat(_)));
        assert!(err.to_string().contains(".docx extension"));
    }

    #[test]
    fn test_import_docx_invalid_zip() {
        use tempfile::NamedTempFile;
        let temp = NamedTempFile::with_suffix(".docx").unwrap();
        std::fs::write(temp.path(), "not a zip file").unwrap();

        let result = import_docx(temp.path());
        assert!(result.is_err());
        // Should fail on ZIP parsing
        let err = result.unwrap_err();
        assert!(matches!(err, DocxImportError::ZipError(_)));
    }

    #[test]
    fn test_analyze_docx_file_not_found() {
        let result = analyze_docx(Path::new("/nonexistent/file.docx"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DocxImportError::FileNotFound(_)));
    }

    // ============================================================================
    // Minimal DOCX Creation and Import Tests
    // ============================================================================

    /// Create a minimal valid DOCX file for testing
    fn create_minimal_docx(path: &Path, document_xml: &str) {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        // Add [Content_Types].xml
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
    <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
    <Default Extension="xml" ContentType="application/xml"/>
    <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#).unwrap();

        // Add _rels/.rels
        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
    <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#).unwrap();

        // Add word/document.xml
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();

        // Add word/_rels/document.xml.rels (empty relationships)
        zip.start_file("word/_rels/document.xml.rels", options)
            .unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#,
        )
        .unwrap();

        zip.finish().unwrap();
    }

    /// Create a minimal DOCX with image relationships
    fn create_docx_with_image(path: &Path, document_xml: &str, image_data: &[u8]) {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        // Add [Content_Types].xml
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
    <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
    <Default Extension="xml" ContentType="application/xml"/>
    <Default Extension="png" ContentType="image/png"/>
    <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#).unwrap();

        // Add _rels/.rels
        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
    <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#).unwrap();

        // Add word/document.xml
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();

        // Add word/_rels/document.xml.rels with image relationship
        zip.start_file("word/_rels/document.xml.rels", options)
            .unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
    <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
</Relationships>"#)
            .unwrap();

        // Add word/media/image1.png
        zip.start_file("word/media/image1.png", options).unwrap();
        zip.write_all(image_data).unwrap();

        zip.finish().unwrap();
    }

    #[test]
    fn test_import_docx_minimal_valid() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:t>Hello World</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        // Warnings vector exists (may or may not have warnings)
        let _ = import_result.warnings.len();
        assert!(import_result.images.is_empty());
    }

    #[test]
    fn test_import_docx_with_heading() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:pPr>
                <w:pStyle w:val="Heading1"/>
            </w:pPr>
            <w:r>
                <w:t>Main Title</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert_eq!(import_result.stats.heading_count, 1);
    }

    #[test]
    fn test_import_docx_with_bold_text() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:b/>
                </w:rPr>
                <w:t>Bold text</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        // Check the tiptap_json contains bold mark
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("bold"));
    }

    #[test]
    fn test_import_docx_with_italic_text() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:i/>
                </w:rPr>
                <w:t>Italic text</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("italic"));
    }

    #[test]
    fn test_import_docx_with_underline() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:u/>
                </w:rPr>
                <w:t>Underlined</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("underline"));
    }

    #[test]
    fn test_import_docx_with_strikethrough() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:strike/>
                </w:rPr>
                <w:t>Strikethrough</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("strike"));
    }

    #[test]
    fn test_import_docx_with_color() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:color w:val="FF0000"/>
                </w:rPr>
                <w:t>Red text</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("textStyle"));
        assert!(json_str.contains("color"));
    }

    #[test]
    fn test_import_docx_with_highlight() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:highlight w:val="yellow"/>
                </w:rPr>
                <w:t>Highlighted</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("highlight"));
    }

    #[test]
    fn test_import_docx_with_font_size() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:sz w:val="48"/>
                </w:rPr>
                <w:t>Large text</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("fontSize"));
        assert!(json_str.contains("24px")); // 48 half-points = 24px
    }

    #[test]
    fn test_import_docx_with_font_family() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:rFonts w:ascii="Arial"/>
                </w:rPr>
                <w:t>Arial text</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("fontFamily"));
        assert!(json_str.contains("Arial"));
    }

    #[test]
    fn test_import_docx_with_alignment() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:pPr>
                <w:jc w:val="center"/>
            </w:pPr>
            <w:r>
                <w:t>Centered text</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("textAlign"));
        assert!(json_str.contains("center"));
    }

    #[test]
    fn test_import_docx_with_right_alignment() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:pPr>
                <w:jc w:val="right"/>
            </w:pPr>
            <w:r>
                <w:t>Right aligned</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("right"));
    }

    #[test]
    fn test_import_docx_with_justify_alignment() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:pPr>
                <w:jc w:val="both"/>
            </w:pPr>
            <w:r>
                <w:t>Justified text</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("justify"));
    }

    #[test]
    fn test_import_docx_with_numbered_list() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:pPr>
                <w:numPr>
                    <w:ilvl w:val="0"/>
                    <w:numId w:val="2"/>
                </w:numPr>
            </w:pPr>
            <w:r>
                <w:t>List item 1</w:t>
            </w:r>
        </w:p>
        <w:p>
            <w:pPr>
                <w:numPr>
                    <w:ilvl w:val="0"/>
                    <w:numId w:val="2"/>
                </w:numPr>
            </w:pPr>
            <w:r>
                <w:t>List item 2</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert_eq!(import_result.stats.list_count, 2);
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("orderedList"));
    }

    #[test]
    fn test_import_docx_with_bullet_list() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:pPr>
                <w:numPr>
                    <w:ilvl w:val="0"/>
                    <w:numId w:val="1"/>
                </w:numPr>
            </w:pPr>
            <w:r>
                <w:t>Bullet item</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        assert!(json_str.contains("bulletList"));
    }

    #[test]
    fn test_import_docx_with_table_warning() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:tbl>
            <w:tr>
                <w:tc>
                    <w:p>
                        <w:r>
                            <w:t>Cell</w:t>
                        </w:r>
                    </w:p>
                </w:tc>
            </w:tr>
        </w:tbl>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert_eq!(import_result.stats.table_count, 1);
        // Should have a warning about tables
        assert!(import_result
            .warnings
            .iter()
            .any(|w| w.code == "unsupported_table"));
    }

    #[test]
    fn test_import_docx_with_break() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:t>Line 1</w:t>
            </w:r>
            <w:r>
                <w:br/>
            </w:r>
            <w:r>
                <w:t>Line 2</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_import_docx_with_image() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
    <w:body>
        <w:p>
            <w:r>
                <w:drawing>
                    <wp:inline>
                        <a:graphic>
                            <a:graphicData>
                                <a:blip r:embed="rId1"/>
                            </a:graphicData>
                        </a:graphic>
                    </wp:inline>
                </w:drawing>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        // PNG magic bytes
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        create_docx_with_image(&docx_path, document_xml, &png_data);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert_eq!(import_result.images.len(), 1);
        assert_eq!(import_result.images[0].content_type, "image/png");
    }

    #[test]
    fn test_analyze_docx_valid() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:pPr>
                <w:pStyle w:val="Heading1"/>
            </w:pPr>
            <w:r>
                <w:t>Title</w:t>
            </w:r>
        </w:p>
        <w:p>
            <w:r>
                <w:t>Paragraph 1</w:t>
            </w:r>
        </w:p>
        <w:p>
            <w:r>
                <w:t>Paragraph 2</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = analyze_docx(&docx_path);
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert_eq!(analysis.heading_count, 1);
        assert_eq!(analysis.paragraph_count, 2);
        assert!(!analysis.has_tables);
    }

    #[test]
    fn test_analyze_docx_with_tables() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:tbl>
            <w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr>
        </w:tbl>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = analyze_docx(&docx_path);
        assert!(result.is_ok());

        let analysis = result.unwrap();
        assert_eq!(analysis.table_count, 1);
        assert!(analysis.has_tables);
    }

    #[test]
    fn test_import_docx_missing_document_xml() {
        use std::io::Write;
        use tempfile::TempDir;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        // Create a ZIP file without document.xml
        let file = File::create(&docx_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(b"<?xml version=\"1.0\"?><Types></Types>")
            .unwrap();

        zip.finish().unwrap();

        let result = import_docx(&docx_path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DocxImportError::InvalidFormat(_)));
    }

    #[test]
    fn test_import_docx_bold_false_value() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        // Test that w:b with w:val="false" doesn't make text bold
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:b w:val="false"/>
                </w:rPr>
                <w:t>Not bold</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        // Should NOT contain bold mark
        assert!(!json_str.contains("\"bold\""));
    }

    #[test]
    fn test_import_docx_color_auto() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        // Test that color="auto" is ignored
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r>
                <w:rPr>
                    <w:color w:val="auto"/>
                </w:rPr>
                <w:t>Auto color</w:t>
            </w:r>
        </w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        let json_str = serde_json::to_string(&import_result.tiptap_json).unwrap();
        // Should NOT have textStyle with color for "auto"
        assert!(!json_str.contains("textStyle") || !json_str.contains("\"color\""));
    }

    #[test]
    fn test_import_docx_multiple_paragraphs() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p><w:r><w:t>Para 1</w:t></w:r></w:p>
        <w:p><w:r><w:t>Para 2</w:t></w:r></w:p>
        <w:p><w:r><w:t>Para 3</w:t></w:r></w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());

        let import_result = result.unwrap();
        assert_eq!(import_result.stats.paragraph_count, 3);
    }

    #[test]
    fn test_import_docx_empty_paragraph() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let docx_path = temp_dir.path().join("test.docx");

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p></w:p>
        <w:p><w:r><w:t>Content</w:t></w:r></w:p>
    </w:body>
</w:document>"#;

        create_minimal_docx(&docx_path, document_xml);

        let result = import_docx(&docx_path);
        assert!(result.is_ok());
    }

    // ============================================================================
    // Additional Tiptap Conversion Tests
    // ============================================================================

    #[test]
    fn test_convert_to_tiptap_list_type_switch() {
        // Test switching from bullet list to ordered list
        let paragraphs = vec![
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Bullet".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(1), // Odd = bullet
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Ordered".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(2), // Even = ordered
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
        ];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        // Should have two separate lists
        assert_eq!(doc.content.len(), 2);
        assert_eq!(doc.content[0].node_type, "bulletList");
        assert_eq!(doc.content[1].node_type, "orderedList");
    }

    #[test]
    fn test_convert_to_tiptap_list_then_paragraph() {
        // Test list followed by regular paragraph
        let paragraphs = vec![
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "List item".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(1),
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Regular para".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties::default(),
                images: vec![],
            },
        ];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        assert_eq!(doc.content.len(), 2);
        assert_eq!(doc.content[0].node_type, "bulletList");
        assert_eq!(doc.content[1].node_type, "paragraph");
    }

    #[test]
    fn test_convert_to_tiptap_heading_breaks_list() {
        // Test heading in the middle of list items
        let paragraphs = vec![
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Item 1".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(1),
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Heading".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    style_id: Some("Heading1".to_string()),
                    ..Default::default()
                },
                images: vec![],
            },
            ParsedParagraph {
                runs: vec![TextRun {
                    text: "Item 2".to_string(),
                    props: RunProperties::default(),
                }],
                props: ParagraphProperties {
                    numbering_id: Some(1),
                    numbering_level: Some(0),
                    ..Default::default()
                },
                images: vec![],
            },
        ];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        // Should be: bulletList, heading, bulletList
        assert_eq!(doc.content.len(), 3);
        assert_eq!(doc.content[0].node_type, "bulletList");
        assert_eq!(doc.content[1].node_type, "heading");
        assert_eq!(doc.content[2].node_type, "bulletList");
    }

    #[test]
    fn test_convert_to_tiptap_image_without_mapping() {
        // Test image reference that doesn't exist in map
        let paragraphs = vec![ParsedParagraph {
            runs: vec![],
            props: ParagraphProperties::default(),
            images: vec!["rIdNonexistent".to_string()],
        }];
        let image_id_map: HashMap<String, String> = HashMap::new();
        let mut warnings = vec![];

        let doc = convert_to_tiptap(paragraphs, &image_id_map, &mut warnings);

        // Should just have the empty paragraph, no image node
        assert_eq!(doc.content.len(), 1);
        assert_eq!(doc.content[0].node_type, "paragraph");
    }

    // ============================================================================
    // DocxAnalysis Tests
    // ============================================================================

    #[test]
    fn test_docx_analysis_serialization() {
        let analysis = DocxAnalysis {
            file_name: "test.docx".to_string(),
            file_size: 12345,
            paragraph_count: 10,
            heading_count: 3,
            image_count: 2,
            table_count: 1,
            has_tables: true,
            warnings: vec!["warning1".to_string()],
        };

        let json = serde_json::to_string(&analysis).unwrap();
        assert!(json.contains("test.docx"));
        assert!(json.contains("12345"));
        // serde uses snake_case by default
        assert!(json.contains("\"paragraph_count\":10"));
        assert!(json.contains("\"has_tables\":true"));
    }

    #[test]
    fn test_docx_import_result_serialization() {
        let result = DocxImportResult {
            tiptap_json: serde_json::json!({"type": "doc"}),
            images: vec![],
            warnings: vec![ImportWarning {
                code: "test".to_string(),
                message: "test message".to_string(),
            }],
            stats: ImportStats::default(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("tiptap_json") || json.contains("tiptapJson"));
        assert!(json.contains("test message"));
    }

    // ============================================================================
    // Error Conversion Tests
    // ============================================================================

    #[test]
    fn test_error_from_zip() {
        // Test conversion from zip error
        let zip_err = zip::result::ZipError::FileNotFound;
        let err: DocxImportError = zip_err.into();
        assert!(matches!(err, DocxImportError::ZipError(_)));
        assert!(err.to_string().contains("ZIP error"));
    }
}
