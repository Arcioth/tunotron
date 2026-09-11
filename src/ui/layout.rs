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

    render_header(frame, chunks[0], state, geom, theme);
    if state.active_tab == 0 {
        render_browser_table(frame, chunks[1], state, geom, theme);
    } else if state.active_tab == 1 && state.tabs.get(1).map(|t| t.id.as_str()) == Some("extensions") {
        crate::ui::render_extension_manager(frame, chunks[1], state, theme);
    } else if let Some(page) = state.active_extension_page() {
        crate::ui::render_extension_page(frame, chunks[1], page, theme);
    } else {
        render_browser_table(frame, chunks[1], state, geom, theme);
    }
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

    // In-App Floating Toast Notification Layer
    if let Some(toast) = state.toast_message() {
        let toast_len = toast.chars().count() as u16;
        let toast_w = (toast_len + 6).min(size.width.saturating_sub(4)).max(24);
        let toast_h = 3;
        let toast_x = size.width.saturating_sub(toast_w + 2);
        let toast_y = size.height.saturating_sub(toast_h + 4);
        let toast_rect = Rect {
            x: toast_x,
            y: toast_y,
            width: toast_w,
            height: toast_h,
        };

        frame.render_widget(ratatui::widgets::Clear, toast_rect);
        let toast_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD))
            .title(" 🔔 Notification ");
        let toast_para = Paragraph::new(format!(" {}", toast)).block(toast_block);
        frame.render_widget(toast_para, toast_rect);
    }
}

fn render_header(frame: &mut Frame, area: Rect, state: &AppState, geom: &mut UiGeom, theme: &Theme) {
    geom.tab_rects.clear();
    geom.header_rect = area;

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

    let mut spans = vec![
        Span::styled(" Folder: ", Style::default().fg(theme.secondary)),
        Span::styled(current_path_str, Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled("[?: Help] ", Style::default().fg(theme.accent)),
        Span::styled("[.: Locate] ", Style::default().fg(theme.accent)),
        Span::styled(density_label, Style::default().fg(theme.accent)),
        Span::styled("[m: Loop] ", Style::default().fg(theme.accent)),
        Span::styled("[s: Shuffle]", Style::default().fg(theme.accent)),
    ];

    if let Some(topbar_slot) = state.slots.get("topbar") {
        spans.push(Span::styled(
            format!(" | {}", topbar_slot),
            Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD),
        ));
    }

    let mut current_char_x = area.x.saturating_add(1);
    for s in &spans {
        current_char_x = current_char_x.saturating_add(s.content.chars().count() as u16);
    }

    spans.push(Span::raw(" | Tabs: "));
    current_char_x = current_char_x.saturating_add(9);

    for (idx, tab) in state.tabs.iter().enumerate() {
        let is_active = idx == state.active_tab;
        let fallback_num = (idx + 1).to_string();
        let num = tab.shortcut.as_deref().unwrap_or(&fallback_num);
        let pill_text = format!("[{}:{}]", num, tab.title);
        let pill_len = pill_text.chars().count() as u16;

        let pill_rect = Rect {
            x: current_char_x,
            y: area.y.saturating_add(1),
            width: pill_len,
            height: 1,
        };
        geom.tab_rects.push((pill_rect, idx));
        current_char_x = current_char_x.saturating_add(pill_len + 1);

        let title_style = if is_active {
            Style::default().bg(theme.selection_bg).fg(theme.selection_fg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.secondary)
        };
        spans.push(Span::styled(pill_text, title_style));
        spans.push(Span::raw(" "));
    }

    let header_line = Line::from(spans);

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

    let mut row1_spans = vec![
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
    ];

    if let Some(extra) = state.slots.get("player_extra") {
        row1_spans.push(Span::raw(" | "));
        row1_spans.push(Span::styled(
            extra.as_str(),
            Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD),
        ));
    }

    let row1 = Line::from(row1_spans);

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
