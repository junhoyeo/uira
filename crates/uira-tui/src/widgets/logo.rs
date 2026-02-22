//! Startup logo image widget

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, Resize, StatefulImage};
use std::path::Path;

/// Logo image state for rendering
pub struct LogoImage {
    protocol: Option<StatefulProtocol>,
    picker: Option<Picker>,
    load_error: Option<String>,
    /// Original pixel dimensions (width, height) of the loaded image
    image_pixels: Option<(u32, u32)>,
}

impl LogoImage {
    /// Create a new logo image holder (call before event loop)
    pub fn new() -> Self {
        // Try to detect terminal graphics protocol (Kitty, Sixel, iTerm2).
        // Falls back to halfblocks (unicode ▀▄) which works in all terminals.
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        Self {
            protocol: None,
            picker: Some(picker),
            load_error: None,
            image_pixels: None,
        }
    }

    /// Set the background color for image transparency compositing.
    /// Must be called before `load_from_path` so the protocol composites
    /// transparent pixels against this color instead of showing black.
    pub fn set_background_color(&mut self, color: Color) {
        if let Some(ref mut picker) = self.picker {
            let (r, g, b) = match color {
                Color::Rgb(r, g, b) => (r, g, b),
                Color::Black => (0, 0, 0),
                Color::Reset => (0, 0, 0),
                _ => (0, 0, 0), // fallback to black for indexed colors
            };
            picker.set_background_color(image::Rgba([r, g, b, 255]));
        }
    }

    /// Load an image from a file path.
    ///
    /// Detects the image's background color by sampling the top-left corner pixel
    /// and makes matching pixels transparent, so the picker's background_color
    /// (set via `set_background_color`) can replace them with the theme background.
    pub fn load_from_path<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let Some(ref picker) = self.picker else {
            let err = "No graphics protocol available".to_string();
            self.load_error = Some(err.clone());
            return Err(err);
        };
        let dyn_img = image::ImageReader::open(path.as_ref())
            .map_err(|e| format!("Failed to open image: {}", e))?
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?;
        // Replace the image's own background with transparency so the picker's
        // background_color (theme bg) shows through instead.
        let dyn_img = Self::replace_bg_with_transparency(dyn_img);
        let (pw, ph) = (dyn_img.width(), dyn_img.height());
        self.protocol = Some(picker.new_resize_protocol(dyn_img));
        self.image_pixels = Some((pw, ph));
        self.load_error = None;
        Ok(())
    }

    /// Sample the top-left corner pixel as the "background" color and replace
    /// all pixels close to it with fully transparent pixels.
    fn replace_bg_with_transparency(img: image::DynamicImage) -> image::DynamicImage {
        use image::DynamicImage;

        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if w == 0 || h == 0 {
            return img;
        }

        // Sample corner pixel as background reference
        let bg_pixel = *rgba.get_pixel(0, 0);
        let (br, bg, bb) = (bg_pixel[0] as i16, bg_pixel[1] as i16, bg_pixel[2] as i16);

        // Tolerance for color distance (handles anti-aliased edges)
        const TOLERANCE: i16 = 30;

        let mut out = rgba;
        for pixel in out.pixels_mut() {
            let (pr, pg, pb) = (pixel[0] as i16, pixel[1] as i16, pixel[2] as i16);
            let dist = (pr - br).abs() + (pg - bg).abs() + (pb - bb).abs();
            if dist < TOLERANCE {
                pixel[3] = 0; // make transparent
            }
        }

        DynamicImage::ImageRgba8(out)
    }

    /// Calculate how many terminal cells the image occupies when fitted into `rect_w × rect_h`.
    /// Returns (cells_wide, cells_tall).
    fn fitted_cell_size(&self, rect_w: u16, rect_h: u16) -> (u16, u16) {
        let Some((pw, ph)) = self.image_pixels else {
            return (rect_w, rect_h);
        };
        let Some(ref picker) = self.picker else {
            return (rect_w, rect_h);
        };
        let (fw, fh) = picker.font_size();
        let fw = fw.max(1) as u32;
        let fh = fh.max(1) as u32;

        // Pixels available in the rect
        let avail_px_w = rect_w as u32 * fw;
        let avail_px_h = rect_h as u32 * fh;

        // Scale to fit while maintaining aspect ratio
        let scale_w = avail_px_w as f64 / pw as f64;
        let scale_h = avail_px_h as f64 / ph as f64;
        let scale = scale_w.min(scale_h);

        // Match ratatui-image's internal rounding: .round() for pixels, .ceil() for cells
        let rendered_px_w = (pw as f64 * scale).round() as u32;
        let rendered_px_h = (ph as f64 * scale).round() as u32;

        let cells_w = (rendered_px_w as f32 / fw as f32).ceil().max(1.0) as u16;
        let cells_h = (rendered_px_h as f32 / fh as f32).ceil().max(1.0) as u16;
        (cells_w.min(rect_w), cells_h.min(rect_h))
    }

    /// Render the logo right-aligned in the given area with title text below
    pub fn render(&mut self, frame: &mut Frame, area: Rect, accent_color: Color, bg_color: Color) {
        // Fill entire area with theme background so cells outside the image
        // don't show the terminal's default bg (which may differ from theme.bg)
        let bg_block = Block::default().style(Style::default().bg(bg_color));
        frame.render_widget(bg_block, area);

        let text_height: u16 = 2;

        // Calculate fitted size BEFORE borrowing protocol mutably
        let max_h = area.height.saturating_sub(text_height) / 2;
        let (img_w, img_h) = self.fitted_cell_size(area.width, max_h);

        if let Some(ref mut protocol) = self.protocol {
            let image_widget = StatefulImage::default().resize(Resize::Scale(None));

            let x = area.x + area.width.saturating_sub(img_w);
            let y = area.y;

            let image_rect = Rect::new(x, y, img_w, img_h);
            frame.render_stateful_widget(image_widget, image_rect, protocol);

            let title = "uira";
            let url = "github.com/junhoyeo/uira";
            let text_y = y + img_h;

            let title_pad = " ".repeat((img_w as usize).saturating_sub(title.len()));
            let url_pad = " ".repeat((img_w as usize).saturating_sub(url.len()));

            let lines = vec![
                Line::from(vec![
                    Span::raw(title_pad),
                    Span::styled(
                        title,
                        Style::default()
                            .fg(accent_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw(url_pad),
                    Span::styled(url, Style::default().fg(Color::DarkGray)),
                ]),
            ];

            let text_rect = Rect::new(
                x,
                text_y,
                img_w,
                text_height.min((area.y + area.height).saturating_sub(text_y)),
            );
            frame.render_widget(Paragraph::new(lines), text_rect);
        } else {
            self.render_fallback(frame, area, accent_color);
        }
    }

    /// Render fallback ASCII art when image can't be displayed
    fn render_fallback(&self, frame: &mut Frame, area: Rect, accent_color: Color) {
        let ascii_art = vec![
            "██╗   ██╗██╗██████╗  █████╗ ",
            "██║   ██║██║██╔══██╗██╔══██╗",
            "██║   ██║██║██████╔╝███████║",
            "██║   ██║██║██╔══██╗██╔══██║",
            "╚██████╔╝██║██║  ██║██║  ██║",
            " ╚═════╝ ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝",
        ];

        let title = "uira";
        let url = "github.com/junhoyeo/uira";

        let total_height = ascii_art.len() + 3; // art + blank + title + url
        let art_width = ascii_art.first().map(|s| s.chars().count()).unwrap_or(0);

        let start_y = area.y + area.height.saturating_sub(total_height as u16) / 2;

        let mut lines: Vec<Line> = Vec::new();

        for line in &ascii_art {
            let padding = " ".repeat(
                (area.width as usize)
                    .saturating_sub(art_width)
                    .saturating_div(2),
            );
            lines.push(Line::from(vec![
                Span::raw(padding),
                Span::styled(*line, Style::default().fg(accent_color)),
            ]));
        }

        lines.push(Line::from(""));

        // Title centered
        let title_padding = " ".repeat(
            (area.width as usize)
                .saturating_sub(title.len())
                .saturating_div(2),
        );
        lines.push(Line::from(vec![
            Span::raw(title_padding),
            Span::styled(
                title,
                Style::default()
                    .fg(accent_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // URL centered
        let url_padding = " ".repeat(
            (area.width as usize)
                .saturating_sub(url.len())
                .saturating_div(2),
        );
        lines.push(Line::from(vec![
            Span::raw(url_padding),
            Span::styled(url, Style::default().fg(Color::DarkGray)),
        ]));

        let centered_area = Rect::new(area.x, start_y, area.width, total_height as u16 + 1);
        frame.render_widget(Paragraph::new(lines), centered_area);
    }
}

impl Default for LogoImage {
    fn default() -> Self {
        Self::new()
    }
}
