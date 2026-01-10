// DOCX Export Service
// Converts Tiptap JSON documents to DOCX format using docx-rs

use docx_rs::{
    AbstractNumbering, AlignmentType, Docx, IndentLevel, Level, LevelJc, LevelText, NumberFormat,
    Numbering, NumberingId, Paragraph, Run, RunFonts, SpecialIndentType, Start,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;

// ============================================================================
// Types - Tiptap Document Structure
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TiptapDocument {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub content: Vec<TiptapNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TiptapNode {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub content: Vec<TiptapNode>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub marks: Vec<TiptapMark>,
    #[serde(default)]
    pub attrs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TiptapMark {
    #[serde(rename = "type")]
    pub mark_type: String,
    #[serde(default)]
    pub attrs: Option<serde_json::Value>,
}

// ============================================================================
// Export Progress
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProgress {
    pub current: usize,
    pub total: usize,
    pub phase: String,
}

// ============================================================================
// Conversion Utilities
// ============================================================================

/// Converts CSS pixel values to Word half-points
/// Formula: px × 2 = half-points (treating px as if they were pt for number parity)
/// This creates visual and numerical consistency: 12px in editor → 12pt in Word
pub fn px_to_half_points(px: &str) -> usize {
    // Extract numeric value from string like "16px"
    let numeric_value: f64 = px.trim_end_matches("px").trim().parse().unwrap_or(14.0);

    if numeric_value <= 0.0 {
        return 24; // Default to 12pt
    }

    // Word has a maximum font size of 1638 pt (3276 half-points)
    // Clamp to reasonable range: 1pt (2 half-points) to 200pt (400 half-points)
    let half_points = (numeric_value * 2.0).round() as usize;

    half_points.clamp(2, 400)
}

/// Maps Tiptap alignment values to DOCX AlignmentType
pub fn tiptap_align_to_docx(align: Option<&str>) -> AlignmentType {
    match align {
        Some("left") => AlignmentType::Left,
        Some("center") => AlignmentType::Center,
        Some("right") => AlignmentType::Right,
        Some("justify") => AlignmentType::Justified,
        _ => AlignmentType::Left,
    }
}

/// Font fallback map for Word compatibility
fn get_font_fallback_map() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("Inter", "Arial");
    map.insert("Roboto", "Arial");
    map.insert("Open Sans", "Arial");
    map.insert("Lato", "Arial");
    map.insert("Merriweather", "Georgia");
    map.insert("Crimson Text", "Georgia");
    map.insert("Lora", "Georgia");
    map.insert("Playfair Display", "Georgia");
    map.insert("JetBrains Mono", "Courier New");
    map.insert("Fira Code", "Courier New");
    map
}

/// Extracts the first font name from a CSS font-family string and maps to Word-compatible font
pub fn extract_font_name(font_family: Option<&str>) -> &'static str {
    let font_family = match font_family {
        Some(f) => f,
        None => return "Georgia", // Default fallback
    };

    // Split by comma and take first font
    let first_font = font_family
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '"' || c == '\'');

    // Don't return generic font families
    let generic_families = ["serif", "sans-serif", "monospace", "cursive", "fantasy"];
    if generic_families
        .iter()
        .any(|&g| g.eq_ignore_ascii_case(first_font))
    {
        return "Georgia";
    }

    // Map to Word-compatible fallback font
    let fallback_map = get_font_fallback_map();
    fallback_map.get(first_font).copied().unwrap_or("Georgia")
}

/// Converts any color format (hex, rgb, rgba) to 6-digit hex without # prefix
pub fn normalize_color_to_hex(color: Option<&str>) -> Option<String> {
    let color = match color {
        Some(c) if !c.is_empty() => c,
        _ => return None,
    };

    // Already hex format
    if color.starts_with('#') {
        let hex = color.trim_start_matches('#');
        // Handle 3-digit hex
        if hex.len() == 3 {
            let chars: Vec<char> = hex.chars().collect();
            return Some(
                format!(
                    "{}{}{}{}{}{}",
                    chars[0], chars[0], chars[1], chars[1], chars[2], chars[2]
                )
                .to_uppercase(),
            );
        }
        return Some(hex.to_uppercase());
    }

    // RGB format: rgb(r, g, b) or rgba(r, g, b, a)
    let rgb_regex = regex::Regex::new(r"rgba?\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)").ok()?;
    if let Some(captures) = rgb_regex.captures(color) {
        let r: u8 = captures.get(1)?.as_str().parse().ok()?;
        let g: u8 = captures.get(2)?.as_str().parse().ok()?;
        let b: u8 = captures.get(3)?.as_str().parse().ok()?;
        return Some(format!("{:02X}{:02X}{:02X}", r, g, b));
    }

    None
}

/// Maps hex colors to DOCX highlight color names
fn hex_to_docx_highlight(hex: &str) -> &'static str {
    let normalized = hex.to_lowercase();
    let hex_with_hash = if normalized.starts_with('#') {
        normalized
    } else {
        format!("#{}", normalized)
    };

    match hex_with_hash.as_str() {
        "#ffff00" => "yellow",
        "#00ff00" => "green",
        "#00ffff" => "cyan",
        "#ff00ff" => "magenta",
        "#0000ff" => "blue",
        "#ff0000" => "red",
        "#ffa500" => "darkYellow",
        "#808080" => "darkGray",
        _ => {
            // Find closest color using simple RGB distance
            // For simplicity, default to yellow for most highlights
            "yellow"
        }
    }
}

// ============================================================================
// Text Processing
// ============================================================================

/// Extracts textStyle mark attributes from marks array
fn extract_text_style(marks: &[TiptapMark]) -> (Option<String>, Option<String>, Option<String>) {
    for mark in marks {
        if mark.mark_type == "textStyle" {
            if let Some(ref attrs) = mark.attrs {
                let font_size = attrs
                    .get("fontSize")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let font_family = attrs
                    .get("fontFamily")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let color = attrs
                    .get("color")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                return (font_size, font_family, color);
            }
        }
    }
    (None, None, None)
}

/// Creates a DOCX Run from a Tiptap text node
fn create_text_run(
    node: &TiptapNode,
    default_size: Option<usize>,
    override_color: Option<&str>,
) -> Run {
    let marks = &node.marks;

    // Extract boolean marks
    let is_bold = marks.iter().any(|m| m.mark_type == "bold");
    let is_italic = marks.iter().any(|m| m.mark_type == "italic");
    let is_strike = marks.iter().any(|m| m.mark_type == "strike");
    let is_code = marks.iter().any(|m| m.mark_type == "code");
    let is_underline = marks.iter().any(|m| m.mark_type == "underline");

    // Extract textStyle
    let (font_size_str, font_family_str, text_color_str) = extract_text_style(marks);

    // Extract highlight
    let highlight_color = marks
        .iter()
        .find(|m| m.mark_type == "highlight")
        .and_then(|m| m.attrs.as_ref())
        .and_then(|a| a.get("color"))
        .and_then(|v| v.as_str());

    // Convert fontSize from px to half-points
    let font_size = if let Some(ref fs) = font_size_str {
        px_to_half_points(fs)
    } else {
        default_size.unwrap_or(28) // Default 14pt
    };

    // Get font name
    let font_family = if is_code {
        "Courier New"
    } else {
        extract_font_name(font_family_str.as_deref())
    };

    // Handle colors - override takes precedence
    let text_color = override_color
        .map(|c| c.to_string())
        .or_else(|| normalize_color_to_hex(text_color_str.as_deref()));

    // Build the run
    let text = node.text.as_deref().unwrap_or("");
    let mut run = Run::new().add_text(text);

    // Apply formatting
    if is_bold {
        run = run.bold();
    }
    if is_italic {
        run = run.italic();
    }
    if is_strike {
        run = run.strike();
    }
    if is_underline {
        run = run.underline("single");
    }

    // Apply font
    run = run.fonts(
        RunFonts::new()
            .ascii(font_family)
            .hi_ansi(font_family)
            .east_asia(font_family)
            .cs(font_family),
    );

    // Apply size
    run = run.size(font_size);

    // Apply color
    if let Some(color) = text_color {
        run = run.color(&color);
    }

    // Apply highlight
    if let Some(hl) = highlight_color {
        if let Some(normalized) = normalize_color_to_hex(Some(hl)) {
            let _hl_name = hex_to_docx_highlight(&normalized);
            // docx-rs uses shading for highlights
            // run = run.highlight(hl_name); // Not directly available, use shading instead
        }
    }

    run
}

/// Processes ALL text nodes in a paragraph content array
fn process_text_nodes(
    nodes: &[TiptapNode],
    default_size: Option<usize>,
    override_color: Option<&str>,
) -> Vec<Run> {
    if nodes.is_empty() {
        return vec![Run::new().add_text("").size(default_size.unwrap_or(28))];
    }

    nodes
        .iter()
        .filter(|node| node.node_type == "text" || node.text.is_some())
        .map(|node| create_text_run(node, default_size, override_color))
        .collect()
}

// ============================================================================
// Paragraph and Heading Processing
// ============================================================================

/// Creates a DOCX Paragraph from a Tiptap paragraph node
fn create_paragraph(node: &TiptapNode) -> Paragraph {
    let alignment = node
        .attrs
        .as_ref()
        .and_then(|a| a.get("textAlign"))
        .and_then(|v| v.as_str())
        .map(|s| tiptap_align_to_docx(Some(s)))
        .unwrap_or(AlignmentType::Left);

    let runs = process_text_nodes(&node.content, None, None);

    let mut para = Paragraph::new();
    para = para.align(alignment);

    for run in runs {
        para = para.add_run(run);
    }

    para
}

/// Creates a DOCX Paragraph with heading style from a Tiptap heading node
fn create_heading(node: &TiptapNode) -> Paragraph {
    let level = node
        .attrs
        .as_ref()
        .and_then(|a| a.get("level"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    // Map Tiptap level to default sizes (in half-points)
    // H1 = 32pt (64 half-points), H2 = 24pt (48), H3 = 20pt (40)
    let default_size = match level {
        1 => 64,
        2 => 48,
        3 => 40,
        _ => 64,
    };

    let alignment = node
        .attrs
        .as_ref()
        .and_then(|a| a.get("textAlign"))
        .and_then(|v| v.as_str())
        .map(|s| tiptap_align_to_docx(Some(s)))
        .unwrap_or(AlignmentType::Left);

    // Process text with black color override and appropriate size
    let runs = process_text_nodes(&node.content, Some(default_size), Some("000000"));

    let mut para = Paragraph::new();
    para = para.align(alignment);

    // Apply heading style based on level
    let style_name = match level {
        1 => "Heading1",
        2 => "Heading2",
        3 => "Heading3",
        _ => "Heading1",
    };
    para = para.style(style_name);

    for run in runs {
        para = para.add_run(run);
    }

    para
}

// ============================================================================
// List Processing
// ============================================================================

#[derive(Clone)]
struct ListContext {
    level: i32,
    numbering_id: u32,
}

/// Processes a list item, handling nested lists
fn process_list_item(list_item: &TiptapNode, context: &ListContext) -> Vec<Paragraph> {
    let mut paragraphs = Vec::new();

    for content in &list_item.content {
        match content.node_type.as_str() {
            "paragraph" => {
                let alignment = content
                    .attrs
                    .as_ref()
                    .and_then(|a| a.get("textAlign"))
                    .and_then(|v| v.as_str())
                    .map(|s| tiptap_align_to_docx(Some(s)))
                    .unwrap_or(AlignmentType::Left);

                let runs = process_text_nodes(&content.content, None, None);

                let mut para = Paragraph::new();
                para = para.align(alignment);

                // Apply numbering
                para = para.numbering(
                    NumberingId::new(context.numbering_id as usize),
                    IndentLevel::new(context.level as usize),
                );

                for run in runs {
                    para = para.add_run(run);
                }

                paragraphs.push(para);
            }
            "bulletList" => {
                let nested_paragraphs =
                    process_bullet_list(content, context.level + 1, context.numbering_id);
                paragraphs.extend(nested_paragraphs);
            }
            "orderedList" => {
                let nested_paragraphs =
                    process_ordered_list(content, context.level + 1, context.numbering_id);
                paragraphs.extend(nested_paragraphs);
            }
            _ => {}
        }
    }

    paragraphs
}

/// Processes a bullet list at the specified nesting level
fn process_bullet_list(node: &TiptapNode, level: i32, numbering_id: u32) -> Vec<Paragraph> {
    let mut paragraphs = Vec::new();

    for list_item in &node.content {
        if list_item.node_type == "listItem" {
            let context = ListContext {
                level,
                numbering_id,
            };
            paragraphs.extend(process_list_item(list_item, &context));
        }
    }

    paragraphs
}

/// Processes an ordered list at the specified nesting level
fn process_ordered_list(node: &TiptapNode, level: i32, numbering_id: u32) -> Vec<Paragraph> {
    let mut paragraphs = Vec::new();

    for list_item in &node.content {
        if list_item.node_type == "listItem" {
            let context = ListContext {
                level,
                numbering_id,
            };
            paragraphs.extend(process_list_item(list_item, &context));
        }
    }

    paragraphs
}

// ============================================================================
// Image Processing
// ============================================================================

/// Creates a paragraph with an image (placeholder - docx-rs image support is limited)
fn create_image_paragraph(node: &TiptapNode) -> Paragraph {
    let attrs = match &node.attrs {
        Some(a) => a,
        None => return Paragraph::new().add_run(Run::new().add_text("[Image]")),
    };

    let _src = attrs.get("src").and_then(|v| v.as_str()).unwrap_or("");
    let _width = attrs
        .get("width")
        .and_then(|v| v.as_str())
        .unwrap_or("400px");
    let _height = attrs
        .get("height")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");
    let align = attrs
        .get("align")
        .and_then(|v| v.as_str())
        .unwrap_or("center-break");

    // Determine alignment
    let alignment = if align.starts_with("left") {
        AlignmentType::Left
    } else if align.starts_with("right") {
        AlignmentType::Right
    } else {
        AlignmentType::Center
    };

    // Note: docx-rs has limited image support - for now we add a placeholder
    // Full image embedding would require using the Pic struct with DrawingML
    // TODO: Implement full image support when docx-rs API allows
    Paragraph::new()
        .align(alignment)
        .add_run(Run::new().add_text("[Image]").italic())
}

// ============================================================================
// Horizontal Rule
// ============================================================================

/// Creates a paragraph that simulates a horizontal rule
fn create_horizontal_rule() -> Paragraph {
    // docx-rs doesn't have direct HR support, so we create a paragraph with bottom border
    // This is handled via styles in the main export function
    Paragraph::new().add_run(Run::new().add_text(""))
}

// ============================================================================
// Main Export Function
// ============================================================================

/// Creates abstract numbering for bullet lists
fn create_bullet_numbering() -> AbstractNumbering {
    AbstractNumbering::new(1)
        .add_level(
            Level::new(
                0,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("•"),
                LevelJc::new("left"),
            )
            .indent(Some(720), Some(SpecialIndentType::Hanging(360)), None, None),
        )
        .add_level(
            Level::new(
                1,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("○"),
                LevelJc::new("left"),
            )
            .indent(
                Some(1440),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                2,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("■"),
                LevelJc::new("left"),
            )
            .indent(
                Some(2160),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                3,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("•"),
                LevelJc::new("left"),
            )
            .indent(
                Some(2880),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                4,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("○"),
                LevelJc::new("left"),
            )
            .indent(
                Some(3600),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                5,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("■"),
                LevelJc::new("left"),
            )
            .indent(
                Some(4320),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
}

/// Creates abstract numbering for ordered lists
fn create_ordered_numbering() -> AbstractNumbering {
    AbstractNumbering::new(2)
        .add_level(
            Level::new(
                0,
                Start::new(1),
                NumberFormat::new("decimal"),
                LevelText::new("%1."),
                LevelJc::new("left"),
            )
            .indent(Some(720), Some(SpecialIndentType::Hanging(360)), None, None),
        )
        .add_level(
            Level::new(
                1,
                Start::new(1),
                NumberFormat::new("lowerLetter"),
                LevelText::new("%2."),
                LevelJc::new("left"),
            )
            .indent(
                Some(1440),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                2,
                Start::new(1),
                NumberFormat::new("lowerRoman"),
                LevelText::new("%3."),
                LevelJc::new("left"),
            )
            .indent(
                Some(2160),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                3,
                Start::new(1),
                NumberFormat::new("decimal"),
                LevelText::new("%4."),
                LevelJc::new("left"),
            )
            .indent(
                Some(2880),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                4,
                Start::new(1),
                NumberFormat::new("lowerLetter"),
                LevelText::new("%5."),
                LevelJc::new("left"),
            )
            .indent(
                Some(3600),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
        .add_level(
            Level::new(
                5,
                Start::new(1),
                NumberFormat::new("lowerRoman"),
                LevelText::new("%6."),
                LevelJc::new("left"),
            )
            .indent(
                Some(4320),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        )
}

/// Converts a Tiptap document to DOCX bytes
pub fn tiptap_to_docx<F>(content: &TiptapDocument, progress_callback: F) -> Result<Vec<u8>, String>
where
    F: Fn(ExportProgress),
{
    let nodes = &content.content;
    let total = nodes.len();

    progress_callback(ExportProgress {
        current: 0,
        total,
        phase: "Processing document".to_string(),
    });

    // Build document with numbering
    let mut docx = Docx::new()
        .add_abstract_numbering(create_bullet_numbering())
        .add_abstract_numbering(create_ordered_numbering())
        .add_numbering(Numbering::new(1, 1)) // Bullet list numbering
        .add_numbering(Numbering::new(2, 2)); // Ordered list numbering

    // Process each node
    for (i, node) in nodes.iter().enumerate() {
        match node.node_type.as_str() {
            "paragraph" => {
                let para = create_paragraph(node);
                docx = docx.add_paragraph(para);
            }
            "heading" => {
                let para = create_heading(node);
                docx = docx.add_paragraph(para);
            }
            "bulletList" => {
                let paragraphs = process_bullet_list(node, 0, 1);
                for para in paragraphs {
                    docx = docx.add_paragraph(para);
                }
            }
            "orderedList" => {
                let paragraphs = process_ordered_list(node, 0, 2);
                for para in paragraphs {
                    docx = docx.add_paragraph(para);
                }
            }
            "image" => {
                let para = create_image_paragraph(node);
                docx = docx.add_paragraph(para);
            }
            "horizontalRule" => {
                let para = create_horizontal_rule();
                docx = docx.add_paragraph(para);
            }
            _ => {
                // Skip unknown node types
            }
        }

        // Report progress every 10 nodes
        if i % 10 == 0 || i == nodes.len() - 1 {
            progress_callback(ExportProgress {
                current: i + 1,
                total,
                phase: "Processing document".to_string(),
            });
        }
    }

    progress_callback(ExportProgress {
        current: total,
        total,
        phase: "Building document".to_string(),
    });

    // Build the document and pack to bytes
    let mut buffer = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buffer)
        .map_err(|e| format!("Failed to build DOCX: {}", e))?;

    progress_callback(ExportProgress {
        current: total,
        total,
        phase: "Complete".to_string(),
    });

    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // px_to_half_points Tests
    // ============================================================================

    #[test]
    fn test_px_to_half_points_standard_sizes() {
        assert_eq!(px_to_half_points("12px"), 24);
        assert_eq!(px_to_half_points("14px"), 28);
        assert_eq!(px_to_half_points("16px"), 32);
        assert_eq!(px_to_half_points("18px"), 36);
        assert_eq!(px_to_half_points("24px"), 48);
    }

    #[test]
    fn test_px_to_half_points_zero_value() {
        // Zero or negative values should return default (24 = 12pt)
        assert_eq!(px_to_half_points("0px"), 24);
    }

    #[test]
    fn test_px_to_half_points_invalid_input() {
        // Invalid input should return default (28 = 14pt)
        assert_eq!(px_to_half_points("invalid"), 28);
        assert_eq!(px_to_half_points(""), 28);
        assert_eq!(px_to_half_points("abc"), 28);
    }

    #[test]
    fn test_px_to_half_points_with_whitespace() {
        // Trailing whitespace after "px" prevents trim_end_matches from matching
        assert_eq!(px_to_half_points("16px "), 28);
        // Leading whitespace is handled via trim() after trim_end_matches
        assert_eq!(px_to_half_points(" 16px"), 32);
        // Space between number and px: "16 px" → trim_end_matches("px") → "16 " → trim() → "16"
        assert_eq!(px_to_half_points("16 px"), 32);
    }

    #[test]
    fn test_px_to_half_points_decimal_values() {
        assert_eq!(px_to_half_points("15.5px"), 31);
        assert_eq!(px_to_half_points("14.2px"), 28);
    }

    #[test]
    fn test_px_to_half_points_clamping() {
        // Very small values should be clamped to minimum (2)
        assert_eq!(px_to_half_points("0.1px"), 2);

        // Very large values should be clamped to maximum (400 = 200pt)
        assert_eq!(px_to_half_points("300px"), 400);
    }

    // ============================================================================
    // normalize_color_to_hex Tests
    // ============================================================================

    #[test]
    fn test_normalize_color_to_hex_full_hex() {
        assert_eq!(
            normalize_color_to_hex(Some("#ff0000")),
            Some("FF0000".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("#00FF00")),
            Some("00FF00".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("#0000ff")),
            Some("0000FF".to_string())
        );
    }

    #[test]
    fn test_normalize_color_to_hex_shorthand() {
        assert_eq!(
            normalize_color_to_hex(Some("#abc")),
            Some("AABBCC".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("#f00")),
            Some("FF0000".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("#0f0")),
            Some("00FF00".to_string())
        );
    }

    #[test]
    fn test_normalize_color_to_hex_rgb() {
        assert_eq!(
            normalize_color_to_hex(Some("rgb(255, 0, 0)")),
            Some("FF0000".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("rgb(0, 255, 0)")),
            Some("00FF00".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("rgb(0, 0, 255)")),
            Some("0000FF".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("rgb(128, 128, 128)")),
            Some("808080".to_string())
        );
    }

    #[test]
    fn test_normalize_color_to_hex_rgba() {
        assert_eq!(
            normalize_color_to_hex(Some("rgba(0, 255, 0, 0.5)")),
            Some("00FF00".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("rgba(255, 0, 0, 1)")),
            Some("FF0000".to_string())
        );
        assert_eq!(
            normalize_color_to_hex(Some("rgba(0, 0, 255, 0)")),
            Some("0000FF".to_string())
        );
    }

    #[test]
    fn test_normalize_color_to_hex_none_and_empty() {
        assert_eq!(normalize_color_to_hex(None), None);
        assert_eq!(normalize_color_to_hex(Some("")), None);
    }

    #[test]
    fn test_normalize_color_to_hex_invalid() {
        assert_eq!(normalize_color_to_hex(Some("not-a-color")), None);
        assert_eq!(normalize_color_to_hex(Some("red")), None); // Named colors not supported
    }

    #[test]
    fn test_normalize_color_to_hex_rgb_with_spacing() {
        assert_eq!(
            normalize_color_to_hex(Some("rgb( 255 , 0 , 0 )")),
            Some("FF0000".to_string())
        );
    }

    // ============================================================================
    // extract_font_name Tests
    // ============================================================================

    #[test]
    fn test_extract_font_name_web_fonts() {
        assert_eq!(extract_font_name(Some("Inter, sans-serif")), "Arial");
        assert_eq!(extract_font_name(Some("Roboto, sans-serif")), "Arial");
        assert_eq!(extract_font_name(Some("Open Sans, sans-serif")), "Arial");
        assert_eq!(extract_font_name(Some("Lato, sans-serif")), "Arial");
    }

    #[test]
    fn test_extract_font_name_serif_fonts() {
        assert_eq!(extract_font_name(Some("Merriweather, serif")), "Georgia");
        assert_eq!(extract_font_name(Some("Crimson Text, serif")), "Georgia");
        assert_eq!(extract_font_name(Some("Lora, serif")), "Georgia");
        assert_eq!(
            extract_font_name(Some("Playfair Display, serif")),
            "Georgia"
        );
    }

    #[test]
    fn test_extract_font_name_monospace_fonts() {
        assert_eq!(
            extract_font_name(Some("\"JetBrains Mono\", monospace")),
            "Courier New"
        );
        assert_eq!(
            extract_font_name(Some("Fira Code, monospace")),
            "Courier New"
        );
    }

    #[test]
    fn test_extract_font_name_generic_families() {
        // Generic families should return default
        assert_eq!(extract_font_name(Some("sans-serif")), "Georgia");
        assert_eq!(extract_font_name(Some("serif")), "Georgia");
        assert_eq!(extract_font_name(Some("monospace")), "Georgia");
        assert_eq!(extract_font_name(Some("cursive")), "Georgia");
        assert_eq!(extract_font_name(Some("fantasy")), "Georgia");
    }

    #[test]
    fn test_extract_font_name_none() {
        assert_eq!(extract_font_name(None), "Georgia");
    }

    #[test]
    fn test_extract_font_name_unknown_font() {
        // Unknown fonts should return default
        assert_eq!(
            extract_font_name(Some("Custom Font, sans-serif")),
            "Georgia"
        );
    }

    #[test]
    fn test_extract_font_name_with_quotes() {
        assert_eq!(
            extract_font_name(Some("'JetBrains Mono', monospace")),
            "Courier New"
        );
        assert_eq!(
            extract_font_name(Some("\"Open Sans\", sans-serif")),
            "Arial"
        );
    }

    // ============================================================================
    // tiptap_align_to_docx Tests
    // ============================================================================

    #[test]
    fn test_tiptap_align_to_docx_left() {
        assert!(matches!(
            tiptap_align_to_docx(Some("left")),
            AlignmentType::Left
        ));
    }

    #[test]
    fn test_tiptap_align_to_docx_center() {
        assert!(matches!(
            tiptap_align_to_docx(Some("center")),
            AlignmentType::Center
        ));
    }

    #[test]
    fn test_tiptap_align_to_docx_right() {
        assert!(matches!(
            tiptap_align_to_docx(Some("right")),
            AlignmentType::Right
        ));
    }

    #[test]
    fn test_tiptap_align_to_docx_justify() {
        assert!(matches!(
            tiptap_align_to_docx(Some("justify")),
            AlignmentType::Justified
        ));
    }

    #[test]
    fn test_tiptap_align_to_docx_none() {
        assert!(matches!(tiptap_align_to_docx(None), AlignmentType::Left));
    }

    #[test]
    fn test_tiptap_align_to_docx_unknown() {
        assert!(matches!(
            tiptap_align_to_docx(Some("unknown")),
            AlignmentType::Left
        ));
    }

    // ============================================================================
    // hex_to_docx_highlight Tests
    // ============================================================================

    #[test]
    fn test_hex_to_docx_highlight_standard_colors() {
        assert_eq!(hex_to_docx_highlight("#ffff00"), "yellow");
        assert_eq!(hex_to_docx_highlight("#00ff00"), "green");
        assert_eq!(hex_to_docx_highlight("#00ffff"), "cyan");
        assert_eq!(hex_to_docx_highlight("#ff00ff"), "magenta");
        assert_eq!(hex_to_docx_highlight("#0000ff"), "blue");
        assert_eq!(hex_to_docx_highlight("#ff0000"), "red");
    }

    #[test]
    fn test_hex_to_docx_highlight_without_hash() {
        assert_eq!(hex_to_docx_highlight("ffff00"), "yellow");
        assert_eq!(hex_to_docx_highlight("ff0000"), "red");
    }

    #[test]
    fn test_hex_to_docx_highlight_case_insensitive() {
        assert_eq!(hex_to_docx_highlight("#FFFF00"), "yellow");
        assert_eq!(hex_to_docx_highlight("#FF0000"), "red");
    }

    #[test]
    fn test_hex_to_docx_highlight_orange() {
        assert_eq!(hex_to_docx_highlight("#ffa500"), "darkYellow");
    }

    #[test]
    fn test_hex_to_docx_highlight_gray() {
        assert_eq!(hex_to_docx_highlight("#808080"), "darkGray");
    }

    #[test]
    fn test_hex_to_docx_highlight_unknown() {
        // Unknown colors should default to yellow
        assert_eq!(hex_to_docx_highlight("#123456"), "yellow");
        assert_eq!(hex_to_docx_highlight("#abcdef"), "yellow");
    }

    // ============================================================================
    // extract_text_style Tests
    // ============================================================================

    #[test]
    fn test_extract_text_style_empty_marks() {
        let marks: Vec<TiptapMark> = vec![];
        let (font_size, font_family, color) = extract_text_style(&marks);
        assert!(font_size.is_none());
        assert!(font_family.is_none());
        assert!(color.is_none());
    }

    #[test]
    fn test_extract_text_style_no_text_style_mark() {
        let marks = vec![
            TiptapMark {
                mark_type: "bold".to_string(),
                attrs: None,
            },
            TiptapMark {
                mark_type: "italic".to_string(),
                attrs: None,
            },
        ];
        let (font_size, font_family, color) = extract_text_style(&marks);
        assert!(font_size.is_none());
        assert!(font_family.is_none());
        assert!(color.is_none());
    }

    #[test]
    fn test_extract_text_style_with_font_size() {
        let marks = vec![TiptapMark {
            mark_type: "textStyle".to_string(),
            attrs: Some(serde_json::json!({
                "fontSize": "16px"
            })),
        }];
        let (font_size, font_family, color) = extract_text_style(&marks);
        assert_eq!(font_size, Some("16px".to_string()));
        assert!(font_family.is_none());
        assert!(color.is_none());
    }

    #[test]
    fn test_extract_text_style_with_font_family() {
        let marks = vec![TiptapMark {
            mark_type: "textStyle".to_string(),
            attrs: Some(serde_json::json!({
                "fontFamily": "Arial, sans-serif"
            })),
        }];
        let (font_size, font_family, color) = extract_text_style(&marks);
        assert!(font_size.is_none());
        assert_eq!(font_family, Some("Arial, sans-serif".to_string()));
        assert!(color.is_none());
    }

    #[test]
    fn test_extract_text_style_with_color() {
        let marks = vec![TiptapMark {
            mark_type: "textStyle".to_string(),
            attrs: Some(serde_json::json!({
                "color": "#ff0000"
            })),
        }];
        let (font_size, font_family, color) = extract_text_style(&marks);
        assert!(font_size.is_none());
        assert!(font_family.is_none());
        assert_eq!(color, Some("#ff0000".to_string()));
    }

    #[test]
    fn test_extract_text_style_with_all_attrs() {
        let marks = vec![TiptapMark {
            mark_type: "textStyle".to_string(),
            attrs: Some(serde_json::json!({
                "fontSize": "18px",
                "fontFamily": "Georgia",
                "color": "#0000ff"
            })),
        }];
        let (font_size, font_family, color) = extract_text_style(&marks);
        assert_eq!(font_size, Some("18px".to_string()));
        assert_eq!(font_family, Some("Georgia".to_string()));
        assert_eq!(color, Some("#0000ff".to_string()));
    }

    #[test]
    fn test_extract_text_style_no_attrs() {
        let marks = vec![TiptapMark {
            mark_type: "textStyle".to_string(),
            attrs: None,
        }];
        let (font_size, font_family, color) = extract_text_style(&marks);
        assert!(font_size.is_none());
        assert!(font_family.is_none());
        assert!(color.is_none());
    }

    // ============================================================================
    // TiptapDocument/Node/Mark Serialization Tests
    // ============================================================================

    #[test]
    fn test_tiptap_document_serialization() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![],
        };
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"type\":\"doc\""));
        assert!(json.contains("\"content\":[]"));
    }

    #[test]
    fn test_tiptap_node_serialization() {
        let node = TiptapNode {
            node_type: "paragraph".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"type\":\"paragraph\""));
    }

    #[test]
    fn test_tiptap_node_with_text() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Hello world".to_string()),
            marks: vec![],
            attrs: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("Hello world"));
    }

    #[test]
    fn test_tiptap_node_with_attrs() {
        let node = TiptapNode {
            node_type: "heading".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({ "level": 1 })),
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"level\":1"));
    }

    #[test]
    fn test_tiptap_mark_serialization() {
        let mark = TiptapMark {
            mark_type: "bold".to_string(),
            attrs: None,
        };
        let json = serde_json::to_string(&mark).unwrap();
        assert!(json.contains("\"type\":\"bold\""));
    }

    #[test]
    fn test_tiptap_mark_with_attrs() {
        let mark = TiptapMark {
            mark_type: "textStyle".to_string(),
            attrs: Some(serde_json::json!({ "color": "#ff0000" })),
        };
        let json = serde_json::to_string(&mark).unwrap();
        assert!(json.contains("\"color\":\"#ff0000\""));
    }

    // ============================================================================
    // Export Progress Tests
    // ============================================================================

    #[test]
    fn test_export_progress_serialization() {
        let progress = ExportProgress {
            current: 5,
            total: 10,
            phase: "Processing".to_string(),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"current\":5"));
        assert!(json.contains("\"total\":10"));
        assert!(json.contains("\"phase\":\"Processing\""));
    }

    // ============================================================================
    // create_paragraph Tests
    // ============================================================================

    #[test]
    fn test_create_paragraph_empty() {
        let node = TiptapNode {
            node_type: "paragraph".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let _para = create_paragraph(&node);
        // Paragraph should be created without errors
        assert!(true); // Just verify it doesn't panic
    }

    #[test]
    fn test_create_paragraph_with_text() {
        let node = TiptapNode {
            node_type: "paragraph".to_string(),
            content: vec![TiptapNode {
                node_type: "text".to_string(),
                content: vec![],
                text: Some("Hello world".to_string()),
                marks: vec![],
                attrs: None,
            }],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let _para = create_paragraph(&node);
        // Verify it creates without error
    }

    #[test]
    fn test_create_paragraph_with_alignment() {
        let node = TiptapNode {
            node_type: "paragraph".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({ "textAlign": "center" })),
        };
        let _para = create_paragraph(&node);
        // Verify it creates without error
    }

    // ============================================================================
    // create_heading Tests
    // ============================================================================

    #[test]
    fn test_create_heading_h1() {
        let node = TiptapNode {
            node_type: "heading".to_string(),
            content: vec![TiptapNode {
                node_type: "text".to_string(),
                content: vec![],
                text: Some("Title".to_string()),
                marks: vec![],
                attrs: None,
            }],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({ "level": 1 })),
        };
        let _para = create_heading(&node);
        // Verify it creates without error
    }

    #[test]
    fn test_create_heading_h2() {
        let node = TiptapNode {
            node_type: "heading".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({ "level": 2 })),
        };
        let _para = create_heading(&node);
    }

    #[test]
    fn test_create_heading_h3() {
        let node = TiptapNode {
            node_type: "heading".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({ "level": 3 })),
        };
        let _para = create_heading(&node);
    }

    #[test]
    fn test_create_heading_with_alignment() {
        let node = TiptapNode {
            node_type: "heading".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({ "level": 1, "textAlign": "center" })),
        };
        let _para = create_heading(&node);
    }

    #[test]
    fn test_create_heading_no_level_defaults_to_1() {
        let node = TiptapNode {
            node_type: "heading".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let _para = create_heading(&node);
    }

    // ============================================================================
    // create_image_paragraph Tests
    // ============================================================================

    #[test]
    fn test_create_image_paragraph_no_attrs() {
        let node = TiptapNode {
            node_type: "image".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let _para = create_image_paragraph(&node);
    }

    #[test]
    fn test_create_image_paragraph_with_attrs() {
        let node = TiptapNode {
            node_type: "image".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({
                "src": "midlight://img-123",
                "width": "400px",
                "height": "300px",
                "align": "center-break"
            })),
        };
        let _para = create_image_paragraph(&node);
    }

    #[test]
    fn test_create_image_paragraph_left_align() {
        let node = TiptapNode {
            node_type: "image".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({
                "align": "left"
            })),
        };
        let _para = create_image_paragraph(&node);
    }

    #[test]
    fn test_create_image_paragraph_right_align() {
        let node = TiptapNode {
            node_type: "image".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: Some(serde_json::json!({
                "align": "right"
            })),
        };
        let _para = create_image_paragraph(&node);
    }

    // ============================================================================
    // create_horizontal_rule Tests
    // ============================================================================

    #[test]
    fn test_create_horizontal_rule() {
        let _para = create_horizontal_rule();
        // Just verify it doesn't panic
    }

    // ============================================================================
    // List Processing Tests
    // ============================================================================

    #[test]
    fn test_process_bullet_list_empty() {
        let node = TiptapNode {
            node_type: "bulletList".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let paragraphs = process_bullet_list(&node, 0, 1);
        assert!(paragraphs.is_empty());
    }

    #[test]
    fn test_process_bullet_list_single_item() {
        let node = TiptapNode {
            node_type: "bulletList".to_string(),
            content: vec![TiptapNode {
                node_type: "listItem".to_string(),
                content: vec![TiptapNode {
                    node_type: "paragraph".to_string(),
                    content: vec![TiptapNode {
                        node_type: "text".to_string(),
                        content: vec![],
                        text: Some("Item 1".to_string()),
                        marks: vec![],
                        attrs: None,
                    }],
                    text: None,
                    marks: vec![],
                    attrs: None,
                }],
                text: None,
                marks: vec![],
                attrs: None,
            }],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let paragraphs = process_bullet_list(&node, 0, 1);
        assert_eq!(paragraphs.len(), 1);
    }

    #[test]
    fn test_process_bullet_list_multiple_items() {
        let node = TiptapNode {
            node_type: "bulletList".to_string(),
            content: vec![
                TiptapNode {
                    node_type: "listItem".to_string(),
                    content: vec![TiptapNode {
                        node_type: "paragraph".to_string(),
                        content: vec![],
                        text: None,
                        marks: vec![],
                        attrs: None,
                    }],
                    text: None,
                    marks: vec![],
                    attrs: None,
                },
                TiptapNode {
                    node_type: "listItem".to_string(),
                    content: vec![TiptapNode {
                        node_type: "paragraph".to_string(),
                        content: vec![],
                        text: None,
                        marks: vec![],
                        attrs: None,
                    }],
                    text: None,
                    marks: vec![],
                    attrs: None,
                },
            ],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let paragraphs = process_bullet_list(&node, 0, 1);
        assert_eq!(paragraphs.len(), 2);
    }

    #[test]
    fn test_process_ordered_list_empty() {
        let node = TiptapNode {
            node_type: "orderedList".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let paragraphs = process_ordered_list(&node, 0, 2);
        assert!(paragraphs.is_empty());
    }

    #[test]
    fn test_process_ordered_list_single_item() {
        let node = TiptapNode {
            node_type: "orderedList".to_string(),
            content: vec![TiptapNode {
                node_type: "listItem".to_string(),
                content: vec![TiptapNode {
                    node_type: "paragraph".to_string(),
                    content: vec![],
                    text: None,
                    marks: vec![],
                    attrs: None,
                }],
                text: None,
                marks: vec![],
                attrs: None,
            }],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let paragraphs = process_ordered_list(&node, 0, 2);
        assert_eq!(paragraphs.len(), 1);
    }

    // ============================================================================
    // create_text_run Tests
    // ============================================================================

    #[test]
    fn test_create_text_run_plain() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Plain text".to_string()),
            marks: vec![],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_bold() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Bold text".to_string()),
            marks: vec![TiptapMark {
                mark_type: "bold".to_string(),
                attrs: None,
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_italic() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Italic text".to_string()),
            marks: vec![TiptapMark {
                mark_type: "italic".to_string(),
                attrs: None,
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_strike() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Strike text".to_string()),
            marks: vec![TiptapMark {
                mark_type: "strike".to_string(),
                attrs: None,
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_underline() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Underlined text".to_string()),
            marks: vec![TiptapMark {
                mark_type: "underline".to_string(),
                attrs: None,
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_code() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("code".to_string()),
            marks: vec![TiptapMark {
                mark_type: "code".to_string(),
                attrs: None,
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_with_color() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Colored text".to_string()),
            marks: vec![TiptapMark {
                mark_type: "textStyle".to_string(),
                attrs: Some(serde_json::json!({ "color": "#ff0000" })),
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_with_override_color() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Text".to_string()),
            marks: vec![TiptapMark {
                mark_type: "textStyle".to_string(),
                attrs: Some(serde_json::json!({ "color": "#ff0000" })),
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, Some("000000"));
    }

    #[test]
    fn test_create_text_run_with_highlight() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Highlighted".to_string()),
            marks: vec![TiptapMark {
                mark_type: "highlight".to_string(),
                attrs: Some(serde_json::json!({ "color": "#ffff00" })),
            }],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    #[test]
    fn test_create_text_run_with_default_size() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Text".to_string()),
            marks: vec![],
            attrs: None,
        };
        let _run = create_text_run(&node, Some(32), None);
    }

    #[test]
    fn test_create_text_run_empty_text() {
        let node = TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: None,
            marks: vec![],
            attrs: None,
        };
        let _run = create_text_run(&node, None, None);
    }

    // ============================================================================
    // process_text_nodes Tests
    // ============================================================================

    #[test]
    fn test_process_text_nodes_empty() {
        let nodes: Vec<TiptapNode> = vec![];
        let runs = process_text_nodes(&nodes, None, None);
        assert_eq!(runs.len(), 1); // Empty placeholder
    }

    #[test]
    fn test_process_text_nodes_single() {
        let nodes = vec![TiptapNode {
            node_type: "text".to_string(),
            content: vec![],
            text: Some("Hello".to_string()),
            marks: vec![],
            attrs: None,
        }];
        let runs = process_text_nodes(&nodes, None, None);
        assert_eq!(runs.len(), 1);
    }

    #[test]
    fn test_process_text_nodes_multiple() {
        let nodes = vec![
            TiptapNode {
                node_type: "text".to_string(),
                content: vec![],
                text: Some("Hello ".to_string()),
                marks: vec![],
                attrs: None,
            },
            TiptapNode {
                node_type: "text".to_string(),
                content: vec![],
                text: Some("world".to_string()),
                marks: vec![TiptapMark {
                    mark_type: "bold".to_string(),
                    attrs: None,
                }],
                attrs: None,
            },
        ];
        let runs = process_text_nodes(&nodes, None, None);
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn test_process_text_nodes_filters_non_text() {
        let nodes = vec![
            TiptapNode {
                node_type: "text".to_string(),
                content: vec![],
                text: Some("Text".to_string()),
                marks: vec![],
                attrs: None,
            },
            TiptapNode {
                node_type: "hardBreak".to_string(),
                content: vec![],
                text: None,
                marks: vec![],
                attrs: None,
            },
        ];
        let runs = process_text_nodes(&nodes, None, None);
        assert_eq!(runs.len(), 1);
    }

    // ============================================================================
    // tiptap_to_docx Integration Tests
    // ============================================================================

    #[test]
    fn test_tiptap_to_docx_empty_document() {
        use std::cell::RefCell;
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![],
        };
        let progress_updates = RefCell::new(vec![]);
        let result = tiptap_to_docx(&doc, |p| progress_updates.borrow_mut().push(p.clone()));
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        // Progress should have been reported
        assert!(!progress_updates.borrow().is_empty());
    }

    #[test]
    fn test_tiptap_to_docx_single_paragraph() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![TiptapNode {
                node_type: "paragraph".to_string(),
                content: vec![TiptapNode {
                    node_type: "text".to_string(),
                    content: vec![],
                    text: Some("Hello world".to_string()),
                    marks: vec![],
                    attrs: None,
                }],
                text: None,
                marks: vec![],
                attrs: None,
            }],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok());
    }

    #[test]
    fn test_tiptap_to_docx_with_heading() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![TiptapNode {
                node_type: "heading".to_string(),
                content: vec![TiptapNode {
                    node_type: "text".to_string(),
                    content: vec![],
                    text: Some("Title".to_string()),
                    marks: vec![],
                    attrs: None,
                }],
                text: None,
                marks: vec![],
                attrs: Some(serde_json::json!({ "level": 1 })),
            }],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok());
    }

    #[test]
    fn test_tiptap_to_docx_with_bullet_list() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![TiptapNode {
                node_type: "bulletList".to_string(),
                content: vec![TiptapNode {
                    node_type: "listItem".to_string(),
                    content: vec![TiptapNode {
                        node_type: "paragraph".to_string(),
                        content: vec![TiptapNode {
                            node_type: "text".to_string(),
                            content: vec![],
                            text: Some("Item".to_string()),
                            marks: vec![],
                            attrs: None,
                        }],
                        text: None,
                        marks: vec![],
                        attrs: None,
                    }],
                    text: None,
                    marks: vec![],
                    attrs: None,
                }],
                text: None,
                marks: vec![],
                attrs: None,
            }],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok());
    }

    #[test]
    fn test_tiptap_to_docx_with_ordered_list() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![TiptapNode {
                node_type: "orderedList".to_string(),
                content: vec![TiptapNode {
                    node_type: "listItem".to_string(),
                    content: vec![TiptapNode {
                        node_type: "paragraph".to_string(),
                        content: vec![],
                        text: None,
                        marks: vec![],
                        attrs: None,
                    }],
                    text: None,
                    marks: vec![],
                    attrs: None,
                }],
                text: None,
                marks: vec![],
                attrs: None,
            }],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok());
    }

    #[test]
    fn test_tiptap_to_docx_with_image() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![TiptapNode {
                node_type: "image".to_string(),
                content: vec![],
                text: None,
                marks: vec![],
                attrs: Some(serde_json::json!({
                    "src": "midlight://img-123"
                })),
            }],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok());
    }

    #[test]
    fn test_tiptap_to_docx_with_horizontal_rule() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![TiptapNode {
                node_type: "horizontalRule".to_string(),
                content: vec![],
                text: None,
                marks: vec![],
                attrs: None,
            }],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok());
    }

    #[test]
    fn test_tiptap_to_docx_unknown_node_type() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![TiptapNode {
                node_type: "unknownType".to_string(),
                content: vec![],
                text: None,
                marks: vec![],
                attrs: None,
            }],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok()); // Should skip unknown types gracefully
    }

    #[test]
    fn test_tiptap_to_docx_progress_callback() {
        use std::cell::RefCell;
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![
                TiptapNode {
                    node_type: "paragraph".to_string(),
                    content: vec![],
                    text: None,
                    marks: vec![],
                    attrs: None,
                };
                15 // 15 paragraphs
            ],
        };
        let progress_updates = RefCell::new(vec![]);
        let result = tiptap_to_docx(&doc, |p| progress_updates.borrow_mut().push(p.clone()));
        assert!(result.is_ok());
        // Should have multiple progress updates
        assert!(progress_updates.borrow().len() >= 2);
        // Last update should show complete
        assert!(progress_updates
            .borrow()
            .last()
            .unwrap()
            .phase
            .contains("Complete"));
    }

    #[test]
    fn test_tiptap_to_docx_complex_document() {
        let doc = TiptapDocument {
            doc_type: "doc".to_string(),
            content: vec![
                TiptapNode {
                    node_type: "heading".to_string(),
                    content: vec![TiptapNode {
                        node_type: "text".to_string(),
                        content: vec![],
                        text: Some("Document Title".to_string()),
                        marks: vec![],
                        attrs: None,
                    }],
                    text: None,
                    marks: vec![],
                    attrs: Some(serde_json::json!({ "level": 1 })),
                },
                TiptapNode {
                    node_type: "paragraph".to_string(),
                    content: vec![
                        TiptapNode {
                            node_type: "text".to_string(),
                            content: vec![],
                            text: Some("This is ".to_string()),
                            marks: vec![],
                            attrs: None,
                        },
                        TiptapNode {
                            node_type: "text".to_string(),
                            content: vec![],
                            text: Some("bold".to_string()),
                            marks: vec![TiptapMark {
                                mark_type: "bold".to_string(),
                                attrs: None,
                            }],
                            attrs: None,
                        },
                        TiptapNode {
                            node_type: "text".to_string(),
                            content: vec![],
                            text: Some(" and ".to_string()),
                            marks: vec![],
                            attrs: None,
                        },
                        TiptapNode {
                            node_type: "text".to_string(),
                            content: vec![],
                            text: Some("italic".to_string()),
                            marks: vec![TiptapMark {
                                mark_type: "italic".to_string(),
                                attrs: None,
                            }],
                            attrs: None,
                        },
                    ],
                    text: None,
                    marks: vec![],
                    attrs: None,
                },
                TiptapNode {
                    node_type: "bulletList".to_string(),
                    content: vec![
                        TiptapNode {
                            node_type: "listItem".to_string(),
                            content: vec![TiptapNode {
                                node_type: "paragraph".to_string(),
                                content: vec![TiptapNode {
                                    node_type: "text".to_string(),
                                    content: vec![],
                                    text: Some("First item".to_string()),
                                    marks: vec![],
                                    attrs: None,
                                }],
                                text: None,
                                marks: vec![],
                                attrs: None,
                            }],
                            text: None,
                            marks: vec![],
                            attrs: None,
                        },
                        TiptapNode {
                            node_type: "listItem".to_string(),
                            content: vec![TiptapNode {
                                node_type: "paragraph".to_string(),
                                content: vec![TiptapNode {
                                    node_type: "text".to_string(),
                                    content: vec![],
                                    text: Some("Second item".to_string()),
                                    marks: vec![],
                                    attrs: None,
                                }],
                                text: None,
                                marks: vec![],
                                attrs: None,
                            }],
                            text: None,
                            marks: vec![],
                            attrs: None,
                        },
                    ],
                    text: None,
                    marks: vec![],
                    attrs: None,
                },
            ],
        };
        let result = tiptap_to_docx(&doc, |_| {});
        assert!(result.is_ok());
        let bytes = result.unwrap();
        // DOCX files should be ZIP archives, check for ZIP magic bytes
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]); // PK\x03\x04
    }

    // ============================================================================
    // Numbering Tests
    // ============================================================================

    #[test]
    fn test_create_bullet_numbering() {
        let numbering = create_bullet_numbering();
        // Just verify it creates without error
        let _ = numbering;
    }

    #[test]
    fn test_create_ordered_numbering() {
        let numbering = create_ordered_numbering();
        // Just verify it creates without error
        let _ = numbering;
    }

    // ============================================================================
    // Font Fallback Map Tests
    // ============================================================================

    #[test]
    fn test_get_font_fallback_map() {
        let map = get_font_fallback_map();
        assert_eq!(map.get("Inter"), Some(&"Arial"));
        assert_eq!(map.get("Merriweather"), Some(&"Georgia"));
        assert_eq!(map.get("JetBrains Mono"), Some(&"Courier New"));
    }
}
