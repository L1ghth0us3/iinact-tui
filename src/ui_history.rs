use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
};
use ratatui::Frame;

use crate::model::{AppSnapshot, HistoryPanelLevel};
use crate::theme::{header_style, job_color, title_style, value_style, TEXT};

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
            HistoryPanelLevel::Encounters => "← dates · ↑/↓ scroll · Enter view details",
            HistoryPanelLevel::EncounterDetail => {
                "← encounters · ↑/↓ switch encounter · h/Esc closes"
            }
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
        HistoryPanelLevel::EncounterDetail => draw_encounter_detail(f, area, s),
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

fn draw_encounter_detail(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let Some(day) = s.history.current_day() else {
        let block = Paragraph::new("No date selected.")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    };

    let Some(encounter) = day.encounters.get(s.history.selected_encounter) else {
        let block = Paragraph::new("No encounter selected.")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    };

    let record = &encounter.record;

    let metrics = vec![
        (
            "Encounter",
            if record.encounter.title.is_empty() {
                encounter.display_title.clone()
            } else {
                record.encounter.title.clone()
            },
        ),
        (
            "Zone",
            if record.encounter.zone.is_empty() {
                "Unknown".to_string()
            } else {
                record.encounter.zone.clone()
            },
        ),
        ("Duration", record.encounter.duration.clone()),
        ("ENCDPS", record.encounter.encdps.clone()),
        ("Damage", record.encounter.damage.clone()),
        ("Snapshots", record.snapshots.to_string()),
        ("Last seen", encounter.timestamp_label.clone()),
    ];

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(area);

    let summary_lines: Vec<Line> = metrics
        .into_iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!("{label}: "), header_style()),
                Span::styled(value, value_style()),
            ])
        })
        .collect();

    let summary = Paragraph::new(summary_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(vec![Span::styled(
                    format!("Details · {}", encounter.display_title),
                    title_style(),
                )])),
        )
        .alignment(Alignment::Left);
    f.render_widget(summary, layout[0]);

    if record.rows.is_empty() {
        let block = Paragraph::new("No combatants recorded.")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, layout[1]);
    } else {
        let widths = [
            Constraint::Length(18),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
        ];

        let header = Row::new(vec![
            Cell::from("Name"),
            Cell::from("Job"),
            Cell::from("ENCDPS"),
            Cell::from("Share"),
            Cell::from("Damage"),
            Cell::from("Crit%"),
            Cell::from("DH%"),
            Cell::from("Deaths"),
        ])
        .style(header_style());

        let rows = record.rows.iter().map(|row| {
            Row::new(vec![
                Cell::from(row.name.clone()).style(Style::default().fg(job_color(&row.job))),
                Cell::from(row.job.clone()),
                Cell::from(row.encdps_str.clone()),
                Cell::from(row.share_str.clone()),
                Cell::from(row.damage_str.clone()),
                Cell::from(row.crit.clone()),
                Cell::from(row.dh.clone()),
                Cell::from(row.deaths.clone()),
            ])
        });

        let table = Table::new(rows, widths)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Combatants"))
            .column_spacing(1)
            .highlight_style(Style::default());

        f.render_widget(table, layout[1]);
    }

    let hint = Paragraph::new("← back · ↑/↓ switch encounter · Enter re-open")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(hint, layout[2]);
}
