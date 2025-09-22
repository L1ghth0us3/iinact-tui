use std::cmp::Ordering;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::model::{AppSnapshot, CombatantRow, HistoryPanelLevel, ViewMode};
use crate::theme::{header_style, title_style, value_style, TEXT};
use crate::ui::{draw_table_with_context, TableRenderContext};

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
                "← encounters · ↑/↓ switch encounter · m toggles DPS/Heal · h/Esc closes"
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
    if let Some(err) = &s.history.error {
        let block = Paragraph::new(err.as_str())
            .alignment(ratatui::layout::Alignment::Left)
            .block(Block::default().borders(Borders::ALL).title("Error"));
        f.render_widget(block, area);
        return;
    }

    let is_loading = s.history.loading;

    if s.history.days.is_empty() {
        let message = if is_loading {
            "Loading history…"
        } else {
            "No encounters recorded yet."
        };
        let block = Paragraph::new(message)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    }

    match s.history.level {
        HistoryPanelLevel::Dates => draw_dates(f, area, s),
        HistoryPanelLevel::Encounters => draw_encounters(f, area, s),
        HistoryPanelLevel::EncounterDetail => draw_encounter_detail(f, area, s),
    }

    if is_loading {
        render_loading_overlay(f, area, "Loading…");
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

    if !day.encounters_loaded && !day.encounter_ids.is_empty() {
        let block = Paragraph::new("Loading encounters…")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    }

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

    let Some(record) = encounter.record.as_ref() else {
        let block = Paragraph::new("Loading encounter…")
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Line::from(vec![Span::styled(
                        format!("Details · {}", encounter.display_title),
                        title_style(),
                    )])),
            );
        f.render_widget(block, area);
        return;
    };

    let basic_metrics = [
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
    ];

    let technical_metrics = [
        ("Snapshots", record.snapshots.to_string()),
        ("Frames", record.frames.len().to_string()),
        ("Last seen", encounter.timestamp_label.clone()),
    ];

    let summary_lines: Vec<Line> = basic_metrics
        .iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!("{label}: "), header_style()),
                Span::styled(value.clone(), value_style()),
            ])
        })
        .collect();

    let technical_lines: Vec<Line> = technical_metrics
        .iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!("{label}: "), header_style()),
                Span::styled(value.clone(), value_style()),
            ])
        })
        .collect();

    let max_summary_rows = summary_lines.len().max(technical_lines.len());
    let mut summary_height = max_summary_rows.saturating_add(2) as u16;
    let max_height = area.height.max(1u16);
    if summary_height > max_height {
        summary_height = max_height;
    }
    let min_required = 3u16.min(max_height);
    if summary_height < min_required {
        summary_height = min_required;
    }

    let detail_mode = s.history.detail_mode;
    let mut sorted_rows = record.rows.clone();
    sort_rows_for_mode(&mut sorted_rows, detail_mode);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(summary_height),
            Constraint::Min(6),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(area);

    let summary_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(layout[0]);

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
    f.render_widget(summary, summary_chunks[0]);

    let technical = Paragraph::new(technical_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(vec![Span::styled(
                    "Technical Details".to_string(),
                    title_style(),
                )])),
        )
        .alignment(Alignment::Left);
    f.render_widget(technical, summary_chunks[1]);

    if sorted_rows.is_empty() {
        let block = Paragraph::new("No combatants recorded.")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(block, layout[1]);
    } else {
        let table_title = Line::from(vec![
            Span::styled(
                format!("Combatants · {}", detail_mode.label()),
                title_style(),
            ),
            Span::raw(" "),
            Span::styled("(m toggles)", Style::default().fg(TEXT)),
        ]);
        let block = Block::default().borders(Borders::ALL).title(table_title);
        let table_area = layout[1];
        let inner = block.inner(table_area);
        f.render_widget(block, table_area);

        let ctx = TableRenderContext {
            rows: &sorted_rows,
            mode: detail_mode,
            decoration: s.decoration,
        };
        draw_table_with_context(f, inner, &ctx);
    }

    let metric_label = match detail_mode {
        ViewMode::Dps => "ENCDPS",
        ViewMode::Heal => "ENCHPS",
    };
    let metric_value = match detail_mode {
        ViewMode::Dps => &record.encounter.encdps,
        ViewMode::Heal => &record.encounter.enchps,
    };
    let total_label = match detail_mode {
        ViewMode::Dps => "Total Damage",
        ViewMode::Heal => "Total Healed",
    };
    let total_value = match detail_mode {
        ViewMode::Dps => &record.encounter.damage,
        ViewMode::Heal => &record.encounter.healed,
    };

    let metric_value = if metric_value.is_empty() {
        "—".to_string()
    } else {
        metric_value.clone()
    };
    let total_value = if total_value.is_empty() {
        "—".to_string()
    } else {
        total_value.clone()
    };

    let mode_lines = vec![
        Line::from(vec![
            Span::styled("Current: ", header_style()),
            Span::styled(detail_mode.label(), value_style()),
            Span::styled(" · press m to toggle", Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("Sorting: ", header_style()),
            Span::styled(metric_label, value_style()),
            Span::styled(" · encounter ", Style::default().fg(TEXT)),
            Span::styled(metric_label, value_style()),
            Span::styled(": ", Style::default().fg(TEXT)),
            Span::styled(metric_value, value_style()),
            Span::styled(" · ", Style::default().fg(TEXT)),
            Span::styled(total_label, header_style()),
            Span::styled(": ", Style::default().fg(TEXT)),
            Span::styled(total_value, value_style()),
        ]),
    ];

    let mode_paragraph = Paragraph::new(mode_lines).alignment(Alignment::Left).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from(vec![Span::styled("View Mode", title_style())])),
    );
    f.render_widget(mode_paragraph, layout[2]);

    let hint = Paragraph::new("← back · ↑/↓ switch encounter · m toggles DPS/Heal · Enter re-open")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(hint, layout[3]);
}

fn sort_rows_for_mode(rows: &mut [CombatantRow], mode: ViewMode) {
    match mode {
        ViewMode::Dps => rows.sort_by(|a, b| {
            b.encdps
                .partial_cmp(&a.encdps)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.name.cmp(&b.name))
        }),
        ViewMode::Heal => rows.sort_by(|a, b| {
            b.enchps
                .partial_cmp(&a.enchps)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.name.cmp(&b.name))
        }),
    }
}

fn render_loading_overlay(f: &mut Frame, area: Rect, message: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let text_width = message.chars().count() as u16 + 4;
    let overlay_width = text_width.min(area.width);
    let overlay_height = 3.min(area.height).max(1);
    let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
    let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
    let overlay = Rect {
        x,
        y,
        width: overlay_width,
        height: overlay_height,
    };
    f.render_widget(Clear, overlay);
    let block = Paragraph::new(message)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(block, overlay);
}
