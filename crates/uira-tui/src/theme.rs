use ratatui::style::Color;

const BUILTIN_THEMES: [&str; 19] = [
    "default",
    "dark",
    "light",
    "dracula",
    "nord",
    "catppuccin-mocha",
    "catppuccin-latte",
    "gruvbox-dark",
    "gruvbox-light",
    "tokyonight",
    "rosepine",
    "solarized-dark",
    "solarized-light",
    "monokai",
    "kanagawa",
    "github-dark",
    "github-light",
    "aura",
    "material",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,

    // Core colors (original 7)
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub borders: Color,

    // Background variants
    pub bg_panel: Color,
    pub bg_element: Color,
    pub bg_menu: Color,

    // Border variants
    pub border_active: Color,
    pub border_subtle: Color,

    // Text variants
    pub text_muted: Color,
    pub text_selected: Color,

    // Diff colors
    pub diff_added: Color,
    pub diff_removed: Color,
    pub diff_context: Color,
    pub diff_added_bg: Color,
    pub diff_removed_bg: Color,

    // Markdown colors
    pub md_heading: Color,
    pub md_link: Color,
    pub md_code_fg: Color,
    pub md_code_bg: Color,
    pub md_blockquote: Color,
    pub md_emphasis: Color,
    pub md_strong: Color,

    // Syntax highlighting colors
    pub syntax_comment: Color,
    pub syntax_keyword: Color,
    pub syntax_function: Color,
    pub syntax_variable: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_type: Color,
    pub syntax_operator: Color,
    pub syntax_punctuation: Color,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeOverrides {
    // Core colors
    pub bg: Option<String>,
    pub fg: Option<String>,
    pub accent: Option<String>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub success: Option<String>,
    pub borders: Option<String>,

    // Background variants
    pub bg_panel: Option<String>,
    pub bg_element: Option<String>,
    pub bg_menu: Option<String>,

    // Border variants
    pub border_active: Option<String>,
    pub border_subtle: Option<String>,

    // Text variants
    pub text_muted: Option<String>,
    pub text_selected: Option<String>,

    // Diff colors
    pub diff_added: Option<String>,
    pub diff_removed: Option<String>,
    pub diff_context: Option<String>,
    pub diff_added_bg: Option<String>,
    pub diff_removed_bg: Option<String>,

    // Markdown colors
    pub md_heading: Option<String>,
    pub md_link: Option<String>,
    pub md_code_fg: Option<String>,
    pub md_code_bg: Option<String>,
    pub md_blockquote: Option<String>,
    pub md_emphasis: Option<String>,
    pub md_strong: Option<String>,

    // Syntax highlighting colors
    pub syntax_comment: Option<String>,
    pub syntax_keyword: Option<String>,
    pub syntax_function: Option<String>,
    pub syntax_variable: Option<String>,
    pub syntax_string: Option<String>,
    pub syntax_number: Option<String>,
    pub syntax_type: Option<String>,
    pub syntax_operator: Option<String>,
    pub syntax_punctuation: Option<String>,
}

// ---------------------------------------------------------------------------
// Color helper functions for deriving semantic tokens from base colors
// ---------------------------------------------------------------------------

/// Extract RGB components from a Color, defaulting to (0, 0, 0) for non-RGB.
fn to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::White => (255, 255, 255),
        Color::DarkGray => (64, 64, 64),
        Color::Gray => (128, 128, 128),
        _ => (128, 128, 128),
    }
}

/// Darken a color by `amount` (0.0-1.0). amount=0.1 means 10% darker.
fn darken(color: Color, amount: f32) -> Color {
    let (r, g, b) = to_rgb(color);
    let factor = (1.0 - amount).max(0.0);
    Color::Rgb(
        (r as f32 * factor) as u8,
        (g as f32 * factor) as u8,
        (b as f32 * factor) as u8,
    )
}

/// Lighten a color by `amount` (0.0-1.0). amount=0.1 means 10% lighter.
fn lighten(color: Color, amount: f32) -> Color {
    let (r, g, b) = to_rgb(color);
    Color::Rgb(
        (r as f32 + (255.0 - r as f32) * amount) as u8,
        (g as f32 + (255.0 - g as f32) * amount) as u8,
        (b as f32 + (255.0 - b as f32) * amount) as u8,
    )
}

/// Blend two colors. `ratio` = 0.0 returns `a`, 1.0 returns `b`.
fn blend(a: Color, b: Color, ratio: f32) -> Color {
    let (ar, ag, ab) = to_rgb(a);
    let (br, bg, bb) = to_rgb(b);
    let inv = 1.0 - ratio;
    Color::Rgb(
        (ar as f32 * inv + br as f32 * ratio) as u8,
        (ag as f32 * inv + bg as f32 * ratio) as u8,
        (ab as f32 * inv + bb as f32 * ratio) as u8,
    )
}

impl Theme {
    pub fn available_names() -> &'static [&'static str] {
        &BUILTIN_THEMES
    }

    /// Derive all semantic tokens from the 7 base colors.
    /// Called after constructing the base theme to fill in extended fields.
    fn with_derived_defaults(mut self) -> Self {
        let is_light = brightness(self.bg) >= 0.5;

        // Background variants
        if is_light {
            self.bg_panel = darken(self.bg, 0.04);
            self.bg_element = darken(self.bg, 0.08);
            self.bg_menu = darken(self.bg, 0.06);
        } else {
            self.bg_panel = lighten(self.bg, 0.03);
            self.bg_element = lighten(self.bg, 0.06);
            self.bg_menu = lighten(self.bg, 0.05);
        }

        // Border variants
        self.border_active = self.accent;
        self.border_subtle = blend(self.borders, self.bg, 0.4);

        // Text variants
        self.text_muted = blend(self.fg, self.bg, 0.45);
        self.text_selected = self.fg;

        // Diff colors
        self.diff_added = self.success;
        self.diff_removed = self.error;
        self.diff_context = self.text_muted;
        self.diff_added_bg = blend(self.success, self.bg, 0.85);
        self.diff_removed_bg = blend(self.error, self.bg, 0.85);

        // Markdown colors
        self.md_heading = self.accent;
        self.md_link = self.accent;
        self.md_code_fg = self.fg;
        if is_light {
            self.md_code_bg = darken(self.bg, 0.08);
        } else {
            self.md_code_bg = lighten(self.bg, 0.08);
        }
        self.md_blockquote = self.text_muted;
        self.md_emphasis = self.fg;
        self.md_strong = self.fg;

        // Syntax highlighting colors (sensible derivations)
        self.syntax_comment = self.text_muted;
        self.syntax_keyword = self.accent;
        self.syntax_function = self.accent;
        self.syntax_variable = self.fg;
        self.syntax_string = self.success;
        self.syntax_number = self.warning;
        self.syntax_type = self.warning;
        self.syntax_operator = self.fg;
        self.syntax_punctuation = self.borders;

        self
    }

    /// Construct a base theme with placeholder defaults for extended fields.
    /// `with_derived_defaults()` or explicit values should fill them afterward.
    #[allow(clippy::too_many_arguments)]
    fn base(
        name: &str,
        bg: Color,
        fg: Color,
        accent: Color,
        error: Color,
        warning: Color,
        success: Color,
        borders: Color,
    ) -> Self {
        Self {
            name: name.to_string(),
            bg,
            fg,
            accent,
            error,
            warning,
            success,
            borders,
            // Placeholders -- will be overwritten by with_derived_defaults() or explicit set
            bg_panel: bg,
            bg_element: bg,
            bg_menu: bg,
            border_active: accent,
            border_subtle: borders,
            text_muted: fg,
            text_selected: fg,
            diff_added: success,
            diff_removed: error,
            diff_context: fg,
            diff_added_bg: bg,
            diff_removed_bg: bg,
            md_heading: accent,
            md_link: accent,
            md_code_fg: fg,
            md_code_bg: bg,
            md_blockquote: fg,
            md_emphasis: fg,
            md_strong: fg,
            syntax_comment: borders,
            syntax_keyword: accent,
            syntax_function: accent,
            syntax_variable: fg,
            syntax_string: success,
            syntax_number: warning,
            syntax_type: warning,
            syntax_operator: fg,
            syntax_punctuation: borders,
        }
    }

    pub fn from_name(name: &str) -> Result<Self, String> {
        let normalized = name.trim().to_ascii_lowercase();
        let theme = match normalized.as_str() {
            // -- Original 5 themes (base colors UNCHANGED) --------------------
            "default" => Self::base(
                "default",
                Color::Rgb(24, 25, 38),
                Color::Rgb(205, 214, 244),
                Color::Rgb(137, 180, 250),
                Color::Rgb(243, 139, 168),
                Color::Rgb(249, 226, 175),
                Color::Rgb(166, 227, 161),
                Color::Rgb(108, 112, 134),
            )
            .with_derived_defaults(),

            "dark" => Self::base(
                "dark",
                Color::Rgb(17, 19, 24),
                Color::Rgb(230, 230, 230),
                Color::Rgb(79, 179, 255),
                Color::Rgb(255, 107, 107),
                Color::Rgb(255, 184, 107),
                Color::Rgb(126, 231, 135),
                Color::Rgb(91, 101, 118),
            )
            .with_derived_defaults(),

            "light" => Self::base(
                "light",
                Color::Rgb(247, 245, 239),
                Color::Rgb(45, 42, 38),
                Color::Rgb(0, 92, 197),
                Color::Rgb(215, 58, 73),
                Color::Rgb(154, 103, 0),
                Color::Rgb(34, 134, 58),
                Color::Rgb(140, 133, 122),
            )
            .with_derived_defaults(),

            "dracula" => Self::base(
                "dracula",
                Color::Rgb(40, 42, 54),
                Color::Rgb(248, 248, 242),
                Color::Rgb(189, 147, 249),
                Color::Rgb(255, 85, 85),
                Color::Rgb(241, 250, 140),
                Color::Rgb(80, 250, 123),
                Color::Rgb(98, 114, 164),
            )
            .with_derived_defaults(),

            "nord" => Self::base(
                "nord",
                Color::Rgb(46, 52, 64),
                Color::Rgb(236, 239, 244),
                Color::Rgb(136, 192, 208),
                Color::Rgb(191, 97, 106),
                Color::Rgb(235, 203, 139),
                Color::Rgb(163, 190, 140),
                Color::Rgb(76, 86, 106),
            )
            .with_derived_defaults(),

            // -- Catppuccin Mocha -----------------------------------------
            "catppuccin-mocha" => {
                let mut t = Self::base(
                    "catppuccin-mocha",
                    Color::Rgb(30, 30, 46),
                    Color::Rgb(205, 214, 244),
                    Color::Rgb(137, 180, 250),
                    Color::Rgb(243, 139, 168),
                    Color::Rgb(249, 226, 175),
                    Color::Rgb(166, 227, 161),
                    Color::Rgb(108, 112, 134),
                );
                t.bg_panel = Color::Rgb(24, 24, 37);
                t.bg_element = Color::Rgb(49, 50, 68);
                t.bg_menu = Color::Rgb(30, 30, 46);
                t.border_active = Color::Rgb(137, 180, 250);
                t.border_subtle = Color::Rgb(69, 71, 90);
                t.text_muted = Color::Rgb(147, 153, 178);
                t.text_selected = Color::Rgb(205, 214, 244);
                t.diff_added = Color::Rgb(166, 227, 161);
                t.diff_removed = Color::Rgb(243, 139, 168);
                t.diff_context = Color::Rgb(147, 153, 178);
                t.diff_added_bg = Color::Rgb(28, 40, 30);
                t.diff_removed_bg = Color::Rgb(45, 25, 30);
                t.md_heading = Color::Rgb(203, 166, 247);
                t.md_link = Color::Rgb(137, 180, 250);
                t.md_code_fg = Color::Rgb(205, 214, 244);
                t.md_code_bg = Color::Rgb(24, 24, 37);
                t.md_blockquote = Color::Rgb(147, 153, 178);
                t.md_emphasis = Color::Rgb(242, 205, 205);
                t.md_strong = Color::Rgb(245, 224, 220);
                t.syntax_comment = Color::Rgb(108, 112, 134);
                t.syntax_keyword = Color::Rgb(203, 166, 247);
                t.syntax_function = Color::Rgb(137, 180, 250);
                t.syntax_variable = Color::Rgb(205, 214, 244);
                t.syntax_string = Color::Rgb(166, 227, 161);
                t.syntax_number = Color::Rgb(250, 179, 135);
                t.syntax_type = Color::Rgb(249, 226, 175);
                t.syntax_operator = Color::Rgb(148, 226, 213);
                t.syntax_punctuation = Color::Rgb(147, 153, 178);
                t
            }

            // -- Catppuccin Latte -----------------------------------------
            "catppuccin-latte" => {
                let mut t = Self::base(
                    "catppuccin-latte",
                    Color::Rgb(239, 241, 245),
                    Color::Rgb(76, 79, 105),
                    Color::Rgb(30, 102, 245),
                    Color::Rgb(210, 15, 57),
                    Color::Rgb(223, 142, 29),
                    Color::Rgb(64, 160, 43),
                    Color::Rgb(140, 143, 161),
                );
                t.bg_panel = Color::Rgb(230, 233, 239);
                t.bg_element = Color::Rgb(204, 208, 218);
                t.bg_menu = Color::Rgb(239, 241, 245);
                t.border_active = Color::Rgb(30, 102, 245);
                t.border_subtle = Color::Rgb(188, 192, 204);
                t.text_muted = Color::Rgb(108, 111, 133);
                t.text_selected = Color::Rgb(76, 79, 105);
                t.diff_added = Color::Rgb(64, 160, 43);
                t.diff_removed = Color::Rgb(210, 15, 57);
                t.diff_context = Color::Rgb(108, 111, 133);
                t.diff_added_bg = Color::Rgb(220, 240, 218);
                t.diff_removed_bg = Color::Rgb(245, 218, 222);
                t.md_heading = Color::Rgb(136, 57, 239);
                t.md_link = Color::Rgb(30, 102, 245);
                t.md_code_fg = Color::Rgb(76, 79, 105);
                t.md_code_bg = Color::Rgb(230, 233, 239);
                t.md_blockquote = Color::Rgb(108, 111, 133);
                t.md_emphasis = Color::Rgb(221, 120, 120);
                t.md_strong = Color::Rgb(220, 138, 120);
                t.syntax_comment = Color::Rgb(140, 143, 161);
                t.syntax_keyword = Color::Rgb(136, 57, 239);
                t.syntax_function = Color::Rgb(30, 102, 245);
                t.syntax_variable = Color::Rgb(76, 79, 105);
                t.syntax_string = Color::Rgb(64, 160, 43);
                t.syntax_number = Color::Rgb(254, 100, 11);
                t.syntax_type = Color::Rgb(223, 142, 29);
                t.syntax_operator = Color::Rgb(23, 146, 153);
                t.syntax_punctuation = Color::Rgb(108, 111, 133);
                t
            }

            // -- Gruvbox Dark ---------------------------------------------
            "gruvbox-dark" => {
                let mut t = Self::base(
                    "gruvbox-dark",
                    Color::Rgb(40, 40, 40),
                    Color::Rgb(235, 219, 178),
                    Color::Rgb(131, 165, 152),
                    Color::Rgb(251, 73, 52),
                    Color::Rgb(250, 189, 47),
                    Color::Rgb(184, 187, 38),
                    Color::Rgb(124, 111, 100),
                );
                t.bg_panel = Color::Rgb(29, 32, 33);
                t.bg_element = Color::Rgb(60, 56, 54);
                t.bg_menu = Color::Rgb(50, 48, 47);
                t.border_active = Color::Rgb(131, 165, 152);
                t.border_subtle = Color::Rgb(80, 73, 69);
                t.text_muted = Color::Rgb(168, 153, 132);
                t.text_selected = Color::Rgb(235, 219, 178);
                t.diff_added = Color::Rgb(184, 187, 38);
                t.diff_removed = Color::Rgb(251, 73, 52);
                t.diff_context = Color::Rgb(168, 153, 132);
                t.diff_added_bg = Color::Rgb(50, 48, 30);
                t.diff_removed_bg = Color::Rgb(55, 30, 30);
                t.md_heading = Color::Rgb(254, 128, 25);
                t.md_link = Color::Rgb(131, 165, 152);
                t.md_code_fg = Color::Rgb(235, 219, 178);
                t.md_code_bg = Color::Rgb(29, 32, 33);
                t.md_blockquote = Color::Rgb(168, 153, 132);
                t.md_emphasis = Color::Rgb(211, 134, 155);
                t.md_strong = Color::Rgb(254, 128, 25);
                t.syntax_comment = Color::Rgb(146, 131, 116);
                t.syntax_keyword = Color::Rgb(251, 73, 52);
                t.syntax_function = Color::Rgb(184, 187, 38);
                t.syntax_variable = Color::Rgb(235, 219, 178);
                t.syntax_string = Color::Rgb(184, 187, 38);
                t.syntax_number = Color::Rgb(211, 134, 155);
                t.syntax_type = Color::Rgb(250, 189, 47);
                t.syntax_operator = Color::Rgb(131, 165, 152);
                t.syntax_punctuation = Color::Rgb(168, 153, 132);
                t
            }

            // -- Gruvbox Light --------------------------------------------
            "gruvbox-light" => {
                let mut t = Self::base(
                    "gruvbox-light",
                    Color::Rgb(251, 241, 199),
                    Color::Rgb(60, 56, 54),
                    Color::Rgb(7, 102, 120),
                    Color::Rgb(204, 36, 29),
                    Color::Rgb(215, 153, 33),
                    Color::Rgb(152, 151, 26),
                    Color::Rgb(168, 153, 132),
                );
                t.bg_panel = Color::Rgb(242, 229, 188);
                t.bg_element = Color::Rgb(235, 219, 178);
                t.bg_menu = Color::Rgb(245, 235, 192);
                t.border_active = Color::Rgb(7, 102, 120);
                t.border_subtle = Color::Rgb(213, 196, 161);
                t.text_muted = Color::Rgb(124, 111, 100);
                t.text_selected = Color::Rgb(60, 56, 54);
                t.diff_added = Color::Rgb(152, 151, 26);
                t.diff_removed = Color::Rgb(204, 36, 29);
                t.diff_context = Color::Rgb(124, 111, 100);
                t.diff_added_bg = Color::Rgb(235, 240, 200);
                t.diff_removed_bg = Color::Rgb(245, 218, 215);
                t.md_heading = Color::Rgb(175, 58, 3);
                t.md_link = Color::Rgb(7, 102, 120);
                t.md_code_fg = Color::Rgb(60, 56, 54);
                t.md_code_bg = Color::Rgb(242, 229, 188);
                t.md_blockquote = Color::Rgb(124, 111, 100);
                t.md_emphasis = Color::Rgb(143, 63, 113);
                t.md_strong = Color::Rgb(175, 58, 3);
                t.syntax_comment = Color::Rgb(146, 131, 116);
                t.syntax_keyword = Color::Rgb(204, 36, 29);
                t.syntax_function = Color::Rgb(152, 151, 26);
                t.syntax_variable = Color::Rgb(60, 56, 54);
                t.syntax_string = Color::Rgb(152, 151, 26);
                t.syntax_number = Color::Rgb(143, 63, 113);
                t.syntax_type = Color::Rgb(215, 153, 33);
                t.syntax_operator = Color::Rgb(7, 102, 120);
                t.syntax_punctuation = Color::Rgb(124, 111, 100);
                t
            }

            // -- TokyoNight -----------------------------------------------
            "tokyonight" => {
                let mut t = Self::base(
                    "tokyonight",
                    Color::Rgb(26, 27, 38),
                    Color::Rgb(192, 202, 245),
                    Color::Rgb(122, 162, 247),
                    Color::Rgb(247, 118, 142),
                    Color::Rgb(224, 175, 104),
                    Color::Rgb(158, 206, 106),
                    Color::Rgb(61, 89, 161),
                );
                t.bg_panel = Color::Rgb(22, 22, 30);
                t.bg_element = Color::Rgb(41, 46, 66);
                t.bg_menu = Color::Rgb(30, 31, 42);
                t.border_active = Color::Rgb(122, 162, 247);
                t.border_subtle = Color::Rgb(41, 46, 66);
                t.text_muted = Color::Rgb(86, 95, 137);
                t.text_selected = Color::Rgb(192, 202, 245);
                t.diff_added = Color::Rgb(158, 206, 106);
                t.diff_removed = Color::Rgb(247, 118, 142);
                t.diff_context = Color::Rgb(86, 95, 137);
                t.diff_added_bg = Color::Rgb(28, 42, 30);
                t.diff_removed_bg = Color::Rgb(50, 25, 32);
                t.md_heading = Color::Rgb(122, 162, 247);
                t.md_link = Color::Rgb(115, 218, 202);
                t.md_code_fg = Color::Rgb(192, 202, 245);
                t.md_code_bg = Color::Rgb(22, 22, 30);
                t.md_blockquote = Color::Rgb(86, 95, 137);
                t.md_emphasis = Color::Rgb(187, 154, 247);
                t.md_strong = Color::Rgb(192, 202, 245);
                t.syntax_comment = Color::Rgb(86, 95, 137);
                t.syntax_keyword = Color::Rgb(187, 154, 247);
                t.syntax_function = Color::Rgb(122, 162, 247);
                t.syntax_variable = Color::Rgb(192, 202, 245);
                t.syntax_string = Color::Rgb(158, 206, 106);
                t.syntax_number = Color::Rgb(255, 158, 100);
                t.syntax_type = Color::Rgb(224, 175, 104);
                t.syntax_operator = Color::Rgb(115, 218, 202);
                t.syntax_punctuation = Color::Rgb(86, 95, 137);
                t
            }

            // -- Rose Pine ------------------------------------------------
            "rosepine" | "rose-pine" => {
                let mut t = Self::base(
                    "rosepine",
                    Color::Rgb(25, 23, 36),
                    Color::Rgb(224, 222, 244),
                    Color::Rgb(196, 167, 231),
                    Color::Rgb(235, 111, 146),
                    Color::Rgb(246, 193, 119),
                    Color::Rgb(156, 207, 216),
                    Color::Rgb(110, 106, 134),
                );
                t.bg_panel = Color::Rgb(21, 19, 30);
                t.bg_element = Color::Rgb(38, 35, 58);
                t.bg_menu = Color::Rgb(30, 28, 46);
                t.border_active = Color::Rgb(196, 167, 231);
                t.border_subtle = Color::Rgb(57, 53, 82);
                t.text_muted = Color::Rgb(110, 106, 134);
                t.text_selected = Color::Rgb(224, 222, 244);
                t.diff_added = Color::Rgb(156, 207, 216);
                t.diff_removed = Color::Rgb(235, 111, 146);
                t.diff_context = Color::Rgb(110, 106, 134);
                t.diff_added_bg = Color::Rgb(25, 35, 38);
                t.diff_removed_bg = Color::Rgb(42, 22, 30);
                t.md_heading = Color::Rgb(235, 188, 186);
                t.md_link = Color::Rgb(196, 167, 231);
                t.md_code_fg = Color::Rgb(224, 222, 244);
                t.md_code_bg = Color::Rgb(21, 19, 30);
                t.md_blockquote = Color::Rgb(110, 106, 134);
                t.md_emphasis = Color::Rgb(196, 167, 231);
                t.md_strong = Color::Rgb(235, 188, 186);
                t.syntax_comment = Color::Rgb(110, 106, 134);
                t.syntax_keyword = Color::Rgb(62, 143, 176);
                t.syntax_function = Color::Rgb(235, 188, 186);
                t.syntax_variable = Color::Rgb(224, 222, 244);
                t.syntax_string = Color::Rgb(246, 193, 119);
                t.syntax_number = Color::Rgb(246, 193, 119);
                t.syntax_type = Color::Rgb(156, 207, 216);
                t.syntax_operator = Color::Rgb(62, 143, 176);
                t.syntax_punctuation = Color::Rgb(110, 106, 134);
                t
            }

            // -- Solarized Dark -------------------------------------------
            "solarized-dark" => {
                let mut t = Self::base(
                    "solarized-dark",
                    Color::Rgb(0, 43, 54),
                    Color::Rgb(131, 148, 150),
                    Color::Rgb(38, 139, 210),
                    Color::Rgb(220, 50, 47),
                    Color::Rgb(181, 137, 0),
                    Color::Rgb(133, 153, 0),
                    Color::Rgb(88, 110, 117),
                );
                t.bg_panel = Color::Rgb(0, 34, 43);
                t.bg_element = Color::Rgb(7, 54, 66);
                t.bg_menu = Color::Rgb(0, 43, 54);
                t.border_active = Color::Rgb(38, 139, 210);
                t.border_subtle = Color::Rgb(7, 54, 66);
                t.text_muted = Color::Rgb(88, 110, 117);
                t.text_selected = Color::Rgb(147, 161, 161);
                t.diff_added = Color::Rgb(133, 153, 0);
                t.diff_removed = Color::Rgb(220, 50, 47);
                t.diff_context = Color::Rgb(88, 110, 117);
                t.diff_added_bg = Color::Rgb(10, 48, 30);
                t.diff_removed_bg = Color::Rgb(50, 30, 30);
                t.md_heading = Color::Rgb(203, 75, 22);
                t.md_link = Color::Rgb(38, 139, 210);
                t.md_code_fg = Color::Rgb(131, 148, 150);
                t.md_code_bg = Color::Rgb(7, 54, 66);
                t.md_blockquote = Color::Rgb(88, 110, 117);
                t.md_emphasis = Color::Rgb(108, 113, 196);
                t.md_strong = Color::Rgb(147, 161, 161);
                t.syntax_comment = Color::Rgb(88, 110, 117);
                t.syntax_keyword = Color::Rgb(133, 153, 0);
                t.syntax_function = Color::Rgb(38, 139, 210);
                t.syntax_variable = Color::Rgb(131, 148, 150);
                t.syntax_string = Color::Rgb(42, 161, 152);
                t.syntax_number = Color::Rgb(203, 75, 22);
                t.syntax_type = Color::Rgb(181, 137, 0);
                t.syntax_operator = Color::Rgb(133, 153, 0);
                t.syntax_punctuation = Color::Rgb(88, 110, 117);
                t
            }

            // -- Solarized Light ------------------------------------------
            "solarized-light" => {
                let mut t = Self::base(
                    "solarized-light",
                    Color::Rgb(253, 246, 227),
                    Color::Rgb(101, 123, 131),
                    Color::Rgb(38, 139, 210),
                    Color::Rgb(220, 50, 47),
                    Color::Rgb(181, 137, 0),
                    Color::Rgb(133, 153, 0),
                    Color::Rgb(147, 161, 161),
                );
                t.bg_panel = Color::Rgb(238, 232, 213);
                t.bg_element = Color::Rgb(238, 232, 213);
                t.bg_menu = Color::Rgb(253, 246, 227);
                t.border_active = Color::Rgb(38, 139, 210);
                t.border_subtle = Color::Rgb(238, 232, 213);
                t.text_muted = Color::Rgb(147, 161, 161);
                t.text_selected = Color::Rgb(88, 110, 117);
                t.diff_added = Color::Rgb(133, 153, 0);
                t.diff_removed = Color::Rgb(220, 50, 47);
                t.diff_context = Color::Rgb(147, 161, 161);
                t.diff_added_bg = Color::Rgb(230, 240, 210);
                t.diff_removed_bg = Color::Rgb(250, 225, 225);
                t.md_heading = Color::Rgb(203, 75, 22);
                t.md_link = Color::Rgb(38, 139, 210);
                t.md_code_fg = Color::Rgb(101, 123, 131);
                t.md_code_bg = Color::Rgb(238, 232, 213);
                t.md_blockquote = Color::Rgb(147, 161, 161);
                t.md_emphasis = Color::Rgb(108, 113, 196);
                t.md_strong = Color::Rgb(88, 110, 117);
                t.syntax_comment = Color::Rgb(147, 161, 161);
                t.syntax_keyword = Color::Rgb(133, 153, 0);
                t.syntax_function = Color::Rgb(38, 139, 210);
                t.syntax_variable = Color::Rgb(101, 123, 131);
                t.syntax_string = Color::Rgb(42, 161, 152);
                t.syntax_number = Color::Rgb(203, 75, 22);
                t.syntax_type = Color::Rgb(181, 137, 0);
                t.syntax_operator = Color::Rgb(133, 153, 0);
                t.syntax_punctuation = Color::Rgb(147, 161, 161);
                t
            }

            // -- Monokai --------------------------------------------------
            "monokai" => {
                let mut t = Self::base(
                    "monokai",
                    Color::Rgb(39, 40, 34),
                    Color::Rgb(248, 248, 242),
                    Color::Rgb(102, 217, 239),
                    Color::Rgb(249, 38, 114),
                    Color::Rgb(230, 219, 116),
                    Color::Rgb(166, 226, 46),
                    Color::Rgb(117, 113, 94),
                );
                t.bg_panel = Color::Rgb(32, 33, 27);
                t.bg_element = Color::Rgb(55, 56, 48);
                t.bg_menu = Color::Rgb(45, 46, 38);
                t.border_active = Color::Rgb(102, 217, 239);
                t.border_subtle = Color::Rgb(62, 61, 50);
                t.text_muted = Color::Rgb(117, 113, 94);
                t.text_selected = Color::Rgb(248, 248, 242);
                t.diff_added = Color::Rgb(166, 226, 46);
                t.diff_removed = Color::Rgb(249, 38, 114);
                t.diff_context = Color::Rgb(117, 113, 94);
                t.diff_added_bg = Color::Rgb(40, 50, 25);
                t.diff_removed_bg = Color::Rgb(55, 25, 35);
                t.md_heading = Color::Rgb(249, 38, 114);
                t.md_link = Color::Rgb(102, 217, 239);
                t.md_code_fg = Color::Rgb(248, 248, 242);
                t.md_code_bg = Color::Rgb(32, 33, 27);
                t.md_blockquote = Color::Rgb(117, 113, 94);
                t.md_emphasis = Color::Rgb(253, 151, 31);
                t.md_strong = Color::Rgb(248, 248, 242);
                t.syntax_comment = Color::Rgb(117, 113, 94);
                t.syntax_keyword = Color::Rgb(249, 38, 114);
                t.syntax_function = Color::Rgb(166, 226, 46);
                t.syntax_variable = Color::Rgb(248, 248, 242);
                t.syntax_string = Color::Rgb(230, 219, 116);
                t.syntax_number = Color::Rgb(174, 129, 255);
                t.syntax_type = Color::Rgb(102, 217, 239);
                t.syntax_operator = Color::Rgb(249, 38, 114);
                t.syntax_punctuation = Color::Rgb(248, 248, 242);
                t
            }

            // -- Kanagawa -------------------------------------------------
            "kanagawa" => {
                let mut t = Self::base(
                    "kanagawa",
                    Color::Rgb(31, 31, 40),
                    Color::Rgb(220, 215, 186),
                    Color::Rgb(126, 156, 216),
                    Color::Rgb(195, 64, 67),
                    Color::Rgb(226, 194, 111),
                    Color::Rgb(152, 187, 108),
                    Color::Rgb(84, 84, 109),
                );
                t.bg_panel = Color::Rgb(22, 22, 28);
                t.bg_element = Color::Rgb(42, 42, 56);
                t.bg_menu = Color::Rgb(31, 31, 40);
                t.border_active = Color::Rgb(126, 156, 216);
                t.border_subtle = Color::Rgb(54, 54, 70);
                t.text_muted = Color::Rgb(114, 113, 105);
                t.text_selected = Color::Rgb(220, 215, 186);
                t.diff_added = Color::Rgb(152, 187, 108);
                t.diff_removed = Color::Rgb(195, 64, 67);
                t.diff_context = Color::Rgb(114, 113, 105);
                t.diff_added_bg = Color::Rgb(30, 40, 28);
                t.diff_removed_bg = Color::Rgb(48, 25, 28);
                t.md_heading = Color::Rgb(255, 160, 102);
                t.md_link = Color::Rgb(126, 156, 216);
                t.md_code_fg = Color::Rgb(220, 215, 186);
                t.md_code_bg = Color::Rgb(22, 22, 28);
                t.md_blockquote = Color::Rgb(114, 113, 105);
                t.md_emphasis = Color::Rgb(210, 126, 153);
                t.md_strong = Color::Rgb(255, 160, 102);
                t.syntax_comment = Color::Rgb(114, 113, 105);
                t.syntax_keyword = Color::Rgb(150, 120, 182);
                t.syntax_function = Color::Rgb(126, 156, 216);
                t.syntax_variable = Color::Rgb(220, 215, 186);
                t.syntax_string = Color::Rgb(152, 187, 108);
                t.syntax_number = Color::Rgb(210, 126, 153);
                t.syntax_type = Color::Rgb(122, 168, 159);
                t.syntax_operator = Color::Rgb(192, 163, 110);
                t.syntax_punctuation = Color::Rgb(154, 149, 120);
                t
            }

            // -- GitHub Dark ----------------------------------------------
            "github-dark" => {
                let mut t = Self::base(
                    "github-dark",
                    Color::Rgb(13, 17, 23),
                    Color::Rgb(230, 237, 243),
                    Color::Rgb(88, 166, 255),
                    Color::Rgb(255, 123, 114),
                    Color::Rgb(210, 153, 34),
                    Color::Rgb(63, 185, 80),
                    Color::Rgb(48, 54, 61),
                );
                t.bg_panel = Color::Rgb(1, 4, 9);
                t.bg_element = Color::Rgb(22, 27, 34);
                t.bg_menu = Color::Rgb(22, 27, 34);
                t.border_active = Color::Rgb(88, 166, 255);
                t.border_subtle = Color::Rgb(33, 38, 45);
                t.text_muted = Color::Rgb(125, 133, 144);
                t.text_selected = Color::Rgb(230, 237, 243);
                t.diff_added = Color::Rgb(63, 185, 80);
                t.diff_removed = Color::Rgb(255, 123, 114);
                t.diff_context = Color::Rgb(125, 133, 144);
                t.diff_added_bg = Color::Rgb(18, 36, 20);
                t.diff_removed_bg = Color::Rgb(50, 18, 18);
                t.md_heading = Color::Rgb(88, 166, 255);
                t.md_link = Color::Rgb(88, 166, 255);
                t.md_code_fg = Color::Rgb(230, 237, 243);
                t.md_code_bg = Color::Rgb(22, 27, 34);
                t.md_blockquote = Color::Rgb(125, 133, 144);
                t.md_emphasis = Color::Rgb(230, 237, 243);
                t.md_strong = Color::Rgb(230, 237, 243);
                t.syntax_comment = Color::Rgb(125, 133, 144);
                t.syntax_keyword = Color::Rgb(255, 123, 114);
                t.syntax_function = Color::Rgb(210, 168, 255);
                t.syntax_variable = Color::Rgb(230, 237, 243);
                t.syntax_string = Color::Rgb(165, 214, 255);
                t.syntax_number = Color::Rgb(121, 192, 255);
                t.syntax_type = Color::Rgb(255, 166, 87);
                t.syntax_operator = Color::Rgb(255, 123, 114);
                t.syntax_punctuation = Color::Rgb(125, 133, 144);
                t
            }

            // -- GitHub Light ---------------------------------------------
            "github-light" => {
                let mut t = Self::base(
                    "github-light",
                    Color::Rgb(255, 255, 255),
                    Color::Rgb(31, 35, 40),
                    Color::Rgb(9, 105, 218),
                    Color::Rgb(207, 34, 46),
                    Color::Rgb(154, 103, 0),
                    Color::Rgb(26, 127, 55),
                    Color::Rgb(216, 222, 228),
                );
                t.bg_panel = Color::Rgb(246, 248, 250);
                t.bg_element = Color::Rgb(234, 238, 242);
                t.bg_menu = Color::Rgb(255, 255, 255);
                t.border_active = Color::Rgb(9, 105, 218);
                t.border_subtle = Color::Rgb(234, 238, 242);
                t.text_muted = Color::Rgb(101, 109, 118);
                t.text_selected = Color::Rgb(31, 35, 40);
                t.diff_added = Color::Rgb(26, 127, 55);
                t.diff_removed = Color::Rgb(207, 34, 46);
                t.diff_context = Color::Rgb(101, 109, 118);
                t.diff_added_bg = Color::Rgb(218, 251, 225);
                t.diff_removed_bg = Color::Rgb(255, 218, 218);
                t.md_heading = Color::Rgb(9, 105, 218);
                t.md_link = Color::Rgb(9, 105, 218);
                t.md_code_fg = Color::Rgb(31, 35, 40);
                t.md_code_bg = Color::Rgb(246, 248, 250);
                t.md_blockquote = Color::Rgb(101, 109, 118);
                t.md_emphasis = Color::Rgb(31, 35, 40);
                t.md_strong = Color::Rgb(31, 35, 40);
                t.syntax_comment = Color::Rgb(101, 109, 118);
                t.syntax_keyword = Color::Rgb(207, 34, 46);
                t.syntax_function = Color::Rgb(130, 80, 223);
                t.syntax_variable = Color::Rgb(31, 35, 40);
                t.syntax_string = Color::Rgb(10, 48, 105);
                t.syntax_number = Color::Rgb(10, 48, 105);
                t.syntax_type = Color::Rgb(154, 103, 0);
                t.syntax_operator = Color::Rgb(207, 34, 46);
                t.syntax_punctuation = Color::Rgb(101, 109, 118);
                t
            }

            // -- Aura -----------------------------------------------------
            "aura" => {
                let mut t = Self::base(
                    "aura",
                    Color::Rgb(21, 20, 27),
                    Color::Rgb(237, 236, 238),
                    Color::Rgb(163, 128, 255),
                    Color::Rgb(255, 102, 102),
                    Color::Rgb(255, 202, 99),
                    Color::Rgb(97, 255, 202),
                    Color::Rgb(110, 108, 126),
                );
                t.bg_panel = Color::Rgb(16, 15, 21);
                t.bg_element = Color::Rgb(36, 34, 47);
                t.bg_menu = Color::Rgb(28, 27, 37);
                t.border_active = Color::Rgb(163, 128, 255);
                t.border_subtle = Color::Rgb(44, 42, 58);
                t.text_muted = Color::Rgb(110, 108, 126);
                t.text_selected = Color::Rgb(237, 236, 238);
                t.diff_added = Color::Rgb(97, 255, 202);
                t.diff_removed = Color::Rgb(255, 102, 102);
                t.diff_context = Color::Rgb(110, 108, 126);
                t.diff_added_bg = Color::Rgb(20, 35, 30);
                t.diff_removed_bg = Color::Rgb(45, 20, 22);
                t.md_heading = Color::Rgb(163, 128, 255);
                t.md_link = Color::Rgb(97, 255, 202);
                t.md_code_fg = Color::Rgb(237, 236, 238);
                t.md_code_bg = Color::Rgb(16, 15, 21);
                t.md_blockquote = Color::Rgb(110, 108, 126);
                t.md_emphasis = Color::Rgb(255, 148, 214);
                t.md_strong = Color::Rgb(237, 236, 238);
                t.syntax_comment = Color::Rgb(110, 108, 126);
                t.syntax_keyword = Color::Rgb(163, 128, 255);
                t.syntax_function = Color::Rgb(255, 202, 99);
                t.syntax_variable = Color::Rgb(237, 236, 238);
                t.syntax_string = Color::Rgb(97, 255, 202);
                t.syntax_number = Color::Rgb(130, 233, 255);
                t.syntax_type = Color::Rgb(130, 233, 255);
                t.syntax_operator = Color::Rgb(163, 128, 255);
                t.syntax_punctuation = Color::Rgb(110, 108, 126);
                t
            }

            // -- Material -------------------------------------------------
            "material" => {
                let mut t = Self::base(
                    "material",
                    Color::Rgb(38, 50, 56),
                    Color::Rgb(176, 190, 197),
                    Color::Rgb(130, 170, 255),
                    Color::Rgb(240, 113, 120),
                    Color::Rgb(255, 203, 107),
                    Color::Rgb(195, 232, 141),
                    Color::Rgb(55, 71, 79),
                );
                t.bg_panel = Color::Rgb(30, 40, 45);
                t.bg_element = Color::Rgb(50, 62, 68);
                t.bg_menu = Color::Rgb(42, 54, 60);
                t.border_active = Color::Rgb(130, 170, 255);
                t.border_subtle = Color::Rgb(55, 71, 79);
                t.text_muted = Color::Rgb(96, 125, 139);
                t.text_selected = Color::Rgb(176, 190, 197);
                t.diff_added = Color::Rgb(195, 232, 141);
                t.diff_removed = Color::Rgb(240, 113, 120);
                t.diff_context = Color::Rgb(96, 125, 139);
                t.diff_added_bg = Color::Rgb(32, 48, 30);
                t.diff_removed_bg = Color::Rgb(50, 30, 32);
                t.md_heading = Color::Rgb(130, 170, 255);
                t.md_link = Color::Rgb(137, 221, 255);
                t.md_code_fg = Color::Rgb(176, 190, 197);
                t.md_code_bg = Color::Rgb(30, 40, 45);
                t.md_blockquote = Color::Rgb(96, 125, 139);
                t.md_emphasis = Color::Rgb(199, 146, 234);
                t.md_strong = Color::Rgb(176, 190, 197);
                t.syntax_comment = Color::Rgb(84, 110, 122);
                t.syntax_keyword = Color::Rgb(199, 146, 234);
                t.syntax_function = Color::Rgb(130, 170, 255);
                t.syntax_variable = Color::Rgb(176, 190, 197);
                t.syntax_string = Color::Rgb(195, 232, 141);
                t.syntax_number = Color::Rgb(247, 140, 108);
                t.syntax_type = Color::Rgb(255, 203, 107);
                t.syntax_operator = Color::Rgb(137, 221, 255);
                t.syntax_punctuation = Color::Rgb(137, 221, 255);
                t
            }

            _ => {
                return Err(format!(
                    "Unknown theme '{}' (available: {})",
                    name,
                    Self::available_names().join(", ")
                ));
            }
        };

        Ok(theme)
    }

    pub fn from_name_with_overrides(
        name: &str,
        overrides: &ThemeOverrides,
    ) -> Result<Self, String> {
        let mut theme = Self::from_name(name)?;
        theme.apply_overrides(overrides)?;
        Ok(theme)
    }

    pub fn apply_overrides(&mut self, overrides: &ThemeOverrides) -> Result<(), String> {
        // Core colors
        apply_override(&mut self.bg, &overrides.bg, "bg")?;
        apply_override(&mut self.fg, &overrides.fg, "fg")?;
        apply_override(&mut self.accent, &overrides.accent, "accent")?;
        apply_override(&mut self.error, &overrides.error, "error")?;
        apply_override(&mut self.warning, &overrides.warning, "warning")?;
        apply_override(&mut self.success, &overrides.success, "success")?;
        apply_override(&mut self.borders, &overrides.borders, "borders")?;

        // Background variants
        apply_override(&mut self.bg_panel, &overrides.bg_panel, "bg_panel")?;
        apply_override(&mut self.bg_element, &overrides.bg_element, "bg_element")?;
        apply_override(&mut self.bg_menu, &overrides.bg_menu, "bg_menu")?;

        // Border variants
        apply_override(
            &mut self.border_active,
            &overrides.border_active,
            "border_active",
        )?;
        apply_override(
            &mut self.border_subtle,
            &overrides.border_subtle,
            "border_subtle",
        )?;

        // Text variants
        apply_override(&mut self.text_muted, &overrides.text_muted, "text_muted")?;
        apply_override(
            &mut self.text_selected,
            &overrides.text_selected,
            "text_selected",
        )?;

        // Diff colors
        apply_override(&mut self.diff_added, &overrides.diff_added, "diff_added")?;
        apply_override(
            &mut self.diff_removed,
            &overrides.diff_removed,
            "diff_removed",
        )?;
        apply_override(
            &mut self.diff_context,
            &overrides.diff_context,
            "diff_context",
        )?;
        apply_override(
            &mut self.diff_added_bg,
            &overrides.diff_added_bg,
            "diff_added_bg",
        )?;
        apply_override(
            &mut self.diff_removed_bg,
            &overrides.diff_removed_bg,
            "diff_removed_bg",
        )?;

        // Markdown colors
        apply_override(&mut self.md_heading, &overrides.md_heading, "md_heading")?;
        apply_override(&mut self.md_link, &overrides.md_link, "md_link")?;
        apply_override(&mut self.md_code_fg, &overrides.md_code_fg, "md_code_fg")?;
        apply_override(&mut self.md_code_bg, &overrides.md_code_bg, "md_code_bg")?;
        apply_override(
            &mut self.md_blockquote,
            &overrides.md_blockquote,
            "md_blockquote",
        )?;
        apply_override(&mut self.md_emphasis, &overrides.md_emphasis, "md_emphasis")?;
        apply_override(&mut self.md_strong, &overrides.md_strong, "md_strong")?;

        // Syntax highlighting colors
        apply_override(
            &mut self.syntax_comment,
            &overrides.syntax_comment,
            "syntax_comment",
        )?;
        apply_override(
            &mut self.syntax_keyword,
            &overrides.syntax_keyword,
            "syntax_keyword",
        )?;
        apply_override(
            &mut self.syntax_function,
            &overrides.syntax_function,
            "syntax_function",
        )?;
        apply_override(
            &mut self.syntax_variable,
            &overrides.syntax_variable,
            "syntax_variable",
        )?;
        apply_override(
            &mut self.syntax_string,
            &overrides.syntax_string,
            "syntax_string",
        )?;
        apply_override(
            &mut self.syntax_number,
            &overrides.syntax_number,
            "syntax_number",
        )?;
        apply_override(&mut self.syntax_type, &overrides.syntax_type, "syntax_type")?;
        apply_override(
            &mut self.syntax_operator,
            &overrides.syntax_operator,
            "syntax_operator",
        )?;
        apply_override(
            &mut self.syntax_punctuation,
            &overrides.syntax_punctuation,
            "syntax_punctuation",
        )?;

        Ok(())
    }

    pub fn contrast_text(color: Color) -> Color {
        if brightness(color) >= 0.5 {
            Color::Black
        } else {
            Color::White
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_name("default").expect("default theme must exist")
    }
}

fn apply_override(target: &mut Color, value: &Option<String>, field: &str) -> Result<(), String> {
    if let Some(raw) = value {
        let parsed =
            parse_hex_color(raw).map_err(|err| format!("invalid {} color: {}", field, err))?;
        *target = parsed;
    }
    Ok(())
}

fn parse_hex_color(raw: &str) -> Result<Color, String> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);

    if hex.len() != 6 {
        return Err(format!("expected 6 hex digits, got '{}'", raw));
    }

    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("expected hexadecimal color, got '{}'", raw));
    }

    let r = u8::from_str_radix(&hex[0..2], 16)
        .map_err(|_| format!("failed to parse red in '{}'", raw))?;
    let g = u8::from_str_radix(&hex[2..4], 16)
        .map_err(|_| format!("failed to parse green in '{}'", raw))?;
    let b = u8::from_str_radix(&hex[4..6], 16)
        .map_err(|_| format!("failed to parse blue in '{}'", raw))?;

    Ok(Color::Rgb(r, g, b))
}

fn brightness(color: Color) -> f32 {
    match color {
        Color::Black => 0.0,
        Color::DarkGray => 0.25,
        Color::Gray => 0.5,
        Color::White => 1.0,
        Color::Rgb(r, g, b) => (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0,
        Color::Indexed(v) => v as f32 / 255.0,
        Color::Reset => 0.0,
        _ => 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_themes_available() {
        let names = Theme::available_names();
        // Original 5 must still be present
        assert!(names.contains(&"default"));
        assert!(names.contains(&"dark"));
        assert!(names.contains(&"light"));
        assert!(names.contains(&"dracula"));
        assert!(names.contains(&"nord"));
        // New themes
        assert!(names.contains(&"catppuccin-mocha"));
        assert!(names.contains(&"tokyonight"));
        assert!(names.contains(&"gruvbox-dark"));
        assert!(names.contains(&"github-dark"));
        assert!(names.len() >= 19);
    }

    #[test]
    fn test_theme_name_is_case_insensitive() {
        let theme = Theme::from_name("DrAcUlA").expect("dracula should resolve");
        assert_eq!(theme.name, "dracula");
    }

    #[test]
    fn test_apply_custom_overrides() {
        let mut theme = Theme::default();
        let overrides = ThemeOverrides {
            bg: Some("#112233".to_string()),
            accent: Some("AABBCC".to_string()),
            ..Default::default()
        };

        theme
            .apply_overrides(&overrides)
            .expect("overrides should parse");

        assert_eq!(theme.bg, Color::Rgb(17, 34, 51));
        assert_eq!(theme.accent, Color::Rgb(170, 187, 204));
    }

    #[test]
    fn test_invalid_custom_color_returns_error() {
        let mut theme = Theme::default();
        let overrides = ThemeOverrides {
            warning: Some("bad".to_string()),
            ..Default::default()
        };

        let err = theme
            .apply_overrides(&overrides)
            .expect_err("invalid color should fail");
        assert!(err.contains("invalid warning color"));
    }

    /// Verify that existing 5 themes produce identical base 7 colors.
    #[test]
    fn test_original_themes_base_colors_unchanged() {
        let default_theme = Theme::from_name("default").unwrap();
        assert_eq!(default_theme.bg, Color::Rgb(24, 25, 38));
        assert_eq!(default_theme.fg, Color::Rgb(205, 214, 244));
        assert_eq!(default_theme.accent, Color::Rgb(137, 180, 250));
        assert_eq!(default_theme.error, Color::Rgb(243, 139, 168));
        assert_eq!(default_theme.warning, Color::Rgb(249, 226, 175));
        assert_eq!(default_theme.success, Color::Rgb(166, 227, 161));
        assert_eq!(default_theme.borders, Color::Rgb(108, 112, 134));

        let dark = Theme::from_name("dark").unwrap();
        assert_eq!(dark.bg, Color::Rgb(17, 19, 24));
        assert_eq!(dark.fg, Color::Rgb(230, 230, 230));

        let light = Theme::from_name("light").unwrap();
        assert_eq!(light.bg, Color::Rgb(247, 245, 239));
        assert_eq!(light.fg, Color::Rgb(45, 42, 38));

        let dracula = Theme::from_name("dracula").unwrap();
        assert_eq!(dracula.bg, Color::Rgb(40, 42, 54));
        assert_eq!(dracula.fg, Color::Rgb(248, 248, 242));

        let nord = Theme::from_name("nord").unwrap();
        assert_eq!(nord.bg, Color::Rgb(46, 52, 64));
        assert_eq!(nord.fg, Color::Rgb(236, 239, 244));
    }

    /// Iterate over all built-in themes and assert they load successfully.
    #[test]
    fn test_new_themes_load_correctly() {
        for name in Theme::available_names() {
            let result = Theme::from_name(name);
            assert!(
                result.is_ok(),
                "Theme '{}' failed to load: {:?}",
                name,
                result.err()
            );
            let theme = result.unwrap();
            assert_eq!(theme.name, *name, "Theme name mismatch for '{}'", name);
        }
    }

    /// Load a base theme and check that derived tokens are non-trivial.
    #[test]
    fn test_derived_defaults_are_set() {
        let theme = Theme::from_name("default").unwrap();

        // bg_panel should differ from bg (it's derived via lighten)
        assert_ne!(theme.bg_panel, theme.bg, "bg_panel should differ from bg");
        assert_ne!(
            theme.bg_element, theme.bg,
            "bg_element should differ from bg"
        );
        assert_ne!(theme.bg_menu, theme.bg, "bg_menu should differ from bg");

        // text_muted should differ from fg (it's blended toward bg)
        assert_ne!(
            theme.text_muted, theme.fg,
            "text_muted should differ from fg"
        );

        // border_subtle should differ from borders (it's blended toward bg)
        assert_ne!(
            theme.border_subtle, theme.borders,
            "border_subtle should differ from borders"
        );

        // diff backgrounds should differ from bg
        assert_ne!(
            theme.diff_added_bg, theme.bg,
            "diff_added_bg should differ from bg"
        );
        assert_ne!(
            theme.diff_removed_bg, theme.bg,
            "diff_removed_bg should differ from bg"
        );

        // md_code_bg should differ from bg
        assert_ne!(
            theme.md_code_bg, theme.bg,
            "md_code_bg should differ from bg"
        );

        // Syntax tokens: not all black (Color::Rgb(0,0,0))
        let black = Color::Rgb(0, 0, 0);
        assert_ne!(theme.syntax_comment, black);
        assert_ne!(theme.syntax_keyword, black);
        assert_ne!(theme.syntax_function, black);
        assert_ne!(theme.syntax_string, black);
        assert_ne!(theme.syntax_number, black);
        assert_ne!(theme.syntax_type, black);
    }

    /// All new themes should have non-trivial derived/explicit tokens.
    #[test]
    fn test_all_themes_have_distinct_tokens() {
        let black = Color::Rgb(0, 0, 0);
        for name in Theme::available_names() {
            let theme = Theme::from_name(name).unwrap();
            assert_ne!(
                theme.syntax_keyword, black,
                "Theme '{}' has black syntax_keyword",
                name
            );
            assert_ne!(
                theme.syntax_function, black,
                "Theme '{}' has black syntax_function",
                name
            );
            assert_ne!(
                theme.md_heading, black,
                "Theme '{}' has black md_heading",
                name
            );
        }
    }

    #[test]
    fn test_overrides_apply_to_new_fields() {
        let mut theme = Theme::default();
        let overrides = ThemeOverrides {
            bg_panel: Some("#FF0000".to_string()),
            syntax_keyword: Some("#00FF00".to_string()),
            md_heading: Some("#0000FF".to_string()),
            ..Default::default()
        };

        theme.apply_overrides(&overrides).unwrap();

        assert_eq!(theme.bg_panel, Color::Rgb(255, 0, 0));
        assert_eq!(theme.syntax_keyword, Color::Rgb(0, 255, 0));
        assert_eq!(theme.md_heading, Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_color_helpers() {
        let white = Color::Rgb(255, 255, 255);
        let black = Color::Rgb(0, 0, 0);

        // darken white by 50% -> gray
        let darkened = darken(white, 0.5);
        assert_eq!(darkened, Color::Rgb(127, 127, 127));

        // lighten black by 50% -> gray
        let lightened = lighten(black, 0.5);
        assert_eq!(lightened, Color::Rgb(127, 127, 127));

        // blend black and white 50% -> gray
        let blended = blend(black, white, 0.5);
        assert_eq!(blended, Color::Rgb(127, 127, 127));

        // blend at 0% -> first color
        let blended0 = blend(white, black, 0.0);
        assert_eq!(blended0, Color::Rgb(255, 255, 255));

        // blend at 100% -> second color
        let blended1 = blend(white, black, 1.0);
        assert_eq!(blended1, Color::Rgb(0, 0, 0));
    }

    #[test]
    fn test_rosepine_alias() {
        let rp1 = Theme::from_name("rosepine").unwrap();
        let rp2 = Theme::from_name("rose-pine").unwrap();
        assert_eq!(rp1.bg, rp2.bg);
        assert_eq!(rp1.fg, rp2.fg);
        assert_eq!(rp1.accent, rp2.accent);
    }
}
