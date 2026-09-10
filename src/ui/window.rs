#![allow(dead_code)]

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};
use crate::ui::theme::Theme;

/// Helper function to create a centered Rect within the parent area
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Renders the built-in Help floating window
pub fn render_help_modal(frame: &mut Frame, area: Rect, theme: &Theme) -> Rect {
    let popup_area = centered_rect(65, 65, area);

    // Clear background behind modal to prevent text bleed
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(concat!(" 🎵 Tunotron Controls & Shortcuts (v", env!("CARGO_PKG_VERSION"), ") "));

    let help_text = vec![
        Line::from(vec![
            Span::styled("Navigation              ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled("Playback Controls", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  j / ↓       ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Move down      "),
            Span::styled("  Space / c   ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Play / Pause (resume)"),
        ]),
        Line::from(vec![
            Span::styled("  k / ↑       ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Move up        "),
            Span::styled("  b           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Next track in folder"),
        ]),
        Line::from(vec![
            Span::styled("  Enter       ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Open / Play    "),
            Span::styled("  z           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Previous track"),
        ]),
        Line::from(vec![
            Span::styled("  Backspace   ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Parent folder  "),
            Span::styled("  v           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Stop playback"),
        ]),
        Line::from(vec![
            Span::styled("  g g / G     ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Top / Bottom   "),
            Span::styled("  m           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Cycle Loop (Off/Track/All)"),
        ]),
        Line::from(vec![
            Span::styled("  d / u       ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Page down/up   "),
            Span::styled("  s           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Toggle Shuffle (On/Off)"),
        ]),
        Line::from(vec![
            Span::raw("                             "),
            Span::styled("  → / ←       ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Seek 5s Forward / Back"),
        ]),
        Line::from(vec![
            Span::raw("                             "),
            Span::styled("  + / -       ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Volume Up / Down"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Folder & App Actions", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  r           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Reload current directory from disk"),
        ]),
        Line::from(vec![
            Span::styled("  ?           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Toggle this Help modal"),
        ]),
        Line::from(vec![
            Span::styled("  q           ", Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
            Span::raw("Quit Tunotron cleanly"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press Esc or ? to close this window", Style::default().fg(theme.secondary).add_modifier(Modifier::ITALIC)),
        ]),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
    popup_area
}

/// Renders a dynamic plugin floating modal window
pub fn render_plugin_modal(
    frame: &mut Frame,
    area: Rect,
    content: &crate::ui::geom::ModalContent,
    theme: &Theme,
) -> Rect {
    let popup_area = centered_rect(65, 60, area);

    // Clear background behind modal to prevent text bleed
    frame.render_widget(Clear, popup_area);

    let title_string = format!(" 💡 {} (Esc to close) ", content.title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(title_string);

    let paragraph = Paragraph::new(content.content.as_str())
        .block(block)
        .style(Style::default().fg(theme.fg))
        .wrap(ratatui::widgets::Wrap { trim: true });

    frame.render_widget(paragraph, popup_area);
    popup_area
}
