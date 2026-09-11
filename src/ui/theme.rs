#![allow(dead_code)]

use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub secondary: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub border: Color,
    pub gauge_fill: Color,
    pub gauge_bg: Color,
    pub error: Color,
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha",
            bg: Color::Rgb(30, 30, 46),
            fg: Color::Rgb(205, 214, 244),
            accent: Color::Rgb(137, 180, 250),        // Blue
            secondary: Color::Rgb(166, 173, 200),     // Subtext
            selection_bg: Color::Rgb(69, 71, 90),     // Surface1
            selection_fg: Color::Rgb(245, 224, 220),  // Rosewater
            border: Color::Rgb(88, 91, 112),          // Surface2
            gauge_fill: Color::Rgb(166, 227, 161),    // Green
            gauge_bg: Color::Rgb(49, 50, 68),         // Surface0
            error: Color::Rgb(243, 139, 168),         // Red
        }
    }

    pub fn nord() -> Self {
        Self {
            name: "Nord",
            bg: Color::Rgb(46, 52, 64),
            fg: Color::Rgb(236, 239, 244),
            accent: Color::Rgb(136, 192, 208),
            secondary: Color::Rgb(216, 222, 233),
            selection_bg: Color::Rgb(67, 76, 94),
            selection_fg: Color::Rgb(236, 239, 244),
            border: Color::Rgb(76, 86, 106),
            gauge_fill: Color::Rgb(163, 190, 140),
            gauge_bg: Color::Rgb(59, 66, 82),
            error: Color::Rgb(191, 97, 106),          // Red
        }
    }

    pub fn default_dark() -> Self {
        Self {
            name: "Default Dark",
            bg: Color::Reset,
            fg: Color::White,
            accent: Color::Cyan,
            secondary: Color::DarkGray,
            selection_bg: Color::Blue,
            selection_fg: Color::White,
            border: Color::DarkGray,
            gauge_fill: Color::Green,
            gauge_bg: Color::DarkGray,
            error: Color::Red,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}
