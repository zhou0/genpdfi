//! Font subsetting module using the subsetter crate.
//!
//! This module provides functionality to create subset fonts that contain only
//! the glyphs actually used in a document, significantly reducing PDF file sizes.

use crate::error::{Error, ErrorKind};
use crate::fonts::GlyphIdMap;
use std::collections::HashSet;
use subsetter::{subset, GlyphRemapper};
use ttf_parser::Face;

/// Result of font subsetting, containing both the subset data and glyph ID mapping.
///
/// This struct is returned by [`subset_font_with_mapping`] and provides:
/// - `data`: The subset font bytes to embed in the PDF
/// - `glyph_id_map`: Mapping from characters to their glyph IDs in the subset
///
/// The glyph ID mapping is essential for correct PDF rendering - it maps each
/// character to the glyph ID it has in the subset font (which differs from
/// the original font's glyph IDs).
#[derive(Debug)]
pub struct SubsetResult {
    /// The subset font data (bytes)
    pub data: Vec<u8>,
    /// Mapping from characters to their glyph IDs in the subset font
    pub glyph_id_map: GlyphIdMap,
}

/// Creates a subset of a font containing only the specified characters.
///
/// # Arguments
/// * `font_data` - The original font file data (TTF/OTF)
/// * `text` - The text containing all characters to include in the subset
///
/// # Returns
/// * `Ok(Vec<u8>)` - The subset font data
/// * `Err(Error)` - If subsetting fails
///
/// # Example
/// ```rust,no_run
/// use genpdfi::subsetting::subset_font;
///
/// let font_data = std::fs::read("font.ttf").unwrap();
/// let text = "Hello World ăâîșț";  // Romanian characters
/// let subset = subset_font(&font_data, text).unwrap();
///
/// // subset now contains a smaller font with only the used glyphs
/// assert!(subset.len() < font_data.len());
/// ```
pub fn subset_font(font_data: &[u8], text: &str) -> Result<Vec<u8>, Error> {
    let face = Face::parse(font_data, 0).map_err(|e| {
        Error::new(
            format!("Failed to parse font: {:?}", e),
            ErrorKind::InvalidFont,
        )
    })?;

    let mut remapper = GlyphRemapper::new();
    remapper.remap(0);

    for ch in text.chars() {
        if let Some(glyph_id) = face.glyph_index(ch) {
            remapper.remap(glyph_id.0);
        }
    }

    let result = subset(font_data, 0, &remapper).map_err(|e| {
        Error::new(
            format!("Font subsetting failed: {:?}", e),
            ErrorKind::InvalidFont,
        )
    })?;

    Ok(result)
}

/// Creates a subset font and returns both the data and glyph ID mapping.
///
/// This is the preferred function for font subsetting as it returns the
/// [`GlyphIdMap`] needed for correct PDF rendering. The mapping tells
/// which glyph ID each character has in the subset font.
///
/// # Arguments
/// * `font_data` - The original font file data (TTF/OTF)
/// * `text` - The text containing all characters to include in the subset
///
/// # Returns
/// * `Ok(SubsetResult)` - The subset font data and glyph ID mapping
/// * `Err(Error)` - If subsetting fails
///
/// # Example
/// ```rust,no_run
/// use genpdfi::subsetting::subset_font_with_mapping;
///
/// let font_data = std::fs::read("font.ttf").unwrap();
/// let text = "Hello World";
/// let result = subset_font_with_mapping(&font_data, text).unwrap();
///
/// // Use result.data for embedding, result.glyph_id_map for rendering
/// assert!(result.data.len() < font_data.len());
/// assert!(!result.glyph_id_map.is_empty());
/// ```
pub fn subset_font_with_mapping(font_data: &[u8], text: &str) -> Result<SubsetResult, Error> {
    let face = Face::parse(font_data, 0).map_err(|e| {
        Error::new(
            format!("Failed to parse font: {:?}", e),
            ErrorKind::InvalidFont,
        )
    })?;

    let mut remapper = GlyphRemapper::new();
    // Always include glyph 0 (.notdef) for missing characters
    remapper.remap(0);

    let mut glyph_id_map = GlyphIdMap::new();

    // Collect unique characters to avoid duplicate mapping
    let unique_chars: HashSet<char> = text.chars().collect();

    for ch in unique_chars {
        if let Some(glyph_id) = face.glyph_index(ch) {
            // Remap the glyph and record the mapping
            let subset_glyph_id = remapper.remap(glyph_id.0);
            glyph_id_map.insert(ch, subset_glyph_id);
        }
    }

    let data = subset(font_data, 0, &remapper).map_err(|e| {
        Error::new(
            format!("Font subsetting failed: {:?}", e),
            ErrorKind::InvalidFont,
        )
    })?;

    Ok(SubsetResult { data, glyph_id_map })
}

/// Collects all unique characters from a string.
///
/// This is useful for determining which characters are actually used
/// in a document before creating a subset.
///
/// # Example
/// ```
/// use genpdfi::subsetting::collect_used_chars;
///
/// let text = "Hello World! Hello again!";
/// let chars = collect_used_chars(text);
/// assert_eq!(chars.len(), 13);  // H, e, l, o, space, W, r, d, !, a, g, i, n
/// ```
pub fn collect_used_chars(text: &str) -> HashSet<char> {
    text.chars().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_used_chars() {
        let text = "Hello World!";
        let chars = collect_used_chars(text);

        assert!(chars.contains(&'H'));
        assert!(chars.contains(&'e'));
        assert!(chars.contains(&' '));
        assert!(chars.contains(&'!'));
        assert_eq!(chars.len(), 9); // H,e,l,o, ,W,r,d,!  (unique chars)
    }

    #[test]
    fn test_collect_used_chars_unicode() {
        let text = "ăâîșț";
        let chars = collect_used_chars(text);

        assert_eq!(chars.len(), 5);
        assert!(chars.contains(&'ă'));
        assert!(chars.contains(&'â'));
        assert!(chars.contains(&'î'));
        assert!(chars.contains(&'ș'));
        assert!(chars.contains(&'ț'));
    }
}
