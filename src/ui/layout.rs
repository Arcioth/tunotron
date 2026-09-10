use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Gauge, Paragraph, Row, Table},
    Frame,
};
use crate::action::WindowId;
use crate::app::{AppState, ViewDensity};
use crate::library::BrowserEntry;
use crate::ui::geom::UiGeom;
use crate::ui::theme::Theme;
use crate::ui::window::{render_help_modal, render_plugin_modal};

pub fn render_app(frame: &mut Frame, state: &AppState, geom: &mut UiGeom, theme: &Theme) {
    let size = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(4),
        ])
        .split(size);

    // Save hitboxes for mouse clicks
    geom.browser_rect = chunks[1];

    render_header(frame, chunks[0], state, theme);
    render_browser_table(frame, chunks[1], state, geom, theme);
    render_player_bar(frame, chunks[2], state, geom, theme);

    // Floating Window Stack Layer
    if let Some(top_window) = geom.top_window() {
        match top_window {
            WindowId::Help => {
                geom.modal_rect = render_help_modal(frame, size, theme);
            }
            WindowId::PluginModal => {
                if let Some(content) = &geom.modal_content {
                    geom.modal_rect = render_plugin_modal(frame, size, content, theme);
                } else {
                    geom.modal_rect = Rect::default();
                }
            }
        }
    } else {
        geom.modal_rect = Rect::default();
    }
}

fn render_header(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(concat!(" 🎵 TUNOTRON v", env!("CARGO_PKG_VERSION"), " "));

    let current_path_str = state.current_dir.to_string_lossy();
    let density_label = match state.density {
        ViewDensity::Comfortable => "[Z: Normal] ",
        ViewDensity::Compact => "[Z: Compact] ",
    };

    let header_line = Line::from(vec![
        Span::styled(" Folder: ", Style::default().fg(theme.secondary)),
        Span::styled(current_path_str, Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled("[?: Help] ", Style::default().fg(theme.accent)),
        Span::styled("[.: Locate] ", Style::default().fg(theme.accent)),
        Span::styled(density_label, Style::default().fg(theme.accent)),
        Span::styled("[m: Loop] ", Style::default().fg(theme.accent)),
        Span::styled("[s: Shuffle]", Style::default().fg(theme.accent)),
    ]);

    let paragraph = Paragraph::new(header_line).block(block);
    frame.render_widget(paragraph, area);
}

fn render_browser_table(frame: &mut Frame, area: Rect, state: &AppState, geom: &mut UiGeom, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(state.browser_title.as_str());

    let inner = block.inner(area);
    let header_h: u16 = 1 + if state.density == ViewDensity::Compact { 0 } else { 1 };
    geom.browser_rows_rect = Rect {
        x: inner.x,
        y: inner.y.saturating_add(header_h),
        width: inner.width,
        height: inner.height.saturating_sub(header_h),
    };

    if state.browser_items.is_empty() {
        let empty_row = Row::new(vec![Cell::from("Empty directory. (Jailed to music root)")]);
        let table = Table::new(vec![empty_row], [Constraint::Percentage(100)]).block(block);
        frame.render_widget(table, area);
        return;
    }

    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from(""),
        Cell::from("Title / Filename"),
        Cell::from("Artist"),
        Cell::from("Duration"),
    ])
    .style(header_style)
    .bottom_margin(if state.density == ViewDensity::Compact { 0 } else { 1 });

    let current_playing_path = state.playback.current_track.as_ref().map(|t| &t.path);

    let rows: Vec<Row> = state
        .browser_items
        .iter()
        .map(|entry| {
            match entry {
                BrowserEntry::ParentDir(_) => {
                    Row::new(vec![
                        Cell::from(" ⬆ "),
                        Cell::from(".. (Parent Directory)"),
                        Cell::from(""),
                        Cell::from(""),
                    ])
                    .style(Style::default().fg(theme.secondary))
                }
                BrowserEntry::Directory { name, .. } => {
                    Row::new(vec![
                        Cell::from(" 📁"),
                        Cell::from(name.as_str()),
                        Cell::from("<Folder>"),
                        Cell::from(""),
                    ])
                    .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))
                }
                BrowserEntry::AudioTrack(track) => {
                    let is_active_track = current_playing_path == Some(&track.path);
                    let (status_icon, track_style) = if is_active_track {
                        if state.playback.is_playing() {
                            (" ▶ ", Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD))
                        } else {
                            (" ⏸ ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))
                        }
                    } else {
                        (" 🎵", Style::default().fg(theme.fg))
                    };

                    Row::new(vec![
                        Cell::from(status_icon),
                        Cell::from(track.title.as_str()),
                        Cell::from(track.artist.as_str()),
                        Cell::from(track.duration_label.as_str()),
                    ])
                    .style(track_style)
                }
            }
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Percentage(50),
        Constraint::Percentage(33),
        Constraint::Length(10),
    ];

    let highlight_style = Style::default()
        .bg(theme.selection_bg)
        .fg(theme.selection_fg)
        .add_modifier(Modifier::BOLD);

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(highlight_style)
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(table, area, &mut geom.table_state);
}

fn render_player_bar(frame: &mut Frame, area: Rect, state: &AppState, geom: &mut UiGeom, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(" Player ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    geom.progress_rect = Rect::default();
    if inner.height < 2 {
        return;
    }

    let sub_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // Save progress bar hitbox for mouse clicks
    geom.progress_rect = sub_chunks[1];

    // Row 1: Status Icon + Title + Artist + Loop/Shuffle + Volume (Zero heap allocations)
    let status_icon = if state.playback.is_playing() {
        Span::styled(" ▶ PLAYING ", Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD))
    } else if state.playback.is_paused() {
        Span::styled(" ⏸ PAUSED  ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" ⏹ STOPPED ", Style::default().fg(theme.secondary))
    };

    let loop_mode_str = state.playback.loop_mode.badge_str();
    let shuffle_str = state.playback.shuffle_mode.badge_str();

    let row1 = Line::from(vec![
        status_icon,
        Span::raw(" "),
        Span::styled(
            state.playback.now_playing_label.as_str(),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(loop_mode_str, Style::default().fg(theme.accent)),
        Span::raw(" "),
        Span::styled(shuffle_str, Style::default().fg(theme.accent)),
        Span::raw(" | "),
        Span::styled(state.playback.vol_label.as_str(), Style::default().fg(theme.secondary)),
    ]);

    frame.render_widget(Paragraph::new(row1), sub_chunks[0]);

    // Row 2: Progress Gauge (Interpolated via monotonic PlaybackClock, 0 heap allocations)
    let elapsed = state.clock.now();
    let duration = state.playback.duration_sec.max(0.001);
    let percent = ((elapsed / duration) * 100.0).clamp(0.0, 100.0) as u16;

    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(theme.gauge_fill)
                .bg(theme.gauge_bg),
        )
        .percent(percent)
        .label(state.playback.time_label.as_str());

    frame.render_widget(gauge, sub_chunks[1]);
}
