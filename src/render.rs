//! Low-level PDF rendering utilities.

use std::cell;
use std::io;
use std::ops;
use std::rc;

use crate::error::{Error, ErrorKind};
use crate::fonts;
use crate::style::{Color, LineStyle, Style};
use crate::{Margins, Mm, Position, Size};

#[cfg(feature = "images")]
use crate::{Rotation, Scale};

#[derive(Debug, Clone)]
struct LayerPosition(Position);

impl LayerPosition {
    pub fn from_area(area: &Area<'_>, position: Position) -> Self {
        Self(position + area.origin)
    }
}

#[derive(Debug, Clone)]
struct UserSpacePosition(Position);

impl UserSpacePosition {
    pub fn from_layer(layer: &Layer<'_>, position: LayerPosition) -> Self {
        Self(Position::new(
            position.0.x,
            layer.page.size.height - position.0.y,
        ))
    }
}

impl From<UserSpacePosition> for printpdf::Point {
    fn from(pos: UserSpacePosition) -> printpdf::Point {
        printpdf::Point {
            x: pos.0.x.into(),
            y: pos.0.y.into(),
        }
    }
}

impl ops::Deref for UserSpacePosition {
    type Target = Position;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Renders a PDF document with one or more pages.
pub struct Renderer {
    doc: printpdf::PdfDocument,
    pages: Vec<Page>,
}

impl Renderer {
    /// Creates a new PDF document renderer with one page of the given size and the given title.
    pub fn new(size: impl Into<Size>, title: impl AsRef<str>) -> Result<Renderer, Error> {
        let size = size.into();
        let doc = printpdf::PdfDocument::new(title.as_ref());
        let page = Page::new(size, "Layer 1");
        Ok(Renderer {
            doc,
            pages: vec![page],
        })
    }

    /// Sets the PDF conformance for the generated PDF document.
    pub fn with_conformance(mut self, conformance: printpdf::PdfConformance) -> Self {
        self.doc.metadata.info.conformance = conformance;
        self
    }

    /// Sets the creation date for the generated PDF document.
    pub fn with_creation_date(mut self, date: printpdf::DateTime) -> Self {
        self.doc.metadata.info.creation_date = date;
        self
    }

    /// Sets the modification date for the generated PDF document.
    pub fn with_modification_date(mut self, date: printpdf::DateTime) -> Self {
        self.doc.metadata.info.modification_date = date;
        self
    }

    /// Adds a new page with the given size to the document.
    pub fn add_page(&mut self, size: impl Into<Size>) {
        let size = size.into();
        self.pages.push(Page::new(size, "Layer 1"))
    }

    /// Returns the number of pages in this document.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Returns a page of this document.
    pub fn get_page(&self, idx: usize) -> Option<&Page> {
        self.pages.get(idx)
    }

    /// Returns a mutable reference to a page of this document.
    pub fn get_page_mut(&mut self, idx: usize) -> Option<&mut Page> {
        self.pages.get_mut(idx)
    }

    /// Returns a reference to the first page of this document.
    pub fn first_page(&self) -> &Page {
        &self.pages[0]
    }

    /// Returns the first page of this document.
    pub fn first_page_mut(&mut self) -> &mut Page {
        &mut self.pages[0]
    }

    /// Returns the last page of this document.
    pub fn last_page(&self) -> &Page {
        &self.pages[self.pages.len() - 1]
    }

    /// Returns a mutable reference to the last page of this document.
    pub fn last_page_mut(&mut self) -> &mut Page {
        let idx = self.pages.len() - 1;
        &mut self.pages[idx]
    }

    /// Adds a builtin font to the document and returns its ID.
    pub fn add_builtin_font(
        &mut self,
        builtin: printpdf::BuiltinFont,
    ) -> Result<printpdf::FontId, Error> {
        Ok(printpdf::FontId(format!("Builtin-{}", builtin_to_str(builtin))))
    }

    /// Adds an embedded font to the document and returns its ID.
    pub fn add_embedded_font(&mut self, data: &[u8]) -> Result<printpdf::FontId, Error> {
        let mut warnings = Vec::new();
        if let Some(font) = printpdf::ParsedFont::from_bytes(data, 0, &mut warnings) {
            Ok(self.doc.add_font(&font))
        } else {
            Err(Error::new("Failed to load PDF font", ErrorKind::InvalidFont))
        }
    }

    /// Writes this PDF document to a writer.
    pub fn write(mut self, w: impl io::Write) -> Result<(), Error> {
        for page_data in self.pages {
            let mut ops = Vec::new();
            for layer in page_data.layers.0.into_inner() {
                let layer_data = rc::Rc::try_unwrap(layer).unwrap();
                let layer_id = self.doc.add_layer(&printpdf::Layer::new(&layer_data.name));
                ops.push(printpdf::Op::BeginLayer { layer_id: layer_id.clone() });

                for op in layer_data.ops.into_inner() {
                    match op {
                        MyOp::Pdf(o) => ops.push(o),
                        #[cfg(feature = "images")]
                        MyOp::Image(img, pos, scale, rot, dpi) => {
                            use image::GenericImageView;
                            let (width, height) = img.dimensions();
                            let raw_image = printpdf::RawImage {
                                pixels: printpdf::RawImageData::U8(img.to_rgb8().into_raw()),
                                width: width as usize,
                                height: height as usize,
                                data_format: printpdf::RawImageFormat::RGB8,
                                tag: Vec::new(),
                            };
                            let id = self.doc.add_image(&raw_image);
                            ops.push(printpdf::Op::UseXobject {
                                id,
                                transform: printpdf::XObjectTransform {
                                    translate_x: Some(pos.x.into()),
                                    translate_y: Some(pos.y.into()),
                                    rotate: Some(printpdf::XObjectRotation {
                                        angle_ccw_degrees: rot.degrees,
                                        rotation_center_x: printpdf::Px(width as usize / 2),
                                        rotation_center_y: printpdf::Px(height as usize / 2),
                                    }),
                                    scale_x: Some(scale.x),
                                    scale_y: Some(scale.y),
                                    dpi,
                                },
                            });
                        }
                    }
                }

                ops.push(printpdf::Op::EndLayer { layer_id });
            }
            let page = printpdf::PdfPage::new(
                page_data.size.width.into(),
                page_data.size.height.into(),
                ops,
            );
            self.doc.pages.push(page);
        }
        let mut warnings = Vec::new();
        self.doc.save_writer(&mut io::BufWriter::new(w), &printpdf::PdfSaveOptions::default(), &mut warnings);
        Ok(())
    }
}

fn builtin_to_str(builtin: printpdf::BuiltinFont) -> &'static str {
    match builtin {
        printpdf::BuiltinFont::TimesRoman => "TimesRoman",
        printpdf::BuiltinFont::TimesBold => "TimesBold",
        printpdf::BuiltinFont::TimesItalic => "TimesItalic",
        printpdf::BuiltinFont::TimesBoldItalic => "TimesBoldItalic",
        printpdf::BuiltinFont::Helvetica => "Helvetica",
        printpdf::BuiltinFont::HelveticaBold => "HelveticaBold",
        printpdf::BuiltinFont::HelveticaOblique => "HelveticaOblique",
        printpdf::BuiltinFont::HelveticaBoldOblique => "HelveticaBoldOblique",
        printpdf::BuiltinFont::Courier => "Courier",
        printpdf::BuiltinFont::CourierBold => "CourierBold",
        printpdf::BuiltinFont::CourierOblique => "CourierOblique",
        printpdf::BuiltinFont::CourierBoldOblique => "CourierBoldOblique",
        printpdf::BuiltinFont::Symbol => "Symbol",
        printpdf::BuiltinFont::ZapfDingbats => "ZapfDingbats",
    }
}

fn str_to_builtin(s: &str) -> printpdf::BuiltinFont {
    match s {
        "TimesRoman" => printpdf::BuiltinFont::TimesRoman,
        "TimesBold" => printpdf::BuiltinFont::TimesBold,
        "TimesItalic" => printpdf::BuiltinFont::TimesItalic,
        "TimesBoldItalic" => printpdf::BuiltinFont::TimesBoldItalic,
        "Helvetica" => printpdf::BuiltinFont::Helvetica,
        "HelveticaBold" => printpdf::BuiltinFont::HelveticaBold,
        "HelveticaOblique" => printpdf::BuiltinFont::HelveticaOblique,
        "HelveticaBoldOblique" => printpdf::BuiltinFont::HelveticaBoldOblique,
        "Courier" => printpdf::BuiltinFont::Courier,
        "CourierBold" => printpdf::BuiltinFont::CourierBold,
        "CourierOblique" => printpdf::BuiltinFont::CourierOblique,
        "CourierBoldOblique" => printpdf::BuiltinFont::CourierBoldOblique,
        "Symbol" => printpdf::BuiltinFont::Symbol,
        "ZapfDingbats" => printpdf::BuiltinFont::ZapfDingbats,
        _ => printpdf::BuiltinFont::Helvetica,
    }
}

enum MyOp {
    Pdf(printpdf::Op),
    #[cfg(feature = "images")]
    Image(image::DynamicImage, UserSpacePosition, Scale, Rotation, Option<f32>),
}

/// A page of a PDF document.
pub struct Page {
    size: Size,
    layers: Layers,
}

impl Page {
    fn new(size: Size, layer_name: &str) -> Page {
        Page {
            size,
            layers: Layers::new(layer_name),
        }
    }
    /// Adds a new layer with the given name to the page.
    pub fn add_layer(&mut self, name: impl Into<String>) {
        self.layers.push(name.into());
    }
    /// Returns the number of layers on this page.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
    /// Returns a layer of this page.
    pub fn get_layer(&self, idx: usize) -> Option<Layer<'_>> {
        self.layers.get(idx).map(|l| Layer::new(self, l))
    }
    /// Returns the first layer of this page.
    pub fn first_layer(&self) -> Layer<'_> {
        Layer::new(self, self.layers.first())
    }
    /// Returns the last layer of this page.
    pub fn last_layer(&self) -> Layer<'_> {
        Layer::new(self, self.layers.last())
    }
    fn next_layer(&self, data: &rc::Rc<LayerData>) -> Layer<'_> {
        let layer = self.layers.next(data).unwrap_or_else(|| {
            self.layers.push(format!("Layer {}", self.layers.len() + 1))
        });
        Layer::new(self, layer)
    }
}

#[derive(Debug)]
struct Layers(cell::RefCell<Vec<rc::Rc<LayerData>>>);

impl Layers {
    pub fn new(name: &str) -> Self {
        Self(vec![LayerData::new(name).into()].into())
    }
    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }
    pub fn first(&self) -> rc::Rc<LayerData> {
        self.0.borrow().first().unwrap().clone()
    }
    pub fn last(&self) -> rc::Rc<LayerData> {
        self.0.borrow().last().unwrap().clone()
    }
    pub fn get(&self, idx: usize) -> Option<rc::Rc<LayerData>> {
        self.0.borrow().get(idx).cloned()
    }
    pub fn push(&self, name: String) -> rc::Rc<LayerData> {
        let layer_data = rc::Rc::from(LayerData::new(&name));
        self.0.borrow_mut().push(layer_data.clone());
        layer_data
    }
    pub fn next(&self, data: &rc::Rc<LayerData>) -> Option<rc::Rc<LayerData>> {
        let borrow = self.0.borrow();
        let idx = borrow.iter().position(|l| rc::Rc::ptr_eq(l, data))?;
        borrow.get(idx + 1).cloned()
    }
}

/// A layer of a page of a PDF document.
#[derive(Clone)]
pub struct Layer<'p> {
    page: &'p Page,
    data: rc::Rc<LayerData>,
}

impl<'p> Layer<'p> {
    fn new(page: &'p Page, data: rc::Rc<LayerData>) -> Layer<'p> {
        Layer { page, data }
    }
    /// Returns the next layer of this page.
    pub fn next(&self) -> Layer<'p> {
        self.page.next_layer(&self.data)
    }
    /// Returns a drawable area for this layer.
    pub fn area(&self) -> Area<'p> {
        Area::new(self.clone(), Position::default(), self.page.size)
    }
    fn transform_position(&self, pos: LayerPosition) -> UserSpacePosition {
        UserSpacePosition::from_layer(self, pos)
    }
    fn set_fill_color(&self, color: Option<Color>) {
        if let Some(color) = color {
            self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::SetFillColor { col: color.into() }));
        }
    }
    fn set_outline_color(&self, color: Option<Color>) {
        if let Some(color) = color {
            self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::SetOutlineColor { col: color.into() }));
        }
    }
    fn set_outline_thickness(&self, thickness: Mm) {
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::SetOutlineThickness { pt: printpdf::Pt::from(thickness) }));
    }
    fn set_font(&self, font: &printpdf::FontId, font_size: u8) {
        if font.0.starts_with("Builtin-") {
            let builtin = str_to_builtin(&font.0[8..]);
            self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::SetFontSizeBuiltinFont { size: printpdf::Pt(font_size as f32), font: builtin }));
        } else {
            self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::SetFontSize { size: printpdf::Pt(font_size as f32), font: font.clone() }));
        }
    }
    fn add_line_break(&self) {
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::AddLineBreak));
    }
    fn set_text_cursor(&self, pos: Position) {
        let pos = self.transform_position(LayerPosition(pos));
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::SetTextCursor { pos: printpdf::Point { x: pos.x.into(), y: pos.y.into() } }));
    }
    fn start_text_section(&self) {
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::StartTextSection));
    }
    fn end_text_section(&self) {
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::EndTextSection));
    }
    /// Adds an annotation to the layer.
    pub fn add_annotation(&self, annotation: printpdf::LinkAnnotation) {
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::LinkAnnotation { link: annotation }));
    }
    fn add_line_shape<I>(&self, points: I)
    where
        I: IntoIterator<Item = LayerPosition>,
    {
        let line_points: Vec<_> = points
            .into_iter()
            .map(|pos| printpdf::LinePoint { p: self.transform_position(pos).into(), bezier: false })
            .collect();
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::DrawLine { line: printpdf::Line { points: line_points, is_closed: false } }));
    }
    #[cfg(feature = "images")]
    fn add_image(&self, image: &image::DynamicImage, pos: LayerPosition, scale: Scale, rot: Rotation, dpi: Option<f32>) {
        let pos = self.transform_position(pos);
        self.data.ops.borrow_mut().push(MyOp::Image(image.clone(), pos, scale, rot, dpi));
    }
    fn write_positioned_codepoints<I1, I2, I3>(&self, font: &printpdf::FontId, positions: I1, codepoints: I2, chars: I3)
    where
        I1: IntoIterator<Item = i64>,
        I2: IntoIterator<Item = u16>,
        I3: IntoIterator<Item = char>,
    {
        let cpk: Vec<_> = positions.into_iter().zip(codepoints.into_iter()).zip(chars.into_iter())
            .map(|((p, cp), c)| (p, cp, c))
            .collect();
        self.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::WriteCodepointsWithKerning { font: font.clone(), cpk }));
    }
}

struct LayerData {
    name: String,
    ops: cell::RefCell<Vec<MyOp>>,
}

impl std::fmt::Debug for LayerData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayerData").field("name", &self.name).finish()
    }
}

impl LayerData {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ops: cell::RefCell::new(Vec::new()),
        }
    }
}

/// A drawable area.
#[derive(Clone)]
pub struct Area<'p> {
    layer: Layer<'p>,
    origin: Position,
    size: Size,
}

impl<'p> Area<'p> {
    fn new(layer: Layer<'p>, origin: Position, size: Size) -> Area<'p> {
        Area { layer, origin, size }
    }
    /// Returns the size of this area.
    pub fn size(&self) -> Size {
        self.size
    }
    /// Returns the layer this area belongs to.
    pub fn layer(&self) -> &Layer<'p> {
        &self.layer
    }
    /// Returns the origin of this area relative to the top left corner of the layer.
    pub fn origin(&self) -> Position {
        self.origin
    }
    /// Returns a sub-area of this area.
    pub fn sub_area(&self, margins: impl Into<Margins>) -> Area<'p> {
        let margins = margins.into();
        Area::new(
            self.layer.clone(),
            self.origin + Position::new(margins.left, margins.top),
            Size::new(
                self.size.width - margins.left - margins.right,
                self.size.height - margins.top - margins.bottom,
            ),
        )
    }
    /// Adds margins to the area.
    pub fn add_margins(&mut self, margins: impl Into<Margins>) {
        let margins = margins.into();
        self.origin += Position::new(margins.left, margins.top);
        self.size.width -= margins.left + margins.right;
        self.size.height -= margins.top + margins.bottom;
    }
    /// Adds an offset to the area.
    pub fn add_offset(&mut self, offset: impl Into<Position>) {
        let offset = offset.into();
        self.origin += offset;
        self.size.width -= offset.x;
        self.size.height -= offset.y;
    }
    /// Sets the height of the area.
    pub fn set_height(&mut self, height: impl Into<Mm>) {
        self.size.height = height.into();
    }
    /// Sets the fill color.
    pub fn set_fill_color(&self, color: impl Into<Option<Color>>) {
        self.layer.set_fill_color(color.into());
    }
    /// Sets the outline color.
    pub fn set_outline_color(&self, color: impl Into<Option<Color>>) {
        self.layer.set_outline_color(color.into());
    }
    /// Sets the outline thickness.
    pub fn set_outline_thickness(&self, thickness: impl Into<Mm>) {
        self.layer.set_outline_thickness(thickness.into());
    }
    /// Draws a line.
    pub fn draw_line(&self, points: Vec<Position>, style: LineStyle) {
        self.set_outline_color(style.color());
        self.set_outline_thickness(style.thickness());
        self.layer.add_line_shape(points.into_iter().map(|p| LayerPosition::from_area(self, p)));
    }
    /// Draws a rectangle.
    pub fn draw_rect(&self, size: Size, position: Position, style: LineStyle) {
        self.draw_line(
            vec![
                position,
                position + Position::new(size.width, 0),
                position + Position::new(size.width, size.height),
                position + Position::new(0, size.height),
                position,
            ],
            style,
        );
    }
    /// Adds an image to the area.
    #[cfg(feature = "images")]
    pub fn add_image(&self, image: &image::DynamicImage, pos: Position, scale: Scale, rot: Rotation, dpi: Option<f32>) {
        self.layer.add_image(image, LayerPosition::from_area(self, pos), scale, rot, dpi);
    }
    /// Returns a new text section for this area.
    pub fn text_section<'a>(&'a self, font_cache: &'a fonts::FontCache, _position: Position, _metrics: fonts::Metrics) -> Option<TextSection<'a, 'p>> where 'a: 'p {
        Some(TextSection::new(self, font_cache))
    }
    /// Prints a string to the area.
    pub fn print_str(
        &self,
        font_cache: &fonts::FontCache,
        position: Position,
        style: Style,
        s: &str,
    ) -> Result<bool, Error> {
        self.layer.set_text_cursor(self.position(position));
        let mut section = TextSection::new(self, font_cache);
        section.print_str(s, style)?;
        Ok(true)
    }
    /// Splits the area horizontally.
    pub fn split_horizontally(&self, weights: &[usize]) -> Vec<Area<'p>> {
        let total_weight: usize = weights.iter().sum();
        let mut areas = Vec::new();
        let mut x_offset = Mm::default();
        for weight in weights {
            let width = self.size.width * ((*weight as f32) / (total_weight as f32));
            areas.push(Area::new(
                self.layer.clone(),
                self.origin + Position::new(x_offset, Mm::default()),
                Size::new(width, self.size.height),
            ));
            x_offset += width;
        }
        areas
    }
    fn position(&self, position: Position) -> Position {
        self.origin + position
    }
}

/// A section of text.
pub struct TextSection<'f, 'p> {
    area: &'p Area<'p>,
    font_cache: &'f fonts::FontCache,
    metrics: fonts::Metrics,
    current_x_offset: Mm,
    cumulative_kerning: Mm,
    is_first: bool,
}

impl<'f, 'p> TextSection<'f, 'p> {
    fn new(area: &'p Area<'p>, font_cache: &'f fonts::FontCache) -> Self {
        area.layer.start_text_section();
        let font = font_cache.default_font_family().regular;
        let style = Style::new();
        let metrics = font.metrics(style.font_size());
        Self {
            area,
            font_cache,
            metrics,
            current_x_offset: Mm::default(),
            cumulative_kerning: Mm::default(),
            is_first: true,
        }
    }
    /// Sets the text cursor.
    pub fn set_text_cursor(&mut self, offset: impl Into<Mm>) {
        let offset = offset.into();
        self.current_x_offset = offset;
        self.cumulative_kerning = Mm::default();
        self.area.layer.set_text_cursor(self.area.position(Position::new(
            self.current_x_offset + self.cumulative_kerning,
            self.metrics.ascent,
        )));
    }
    /// Adds a line break.
    pub fn add_line_break(&mut self) -> bool {
        if self.metrics.line_height > self.area.size.height {
            false
        } else {
            self.area.layer.add_line_break();
            true
        }
    }
    /// Adds a link.
    pub fn add_link(&mut self, text: impl AsRef<str>, uri: String, style: Style) -> Result<(), Error> {
        let font = style.font(self.font_cache);
        let text = text.as_ref();
        let start_x = self.current_x_offset + self.cumulative_kerning;
        let current_pos = self.area.position(Position::new(start_x, Mm::default()));
        let pdf_pos = self.area.layer.transform_position(LayerPosition(current_pos));
        let text_width = style.text_width(self.font_cache, text);
        let rect = printpdf::Rect {
            x: pdf_pos.x.into(),
            y: printpdf::Pt(pdf_pos.y.0 - font.ascent(style.font_size()).0),
            width: text_width.into(),
            height: printpdf::Pt(font.ascent(style.font_size()).0 - font.descent(style.font_size()).0),
        };
        let annotation = printpdf::LinkAnnotation::new(
            rect,
            printpdf::Actions::uri(uri),
            None,
            None,
            None,
        );
        self.area.layer.add_annotation(annotation);
        self.print_str(text, style)
    }
    /// Prints a string.
    pub fn print_str(&mut self, s: impl AsRef<str>, style: Style) -> Result<(), Error> {
        let font = style.font(self.font_cache);
        let s = s.as_ref();
        if self.is_first {
            if let Some(first_c) = s.chars().next() {
                let x_offset = style.char_left_side_bearing(self.font_cache, first_c) * -1.0;
                self.set_text_cursor(x_offset);
            }
            self.is_first = false;
        }
        let pdf_font = self.font_cache.get_pdf_font(font).expect("Could not find PDF font in font cache");
        self.area.layer.set_fill_color(style.color());
        self.area.layer.set_font(pdf_font, style.font_size());
        let text_width = style.text_width(self.font_cache, s);

        if pdf_font.0.starts_with("Builtin-") {
            let builtin = str_to_builtin(&pdf_font.0[8..]);
            self.area.layer.data.ops.borrow_mut().push(MyOp::Pdf(printpdf::Op::WriteTextBuiltinFont {
                items: vec![printpdf::TextItem::Text(s.to_string())],
                font: builtin,
            }));
        } else {
            let kerning_positions = font.kerning(self.font_cache, s.chars());
            let positions = kerning_positions.into_iter().map(|pos| (-pos * 1000.0) as i64);
            let codepoints = font.glyph_ids(&self.font_cache, s.chars());
            self.area.layer.write_positioned_codepoints(pdf_font, positions, codepoints, s.chars());
        }

        self.current_x_offset += text_width;
        Ok(())
    }
}

impl<'f, 'p> Drop for TextSection<'f, 'p> {
    fn drop(&mut self) {
        self.area.layer.end_text_section();
    }
}
