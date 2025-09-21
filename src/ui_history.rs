use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::model::{AppSnapshot, HistoryPanelLevel};
use crate::theme::TEXT;

pub fn draw_history(f: &mut Frame, s: &AppSnapshot) {
    let area = f.size();
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .margin(0)
        .split(area);

    draw_header(f, chunks[0], s);
    draw_body(f, chunks[1], s);
}

fn draw_header(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let subtitle = if s.history.loading {
        "Loading history…"
    } else if let Some(err) = &s.history.error {
        err.as_str()
    } else {
        match s.history.level {
            HistoryPanelLevel::Dates => "Enter/Click ▸ view encounters · ↑/↓ scroll · q/Esc quits",
            HistoryPanelLevel::Encounters => "← go back · ↑/↓ scroll · Enter selects",
        }
    };

    let title_line = Line::from(vec![Span::styled(
        "History",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]);
    let subtitle_line = Line::from(vec![Span::styled(subtitle, Style::default().fg(TEXT))]);

    let block = Paragraph::new(vec![title_line, subtitle_line])
        .alignment(ratatui::layout::Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title("History"));
    f.render_widget(block, area);
}

fn draw_body(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    if s.history.loading {
        let paragraph = Paragraph::new("Loading…")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(paragraph, area);
        return;
    }

    if let Some(err) = &s.history.error {
        let block = Paragraph::new(err.as_str())
            .alignment(ratatui::layout::Alignment::Left)
            .block(Block::default().borders(Borders::ALL).title("Error"));
        f.render_widget(block, area);
        return;
    }

    match s.history.level {
        HistoryPanelLevel::Dates => draw_dates(f, area, s),
        HistoryPanelLevel::Encounters => draw_encounters(f, area, s),
    }
}

fn draw_dates(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    if s.history.days.is_empty() {
        let block = Paragraph::new("No encounters recorded yet.")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    }

    let items: Vec<ListItem> = s
        .history
        .days
        .iter()
        .map(|day| ListItem::new(day.label.clone()))
        .collect();

    let mut state = ListState::default();
    state.select(Some(s.history.selected_day));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Dates"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_encounters(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let Some(day) = s.history.current_day() else {
        let block = Paragraph::new("No date selected.")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    };

    if day.encounters.is_empty() {
        let block = Paragraph::new("No encounters captured for this date.")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    }

    let items: Vec<ListItem> = day
        .encounters
        .iter()
        .map(|enc| {
            let text = format!("{}  [{}]", enc.display_title, enc.time_label);
            ListItem::new(text)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(s.history.selected_encounter));

    let title = format!("Encounters · {}", day.label);
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut state);
}
