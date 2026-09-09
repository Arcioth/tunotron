use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph, Tabs},
    Frame,
};
use crate::action::ViewId;
use crate::app::AppState;
use crate::ui::theme::Theme;
use crate::ui::views::View;

pub fn render_app(frame: &mut Frame, state: &mut AppState, theme: &Theme) {
    let size = frame.area();

    // Divide screen into 3 vertical chunks: Top bar (3), Main view (min 0), Bottom Player Bar (4)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(4),
        ])
        .split(size);

    render_header(frame, chunks[0], state, theme);
    render_main_view(frame, chunks[1], state, theme);
    render_player_bar(frame, chunks[2], state, theme);
}

fn render_header(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let titles = vec![
        " 1: Library ",
        " 2: Browser ",
        " 3: Queue ",
    ];

    let selected_index = match state.active_view {
        ViewId::Library => 0,
        ViewId::FileBrowser => 1,
        ViewId::Queue => 2,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border))
                .title(" 🎵 TUNOTRON "),
        )
        .select(selected_index)
        .style(Style::default().fg(theme.secondary))
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn render_main_view(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    match state.active_view {
        ViewId::Library => {
            state.views.library.render(frame, area, theme, true);
        }
        ViewId::FileBrowser => {
            state.views.browser.render(frame, area, theme, true);
        }
        ViewId::Queue => {
            state.views.queue.render(frame, area, theme, true);
        }
    }
}

fn render_player_bar(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(" Player ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let sub_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // Row 1: Status Icon + Title + Artist + Volume
    let status_icon = if state.playback.is_playing {
        Span::styled(" ▶ PLAYING ", Style::default().fg(theme.gauge_fill).add_modifier(Modifier::BOLD))
    } else if state.playback.is_paused {
        Span::styled(" ⏸ PAUSED  ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" ⏹ STOPPED ", Style::default().fg(theme.secondary))
    };

    let track_info = if let Some(track) = &state.playback.current_track {
        format!("{} — {}", track.artist, track.title)
    } else {
        "No track playing. Select a track and press Enter.".to_string()
    };

    let vol_text = format!("Vol: {:>3.0}%", state.playback.volume);

    let row1 = Line::from(vec![
        status_icon,
        Span::raw(" "),
        Span::styled(track_info, Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(vol_text, Style::default().fg(theme.secondary)),
    ]);

    frame.render_widget(Paragraph::new(row1), sub_chunks[0]);

    // Row 2: Progress Gauge
    let elapsed = state.playback.current_time_sec;
    let duration = state.playback.duration_sec.max(0.001);
    let percent = ((elapsed / duration) * 100.0).clamp(0.0, 100.0) as u16;

    let elapsed_fmt = format_seconds(elapsed);
    let duration_fmt = format_seconds(state.playback.duration_sec);
    let label = format!("{} / {}", elapsed_fmt, duration_fmt);

    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(theme.gauge_fill)
                .bg(theme.gauge_bg),
        )
        .percent(percent)
        .label(label);

    frame.render_widget(gauge, sub_chunks[1]);
}

fn format_seconds(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{:02}:{:02}", mins, secs)
}
