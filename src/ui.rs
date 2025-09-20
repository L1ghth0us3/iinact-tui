use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::model::AppSnapshot;
use crate::theme::{header_style, job_color, title_style, value_style, TEXT};

pub fn draw(f: &mut Frame, s: &AppSnapshot) {
    // Split into header + table + footer/status
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(f.size());

    draw_header(f, chunks[0], s);
    draw_table(f, chunks[1], s);
    draw_status(f, chunks[2], s);
}

fn draw_header(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let block = Block::default().borders(Borders::NONE);
    let mut line = Line::default();
    line.spans.push(Span::styled(" IINACT TUI ", title_style()));

    if let Some(enc) = &s.encounter {
        line.spans.push(Span::raw(" | "));
        line.spans
            .push(Span::styled(enc.title.as_str(), header_style()));
        line.spans.push(Span::raw("  in  "));
        line.spans
            .push(Span::styled(enc.zone.as_str(), header_style()));
        line.spans.push(Span::raw("  "));
        line.spans.push(Span::styled("Dur:", header_style()));
        line.spans
            .push(Span::styled(format!(" {}", enc.duration), value_style()));
        line.spans.push(Span::raw("  "));
        line.spans.push(Span::styled("ENCDPS:", header_style()));
        line.spans
            .push(Span::styled(format!(" {}", enc.encdps), value_style()));
        line.spans.push(Span::raw("  "));
        line.spans.push(Span::styled("Damage:", header_style()));
        line.spans
            .push(Span::styled(format!(" {}", enc.damage), value_style()));
    } else {
        line.spans.push(Span::raw(" | Waiting for data..."));
    }

    let widget = Paragraph::new(line)
        .block(block)
        .style(Style::default().fg(TEXT));
    f.render_widget(widget, area);
}

fn draw_table(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let headers = Row::new([
        Cell::from("Name"),
        Cell::from("Job"),
        Cell::from("ENCDPS"),
        Cell::from("Crit%"),
        Cell::from("DH%"),
        Cell::from("Deaths"),
    ])
    .style(header_style());

    let rows = s.rows.iter().map(|r| {
        Row::new([
            Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
            Cell::from(r.job.clone()),
            Cell::from(r.encdps_str.clone()),
            Cell::from(r.crit.clone()),
            Cell::from(r.dh.clone()),
            Cell::from(r.deaths.clone()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(34),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(headers)
    .block(Block::default().borders(Borders::NONE))
    .column_spacing(1);

    f.render_widget(table, area);
}

fn draw_status(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let status = if s.connected {
        "Connected"
    } else {
        "Disconnected"
    };
    let age = s.last_update_ms;
    let line = Line::from(vec![
        Span::styled(" q ", title_style()),
        Span::styled("quit", header_style()),
        Span::raw("  |  "),
        Span::styled("Status:", header_style()),
        Span::styled(format!(" {}", status), value_style()),
        Span::raw("  "),
        Span::styled("Last Update:", header_style()),
        Span::styled(format!(" {} ms", age), value_style()),
    ]);
    let widget = Paragraph::new(line).block(Block::default().borders(Borders::NONE));
    f.render_widget(widget, area);
}
