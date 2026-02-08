use ratatui::style::Color;

const BUILTIN_THEMES: [&str; 5] = ["default", "dark", "light", "dracula", "nord"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub borders: Color,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeOverrides {
    pub bg: Option<String>,
    pub fg: Option<String>,
    pub accent: Option<String>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub success: Option<String>,
    pub borders: Option<String>,
}

impl Theme {
    pub fn available_names() -> &'static [&'static str] {
        &BUILTIN_THEMES
    }

    pub fn from_name(name: &str) -> Result<Self, String> {
        let normalized = name.trim().to_ascii_lowercase();
        let theme = match normalized.as_str() {
            "default" => Self {
                name: "default".to_string(),
                bg: Color::Rgb(24, 25, 38),
                fg: Color::Rgb(205, 214, 244),
                accent: Color::Rgb(137, 180, 250),
                error: Color::Rgb(243, 139, 168),
                warning: Color::Rgb(249, 226, 175),
                success: Color::Rgb(166, 227, 161),
                borders: Color::Rgb(108, 112, 134),
            },
            "dark" => Self {
                name: "dark".to_string(),
                bg: Color::Rgb(17, 19, 24),
                fg: Color::Rgb(230, 230, 230),
                accent: Color::Rgb(79, 179, 255),
                error: Color::Rgb(255, 107, 107),
                warning: Color::Rgb(255, 184, 107),
                success: Color::Rgb(126, 231, 135),
                borders: Color::Rgb(91, 101, 118),
            },
            "light" => Self {
                name: "light".to_string(),
                bg: Color::Rgb(247, 245, 239),
                fg: Color::Rgb(45, 42, 38),
                accent: Color::Rgb(0, 92, 197),
                error: Color::Rgb(215, 58, 73),
                warning: Color::Rgb(154, 103, 0),
                success: Color::Rgb(34, 134, 58),
                borders: Color::Rgb(140, 133, 122),
            },
            "dracula" => Self {
                name: "dracula".to_string(),
                bg: Color::Rgb(40, 42, 54),
                fg: Color::Rgb(248, 248, 242),
                accent: Color::Rgb(189, 147, 249),
                error: Color::Rgb(255, 85, 85),
                warning: Color::Rgb(241, 250, 140),
                success: Color::Rgb(80, 250, 123),
                borders: Color::Rgb(98, 114, 164),
            },
            "nord" => Self {
                name: "nord".to_string(),
                bg: Color::Rgb(46, 52, 64),
                fg: Color::Rgb(236, 239, 244),
                accent: Color::Rgb(136, 192, 208),
                error: Color::Rgb(191, 97, 106),
                warning: Color::Rgb(235, 203, 139),
                success: Color::Rgb(163, 190, 140),
                borders: Color::Rgb(76, 86, 106),
            },
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
        apply_override(&mut self.bg, &overrides.bg, "bg")?;
        apply_override(&mut self.fg, &overrides.fg, "fg")?;
        apply_override(&mut self.accent, &overrides.accent, "accent")?;
        apply_override(&mut self.error, &overrides.error, "error")?;
        apply_override(&mut self.warning, &overrides.warning, "warning")?;
        apply_override(&mut self.success, &overrides.success, "success")?;
        apply_override(&mut self.borders, &overrides.borders, "borders")?;
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
        assert_eq!(
            Theme::available_names(),
            &["default", "dark", "light", "dracula", "nord"]
        );
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
}
