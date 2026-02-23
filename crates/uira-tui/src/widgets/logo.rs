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
    /// Background color used for alpha compositing
    bg_rgba: image::Rgba<u8>,
    /// Terminal's actual background color detected via OSC 11 query.
    /// When set, this overrides theme.bg for image compositing so the
    /// logo seamlessly blends with the terminal window regardless of theme.
    terminal_bg: Option<image::Rgba<u8>>,
    /// Composited image stored for text-line rendering (halfblock mode)
    composited: Option<image::DynamicImage>,
    /// Original un-composited image for re-compositing with different backgrounds
    original_image: Option<image::DynamicImage>,
    /// Cached text-line rendering (invalidated when dimensions or bg change)
    cached_lines: Option<Vec<ratatui::text::Line<'static>>>,
    cached_lines_dims: Option<(u16, u16, Color)>,
}

#[allow(dead_code)]
impl LogoImage {
    /// Create a new logo image holder (call before event loop)
    pub fn new() -> Self {
        // Detect terminal's actual background color via OSC 11 FIRST,
        // before any other terminal queries (needs raw mode).
        let terminal_bg = query_terminal_bg();

        // Try to detect terminal graphics protocol (Kitty, Sixel, iTerm2).
        // Falls back to halfblocks (unicode ▀▄) which works in all terminals.
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        Self {
            protocol: None,
            picker: Some(picker),
            load_error: None,
            image_pixels: None,
            bg_rgba: terminal_bg.unwrap_or(image::Rgba([0, 0, 0, 255])),
            terminal_bg,
            composited: None,
            original_image: None,
            cached_lines: None,
            cached_lines_dims: None,
        }
    }

    /// Set the background color used to replace the image's own background.
    /// Must be called before `load_from_path` so the loaded image's background
    /// pixels are directly painted with this color.
    ///
    /// If the terminal's actual background was detected via OSC 11, that color
    /// is used instead of the provided theme color.
    pub fn set_background_color(&mut self, color: Color) {
        // If terminal bg was detected, prefer that for image compositing
        if let Some(term_bg) = self.terminal_bg {
            self.bg_rgba = term_bg;
            if let Some(ref mut picker) = self.picker {
                picker.set_background_color(term_bg);
            }
            return;
        }
        let (r, g, b) = match color {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Black => (0, 0, 0),
            Color::Reset => (0, 0, 0),
            _ => (0, 0, 0),
        };
        self.bg_rgba = image::Rgba([r, g, b, 255]);
        if let Some(ref mut picker) = self.picker {
            picker.set_background_color(image::Rgba([r, g, b, 255]));
        }
    }

    /// Load an image from a file path.
    ///
    /// Alpha-composites the image against `bg_rgba` (set via `set_background_color`)
    /// to produce a fully opaque image. This avoids relying on ratatui-image's
    /// internal compositing which can produce incorrect colors.
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
        // Composite transparent pixels against the detected background ourselves,
        // producing a fully opaque image. This bypasses ratatui-image's
        // compositing which produces wrong colors on halfblocks.
        self.original_image = Some(dyn_img.clone());
        let dyn_img = Self::composite_onto_bg(dyn_img, self.bg_rgba);
        self.composited = Some(dyn_img.clone());
        self.cached_lines = None;
        self.cached_lines_dims = None;
        let (pw, ph) = (dyn_img.width(), dyn_img.height());
        self.protocol = Some(picker.new_resize_protocol(dyn_img));
        self.image_pixels = Some((pw, ph));
        self.load_error = None;
        Ok(())
    }

    /// Alpha-composite every pixel against `bg` to produce a fully opaque image.
    /// Transparent pixels become `bg`; semi-transparent pixels blend smoothly.
    fn composite_onto_bg(img: image::DynamicImage, bg: image::Rgba<u8>) -> image::DynamicImage {
        let mut rgba = img.to_rgba8();
        let [br, bg_g, bb, _] = bg.0;
        for pixel in rgba.pixels_mut() {
            let a = pixel[3] as f32 / 255.0;
            if a < 1.0 {
                pixel[0] = (pixel[0] as f32 * a + br as f32 * (1.0 - a)).round() as u8;
                pixel[1] = (pixel[1] as f32 * a + bg_g as f32 * (1.0 - a)).round() as u8;
                pixel[2] = (pixel[2] as f32 * a + bb as f32 * (1.0 - a)).round() as u8;
                pixel[3] = 255;
            }
        }
        image::DynamicImage::ImageRgba8(rgba)
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
        // Use terminal bg if detected, otherwise theme bg, so the logo area
        // seamlessly blends with the terminal window.
        let effective_bg = match self.terminal_bg {
            Some(ref term_bg) => Color::Rgb(term_bg.0[0], term_bg.0[1], term_bg.0[2]),
            None => bg_color,
        };
        let bg_block = Block::default().style(Style::default().bg(effective_bg));
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

    /// Render the logo as text lines using halfblock characters (▀) for scrollable chat integration.
    /// Returns right-aligned image lines + "uira" title + URL, suitable for inclusion in ChatView.
    pub fn render_as_lines(
        &mut self,
        max_width: u16,
        max_height: u16,
        accent_color: Color,
        bg_color: Color,
    ) -> Vec<Line<'static>> {
        if let (Some(cached), Some(dims)) = (&self.cached_lines, self.cached_lines_dims) {
            if dims == (max_width, max_height, bg_color) {
                return cached.clone();
            }
        }

        let lines = self.build_halfblock_lines(max_width, max_height, accent_color, bg_color);
        self.cached_lines = Some(lines.clone());
        self.cached_lines_dims = Some((max_width, max_height, bg_color));
        lines
    }

    fn build_halfblock_lines(
        &self,
        max_width: u16,
        max_height: u16,
        accent_color: Color,
        bg_color: Color,
    ) -> Vec<Line<'static>> {
        // Re-composite the original image against the ChatView's bg color so
        // transparent areas blend seamlessly with the surrounding chat background.
        let chat_bg = match bg_color {
            Color::Rgb(r, g, b) => image::Rgba([r, g, b, 255]),
            _ => self.bg_rgba,
        };
        let img = match self.original_image {
            Some(ref orig) => Self::composite_onto_bg(orig.clone(), chat_bg),
            None => return self.fallback_text_lines(max_width, accent_color),
        };

        let text_height: u16 = 2;
        let img_max_h = max_height.saturating_sub(text_height);
        if img_max_h == 0 || max_width == 0 {
            return self.fallback_text_lines(max_width, accent_color);
        }

        let target_px_h = img_max_h as u32 * 2;
        let target_px_w = max_width as u32;

        let (pw, ph) = (img.width(), img.height());
        let scale_w = target_px_w as f64 / pw as f64;
        let scale_h = target_px_h as f64 / ph as f64;
        let scale = scale_w.min(scale_h);

        let render_w = ((pw as f64 * scale).round() as u32).max(1);
        let render_h = ((ph as f64 * scale).round() as u32).max(1);

        let resized = image::imageops::resize(
            &img.to_rgba8(),
            render_w,
            render_h,
            image::imageops::FilterType::Lanczos3,
        );

        let cell_w = render_w as u16;
        let mut lines: Vec<Line<'static>> = Vec::new();

        let mut y = 0u32;
        while y < render_h {
            let mut spans: Vec<Span<'static>> = Vec::new();

            let pad = max_width.saturating_sub(cell_w) as usize;
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }

            for x in 0..render_w {
                let top = resized.get_pixel(x, y);
                let bottom = if y + 1 < render_h {
                    *resized.get_pixel(x, y + 1)
                } else {
                    chat_bg
                };

                spans.push(Span::styled(
                    "▀",
                    Style::default()
                        .fg(Color::Rgb(top[0], top[1], top[2]))
                        .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
                ));
            }
            lines.push(Line::from(spans));
            y += 2;
        }

        let title = "uira";
        let url = "github.com/junhoyeo/uira";

        let title_pad = " ".repeat((max_width as usize).saturating_sub(title.len()));
        let url_pad = " ".repeat((max_width as usize).saturating_sub(url.len()));

        lines.push(Line::from(vec![
            Span::raw(title_pad),
            Span::styled(
                title,
                Style::default()
                    .fg(accent_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw(url_pad),
            Span::styled(url, Style::default().fg(Color::DarkGray)),
        ]));

        lines
    }

    fn fallback_text_lines(&self, max_width: u16, accent_color: Color) -> Vec<Line<'static>> {
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
        let art_width = ascii_art.first().map(|s| s.chars().count()).unwrap_or(0);

        let mut lines: Vec<Line<'static>> = Vec::new();
        for line in &ascii_art {
            let padding = " ".repeat(
                (max_width as usize)
                    .saturating_sub(art_width)
                    .saturating_div(2),
            );
            lines.push(Line::from(vec![
                Span::raw(padding),
                Span::styled(*line, Style::default().fg(accent_color)),
            ]));
        }
        lines.push(Line::from(""));
        let title_pad = " ".repeat(
            (max_width as usize)
                .saturating_sub(title.len())
                .saturating_div(2),
        );
        lines.push(Line::from(vec![
            Span::raw(title_pad),
            Span::styled(
                title,
                Style::default()
                    .fg(accent_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        let url_pad = " ".repeat(
            (max_width as usize)
                .saturating_sub(url.len())
                .saturating_div(2),
        );
        lines.push(Line::from(vec![
            Span::raw(url_pad),
            Span::styled(url, Style::default().fg(Color::DarkGray)),
        ]));
        lines
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

// ---------------------------------------------------------------------------
// OSC 11 terminal background color detection
// ---------------------------------------------------------------------------

/// Query the terminal's actual background color using the OSC 11 escape sequence.
///
/// Sends `\x1b]11;?\x07` and parses the terminal's response which is typically
/// `\x1b]11;rgb:RRRR/GGGG/BBBB\x07` (4-digit hex per component) or
/// `\x1b]11;rgb:RR/GG/BB\x07` (2-digit hex).
///
/// Must be called before entering the alternate screen buffer and before any
/// other terminal queries. Uses a background thread with timeout to avoid
/// hanging if the terminal doesn't support the query.
fn query_terminal_bg() -> Option<image::Rgba<u8>> {
    use std::io::{Read, Write};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        if crossterm::terminal::enable_raw_mode().is_err() {
            return;
        }

        let result = (|| -> Option<image::Rgba<u8>> {
            let mut stdout = std::io::stdout();
            stdout.write_all(b"\x1b]11;?\x07").ok()?;
            stdout.flush().ok()?;

            let mut response = Vec::with_capacity(64);
            let mut byte = [0u8; 1];

            while let Ok(1) = std::io::stdin().read(&mut byte) {
                response.push(byte[0]);
                // BEL terminator
                if byte[0] == 0x07 {
                    break;
                }
                // ST terminator (\x1b\\)
                if response.len() >= 2 && response.ends_with(b"\x1b\\") {
                    break;
                }
                // Safety limit
                if response.len() > 64 {
                    break;
                }
            }

            parse_osc11_response(&response)
        })();

        let _ = crossterm::terminal::disable_raw_mode();

        if let Some(color) = result {
            let _ = tx.send(color);
        }
    });

    rx.recv_timeout(Duration::from_millis(300)).ok()
}

/// Parse an OSC 11 response to extract the RGB background color.
///
/// Handles both 4-digit (`RRRR/GGGG/BBBB`) and 2-digit (`RR/GG/BB`) hex formats.
fn parse_osc11_response(response: &[u8]) -> Option<image::Rgba<u8>> {
    let s = std::str::from_utf8(response).ok()?;
    let rgb_idx = s.find("rgb:")?;
    let after_rgb = &s[rgb_idx + 4..];

    let parts: Vec<&str> = after_rgb.split('/').take(3).collect();
    if parts.len() < 3 {
        return None;
    }

    let parse_component = |s: &str| -> Option<u8> {
        let hex: String = s.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if hex.is_empty() {
            return None;
        }
        let val = u16::from_str_radix(&hex, 16).ok()?;
        if hex.len() > 2 {
            Some((val >> 8) as u8)
        } else {
            Some(val as u8)
        }
    };

    let r = parse_component(parts[0])?;
    let g = parse_component(parts[1])?;
    let b = parse_component(parts[2])?;

    Some(image::Rgba([r, g, b, 255]))
}
