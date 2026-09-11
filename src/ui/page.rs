#![allow(dead_code)]

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use serde::{Deserialize, Serialize};
use crate::ui::theme::Theme;

fn default_max_slider() -> f64 { 100.0 }
fn default_step_slider() -> f64 { 1.0 }
fn default_radar_distance() -> f64 { 1.0 }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FormField {
    Slider {
        id: String,
        label: String,
        value: f64,
        #[serde(default)]
        min: f64,
        #[serde(default = "default_max_slider")]
        max: f64,
        #[serde(default = "default_step_slider")]
        step: f64,
        #[serde(default)]
        unit: String,
    },
    Select {
        id: String,
        label: String,
        options: Vec<String>,
        #[serde(default)]
        selected: usize,
    },
    Toggle {
        id: String,
        label: String,
        #[serde(default)]
        checked: bool,
    },
    Button {
        id: String,
        label: String,
    },
    Text {
        id: String,
        label: String,
        #[serde(default)]
        value: String,
    },
}

impl FormField {
    pub fn id(&self) -> &str {
        match self {
            FormField::Slider { id, .. }
            | FormField::Select { id, .. }
            | FormField::Toggle { id, .. }
            | FormField::Button { id, .. }
            | FormField::Text { id, .. } => id.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadarState {
    #[serde(alias = "angle")]
    pub angle_rad: f64,
    #[serde(default = "default_radar_distance")]
    pub distance: f64,
    #[serde(alias = "elevation", default)]
    pub elevation_deg: f64,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabEntry {
    pub id: String,
    pub title: String,
    pub shortcut: Option<String>,
    pub plugin_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionPage {
    #[serde(alias = "tab_id")]
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<FormField>,
    #[serde(default)]
    pub selected_field: usize,
    #[serde(default)]
    pub radar: Option<RadarState>,
}

impl ExtensionPage {
    pub fn new(id: String, title: String) -> Self {
        Self {
            id,
            title,
            description: None,
            fields: Vec::new(),
            selected_field: 0,
            radar: None,
        }
    }

    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    pub fn with_fields(mut self, fields: Vec<FormField>) -> Self {
        self.fields = fields;
        self
    }

    pub fn with_radar(mut self, radar: RadarState) -> Self {
        self.radar = Some(radar);
        self
    }

    pub fn nav_up(&mut self) {
        if !self.fields.is_empty() {
            self.selected_field = self.selected_field.saturating_sub(1);
        }
    }

    pub fn nav_down(&mut self) {
        if !self.fields.is_empty() {
            self.selected_field = (self.selected_field + 1).min(self.fields.len() - 1);
        }
    }

    pub fn adjust_left(&mut self) -> Option<(String, serde_json::Value)> {
        if self.selected_field >= self.fields.len() {
            return None;
        }

        match &mut self.fields[self.selected_field] {
            FormField::Slider { id, value, min, step, .. } => {
                *value = (*value - *step).max(*min);
                // Round to 4 decimal places to prevent floating point inaccuracies
                *value = (*value * 10000.0).round() / 10000.0;
                Some((id.clone(), serde_json::json!(*value)))
            }
            FormField::Select { id, options, selected, .. } => {
                if !options.is_empty() {
                    *selected = if *selected == 0 {
                        options.len() - 1
                    } else {
                        *selected - 1
                    };
                    Some((id.clone(), serde_json::json!(options[*selected].clone())))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn adjust_right(&mut self) -> Option<(String, serde_json::Value)> {
        if self.selected_field >= self.fields.len() {
            return None;
        }

        match &mut self.fields[self.selected_field] {
            FormField::Slider { id, value, max, step, .. } => {
                *value = (*value + *step).min(*max);
                *value = (*value * 10000.0).round() / 10000.0;
                Some((id.clone(), serde_json::json!(*value)))
            }
            FormField::Select { id, options, selected, .. } => {
                if !options.is_empty() {
                    *selected = (*selected + 1) % options.len();
                    Some((id.clone(), serde_json::json!(options[*selected].clone())))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn activate(&mut self) -> Option<(String, serde_json::Value)> {
        if self.selected_field >= self.fields.len() {
            return None;
        }

        match &mut self.fields[self.selected_field] {
            FormField::Toggle { id, checked, .. } => {
                *checked = !*checked;
                Some((id.clone(), serde_json::json!(*checked)))
            }
            FormField::Button { id, .. } => {
                Some((id.clone(), serde_json::json!("clicked")))
            }
            _ => None,
        }
    }

    pub fn update_field_value(&mut self, field_id: &str, val: &serde_json::Value) {
        for field in &mut self.fields {
            if field.id() == field_id {
                match field {
                    FormField::Slider { value, .. } => {
                        if let Some(num) = val.as_f64() {
                            *value = num;
                        }
                    }
                    FormField::Select { options, selected, .. } => {
                        if let Some(idx) = val.as_u64() {
                            if (idx as usize) < options.len() {
                                *selected = idx as usize;
                            }
                        } else if let Some(s) = val.as_str() {
                            if let Some(pos) = options.iter().position(|o| o == s) {
                                *selected = pos;
                            }
                        }
                    }
                    FormField::Toggle { checked, .. } => {
                        if let Some(b) = val.as_bool() {
                            *checked = b;
                        }
                    }
                    FormField::Text { value, .. } => {
                        if let Some(s) = val.as_str() {
                            *value = s.to_string();
                        }
                    }
                    FormField::Button { .. } => {}
                }
                break;
            }
        }
    }
}

pub fn render_extension_page(
    frame: &mut Frame,
    area: Rect,
    page: &ExtensionPage,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(format!(" ⚙️ {} ", page.title));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    if page.radar.is_some() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(inner);

        render_form_controls(frame, cols[0], page, theme);
        render_spatial_radar(frame, cols[1], page, theme);
    } else {
        render_form_controls(frame, inner, page, theme);
    }
}

fn render_form_controls(
    frame: &mut Frame,
    area: Rect,
    page: &ExtensionPage,
    theme: &Theme,
) {
    let mut lines = Vec::new();

    if let Some(desc) = &page.description {
        lines.push(Line::from(vec![
            Span::styled(format!(" ℹ️ {}", desc), Style::default().fg(theme.secondary)),
        ]));
        lines.push(Line::from(""));
    }

    for (idx, field) in page.fields.iter().enumerate() {
        let is_selected = idx == page.selected_field;
        let prefix = if is_selected { " ▶ " } else { "   " };
        let label_style = if is_selected {
            Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };

        match field {
            FormField::Slider { label, value, min, max, unit, .. } => {
                let width_chars: usize = 16;
                let ratio = ((*value - *min) / (*max - *min).max(0.0001)).clamp(0.0, 1.0);
                let filled = (ratio * width_chars as f64).round() as usize;
                let empty = width_chars.saturating_sub(filled);

                let bar = format!("[{}{}]", "=".repeat(filled), "-".repeat(empty));
                let val_str = format!("{:.2}{}", value, unit);

                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(theme.gauge_fill)),
                    Span::styled(format!("{:<22} ", label), label_style),
                    Span::styled(bar, if is_selected { Style::default().fg(theme.gauge_fill) } else { Style::default().fg(theme.secondary) }),
                    Span::raw(" "),
                    Span::styled(val_str, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                ]));
            }
            FormField::Select { label, options, selected, .. } => {
                let current_opt = options.get(*selected).map(|s| s.as_str()).unwrap_or("None");
                let opt_str = format!("< {} >", current_opt);

                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(theme.gauge_fill)),
                    Span::styled(format!("{:<22} ", label), label_style),
                    Span::styled(opt_str, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
                ]));
            }
            FormField::Toggle { label, checked, .. } => {
                let check_str = if *checked { "[✓] Enabled" } else { "[ ] Disabled" };
                let check_style = if *checked {
                    Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.secondary)
                };

                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(theme.gauge_fill)),
                    Span::styled(format!("{:<22} ", label), label_style),
                    Span::styled(check_str, check_style),
                ]));
            }
            FormField::Button { label, .. } => {
                let btn_str = format!("[ {} ]", label);
                let btn_style = if is_selected {
                    Style::default().bg(theme.selection_bg).fg(theme.selection_fg).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.secondary)
                };

                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(theme.gauge_fill)),
                    Span::styled(btn_str, btn_style),
                ]));
            }
            FormField::Text { label, value, .. } => {
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(theme.gauge_fill)),
                    Span::styled(format!("{:<22}: ", label), label_style),
                    Span::styled(value.as_str(), Style::default().fg(theme.fg)),
                ]));
            }
        }
        lines.push(Line::from("")); // Spacing between controls
    }

    lines.push(Line::from(vec![
        Span::styled(" [↑/↓] Select Field   [←/→] Adjust Value   [Enter/Space] Toggle / Action", Style::default().fg(theme.secondary)),
    ]));

    let para = Paragraph::new(lines);
    frame.render_widget(para, area);
}

fn render_spatial_radar(
    frame: &mut Frame,
    area: Rect,
    page: &ExtensionPage,
    theme: &Theme,
) {
    let radar_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.gauge_fill))
        .title(" 🌐 3D Soundstage Orbit ");

    let inner = radar_block.inner(area);
    frame.render_widget(radar_block, area);

    if inner.height < 7 || inner.width < 15 {
        return;
    }

    let Some(radar) = &page.radar else { return; };

    let angle_deg = (radar.angle_rad.to_degrees().rem_euclid(360.0) * 10.0).round() / 10.0;
    let heading = match angle_deg {
        a if !(22.5..337.5).contains(&a) => "Front [N]",
        a if (22.5..67.5).contains(&a) => "Front-Right [NE]",
        a if (67.5..112.5).contains(&a) => "Right Ear [E]",
        a if (112.5..157.5).contains(&a) => "Rear-Right [SE]",
        a if (157.5..202.5).contains(&a) => "Behind [S]",
        a if (202.5..247.5).contains(&a) => "Rear-Left [SW]",
        a if (247.5..292.5).contains(&a) => "Left Ear [W]",
        _ => "Front-Left [NW]",
    };

    // Calculate source coordinate relative to listener at center (5x5 grid)
    let sin = radar.angle_rad.sin();
    let cos = radar.angle_rad.cos();

    // Determine grid quadrant
    let quadrant_str = if sin > 0.3 && cos > 0.3 {
        "↗ NE (Front-Right)"
    } else if sin > 0.3 && cos < -0.3 {
        "↘ SE (Rear-Right)"
    } else if sin < -0.3 && cos < -0.3 {
        "↙ SW (Rear-Left)"
    } else if sin < -0.3 && cos > 0.3 {
        "↖ NW (Front-Left)"
    } else if cos >= 0.7 {
        "↑ Front Center"
    } else if sin >= 0.7 {
        "→ Direct Right"
    } else if cos <= -0.7 {
        "↓ Direct Behind"
    } else {
        "← Direct Left"
    };

    let mut lines = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("        [Front: 0°]       ", Style::default().fg(theme.secondary)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("             ·            "),
    ]));

    // Dynamic orbital indicator
    let row_n = if cos > 0.4 { "     ·       *       ·     " } else { "     ·       |       ·     " };
    lines.push(Line::from(vec![Span::styled(row_n, Style::default().fg(theme.gauge_fill))]));

    let row_c = if sin > 0.5 {
        " [Left] · · [O] · · * [Right] "
    } else if sin < -0.5 {
        " [Left] * · [O] · · · [Right] "
    } else {
        " [Left] · · [O] · · · [Right] "
    };
    lines.push(Line::from(vec![
        Span::styled(row_c, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
    ]));

    let row_s = if cos < -0.4 { "     ·       *       ·     " } else { "     ·       |       ·     " };
    lines.push(Line::from(vec![Span::styled(row_s, Style::default().fg(theme.gauge_fill))]));

    lines.push(Line::from(vec![
        Span::raw("             ·            "),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       [Behind: 180°]     ", Style::default().fg(theme.secondary)),
    ]));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::styled(" Heading:    ", Style::default().fg(theme.secondary)),
        Span::styled(heading, Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" Azimuth:    ", Style::default().fg(theme.secondary)),
        Span::styled(format!("{:.1}°", angle_deg), Style::default().fg(theme.accent)),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" Elevation:  ", Style::default().fg(theme.secondary)),
        Span::styled(format!("{:.1}°", radar.elevation_deg), Style::default().fg(theme.accent)),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" Trajectory: ", Style::default().fg(theme.secondary)),
        Span::styled(quadrant_str, Style::default().fg(theme.fg)),
    ]));

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_page_navigation_and_adjust() {
        let mut page = ExtensionPage::new("test_page".into(), "Test".into())
            .with_fields(vec![
                FormField::Slider {
                    id: "speed".into(),
                    label: "Speed".into(),
                    value: 0.1,
                    min: 0.0,
                    max: 1.0,
                    step: 0.05,
                    unit: "Hz".into(),
                },
                FormField::Select {
                    id: "pattern".into(),
                    label: "Pattern".into(),
                    options: vec!["Circular".into(), "Figure8".into(), "Spiral".into()],
                    selected: 0,
                },
                FormField::Toggle {
                    id: "reverb".into(),
                    label: "Reverb".into(),
                    checked: false,
                },
            ]);

        assert_eq!(page.selected_field, 0);

        // Adjust slider right
        let update = page.adjust_right();
        assert_eq!(update, Some(("speed".into(), serde_json::json!(0.15))));

        // Adjust slider left
        let update = page.adjust_left();
        assert_eq!(update, Some(("speed".into(), serde_json::json!(0.1))));

        // Navigate down to select
        page.nav_down();
        assert_eq!(page.selected_field, 1);
        let update = page.adjust_right();
        assert_eq!(update, Some(("pattern".into(), serde_json::json!("Figure8"))));

        // Navigate down to toggle
        page.nav_down();
        assert_eq!(page.selected_field, 2);
        let update = page.activate();
        assert_eq!(update, Some(("reverb".into(), serde_json::json!(true))));
    }
}
