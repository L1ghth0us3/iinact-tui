use std::borrow::Cow;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::model::{AppSnapshot, Decoration, ViewMode};
use crate::theme::{header_style, job_color, role_bar_color, title_style, value_style, TEXT};

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

    if s.show_settings {
        draw_settings(f, s);
    }
}

fn right_align(text: &str, width: usize) -> String {
    let len = text.len();
    if len >= width {
        // Keep rightmost content
        text.chars()
            .rev()
            .take(width)
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    } else {
        format!("{:>width$}", text, width = width)
    }
}

// inline name underline removed; inline mode now uses background meters only

fn draw_header(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let block = Block::default().borders(Borders::NONE);
    let w = area.width as usize;

    let line_top = if let Some(enc) = &s.encounter {
        // Top header now excludes Encounter/Zone; show compact metrics (DPS or HEAL mode)
        let (metric_label, metric_val, total_label, total_val) = match s.mode {
            ViewMode::Dps => ("ENCDPS", enc.encdps.as_str(), "Damage", enc.damage.as_str()),
            ViewMode::Heal => ("ENCHPS", enc.enchps.as_str(), "Healed", enc.healed.as_str()),
        };
        if w >= 56 {
            Line::from(vec![
                Span::styled("Dur:", header_style()),
                Span::styled(format!(" {} ", enc.duration), value_style()),
                Span::raw("| "),
                Span::styled(format!("{}:", metric_label), header_style()),
                Span::styled(format!(" {} ", metric_val), value_style()),
                Span::raw("| "),
                Span::styled(format!("{}:", total_label), header_style()),
                Span::styled(format!(" {}", total_val), value_style()),
            ])
        } else if w >= 40 {
            Line::from(vec![
                Span::styled("Dur:", header_style()),
                Span::styled(format!(" {} ", enc.duration), value_style()),
                Span::styled(format!("{}:", metric_label), header_style()),
                Span::styled(format!(" {}", metric_val), value_style()),
            ])
        } else if w >= 28 {
            Line::from(vec![
                Span::styled(enc.duration.as_str(), value_style()),
                Span::raw("  "),
                Span::styled(metric_val, value_style()),
            ])
        } else {
            Line::from(vec![Span::styled(metric_val, value_style())])
        }
    } else {
        Line::from(vec![Span::raw("Waiting for data...")])
    };

    // Second line: Encounter and Zone to occupy the empty header space
    let line_bottom = if let Some(enc) = &s.encounter {
        // Choose a live-friendly title: during active fights, ACT may not finalize the boss name.
        // Fall back to Zone (with an 'active' hint) to keep this line reactive.
        let display_title = if enc.title.is_empty()
            || (enc.is_active && enc.title.eq_ignore_ascii_case("Encounter"))
        {
            enc.zone.clone()
        } else {
            enc.title.clone()
        };
        if w >= 40 {
            Line::from(vec![
                Span::styled("Encounter:", header_style()),
                Span::styled(format!(" {}  ", display_title), value_style()),
                Span::styled("Zone:", header_style()),
                Span::styled(format!(" {}", enc.zone), value_style()),
            ])
        } else if w >= 24 {
            Line::from(vec![
                Span::styled("Enc:", header_style()),
                Span::styled(format!(" {}  ", display_title), value_style()),
            ])
        } else {
            Line::from(vec![])
        }
    } else {
        Line::from(vec![])
    };

    let head = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    // Swap: show Encounter/Zone on top, and Dur/ENCDPS/Damage below
    let widget_top = Paragraph::new(line_bottom)
        .block(block.clone())
        .style(Style::default().fg(TEXT))
        .alignment(Alignment::Left);
    f.render_widget(widget_top, head[0]);

    let widget_bottom = Paragraph::new(line_top)
        .block(block)
        .style(Style::default().fg(TEXT))
        .alignment(Alignment::Left);
    f.render_widget(widget_bottom, head[1]);
}

fn draw_table(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    f.render_widget(Clear, area);
    let w = area.width as usize;
    let row_h = s.decoration.row_height();

    // Breakpoints: progressively hide columns on narrow terminals
    enum Variant {
        Full,
        NoDeaths,
        NoDHDeaths,
        Minimal,
        NameOnly,
    }
    let variant = if w >= 90 {
        Variant::Full
    } else if w >= 72 {
        Variant::NoDeaths
    } else if w >= 58 {
        Variant::NoDHDeaths
    } else if w >= 44 {
        Variant::Minimal
    } else {
        Variant::NameOnly
    };

    // Draw background meters first (behind text) when enabled
    if matches!(s.decoration, Decoration::Background) {
        draw_bg_meters(f, area, s);
    }

    let heal_mode = matches!(s.mode, ViewMode::Heal);
    match variant {
        Variant::Full => {
            let headers = if !heal_mode {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Share%", 7)),
                    Cell::from(right_align("ENCDPS", 10)),
                    Cell::from(right_align("Job", 5)),
                    Cell::from(right_align("Crit%", 8)),
                    Cell::from(right_align("DH%", 8)),
                    Cell::from(right_align("Deaths", 8)),
                ])
            } else {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Heal%", 7)),
                    Cell::from(right_align("ENCHPS", 10)),
                    Cell::from(right_align("Job", 5)),
                    Cell::from(right_align("Overheal%", 10)),
                    Cell::from(right_align("Deaths", 8)),
                ])
            }
            .height(2)
            .style(header_style());
            let rows = s.rows.iter().map(|r| {
                if !heal_mode {
                    let share = right_align(&r.share_str, 7);
                    let enc = right_align(&r.encdps_str, 10);
                    let job = right_align(&r.job, 5);
                    let crit = right_align(&r.crit, 8);
                    let dh = right_align(&r.dh, 8);
                    let deaths = right_align(&r.deaths, 8);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                        Cell::from(job),
                        Cell::from(crit),
                        Cell::from(dh),
                        Cell::from(deaths),
                    ])
                } else {
                    let share = right_align(&r.heal_share_str, 7);
                    let enc = right_align(&r.enchps_str, 10);
                    let job = right_align(&r.job, 5);
                    let oh = right_align(&r.overheal_pct, 10);
                    let deaths = right_align(&r.deaths, 8);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                        Cell::from(job),
                        Cell::from(oh),
                        Cell::from(deaths),
                    ])
                }
                .height(row_h)
            });
            let widths: Vec<Constraint> = if !heal_mode {
                vec![
                    Constraint::Percentage(34),
                    Constraint::Length(7),
                    Constraint::Length(10),
                    Constraint::Length(5),
                    Constraint::Length(8),
                    Constraint::Length(8),
                    Constraint::Length(8),
                ]
            } else {
                vec![
                    Constraint::Percentage(34),
                    Constraint::Length(7),
                    Constraint::Length(10),
                    Constraint::Length(5),
                    Constraint::Length(10),
                    Constraint::Length(8),
                ]
            };
            let table = Table::new(rows, widths)
                .header(headers)
                .block(Block::default().borders(Borders::NONE))
                .column_spacing(1);
            f.render_widget(table, area);
        }
        Variant::NoDeaths => {
            let headers = if !heal_mode {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Share%", 7)),
                    Cell::from(right_align("ENCDPS", 9)),
                    Cell::from(right_align("Job", 5)),
                    Cell::from(right_align("Crit%", 6)),
                    Cell::from(right_align("DH%", 6)),
                ])
            } else {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Heal%", 7)),
                    Cell::from(right_align("ENCHPS", 9)),
                    Cell::from(right_align("Job", 5)),
                    Cell::from(right_align("Overheal%", 9)),
                ])
            }
            .height(2)
            .style(header_style());
            let rows = s.rows.iter().map(|r| {
                if !heal_mode {
                    let share = right_align(&r.share_str, 7);
                    let enc = right_align(&r.encdps_str, 9);
                    let job = right_align(&r.job, 5);
                    let crit = right_align(&r.crit, 6);
                    let dh = right_align(&r.dh, 6);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                        Cell::from(job),
                        Cell::from(crit),
                        Cell::from(dh),
                    ])
                } else {
                    let share = right_align(&r.heal_share_str, 7);
                    let enc = right_align(&r.enchps_str, 9);
                    let job = right_align(&r.job, 5);
                    let oh = right_align(&r.overheal_pct, 9);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                        Cell::from(job),
                        Cell::from(oh),
                    ])
                }
                .height(row_h)
            });
            let widths: Vec<Constraint> = if !heal_mode {
                vec![
                    Constraint::Percentage(38),
                    Constraint::Length(7),
                    Constraint::Length(9),
                    Constraint::Length(5),
                    Constraint::Length(6),
                    Constraint::Length(6),
                ]
            } else {
                vec![
                    Constraint::Percentage(44),
                    Constraint::Length(7),
                    Constraint::Length(9),
                    Constraint::Length(5),
                    Constraint::Length(9),
                ]
            };
            let table = Table::new(rows, widths)
                .header(headers)
                .block(Block::default().borders(Borders::NONE))
                .column_spacing(1);
            f.render_widget(table, area);
        }
        Variant::NoDHDeaths => {
            let headers = if !heal_mode {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Share%", 7)),
                    Cell::from(right_align("ENCDPS", 9)),
                    Cell::from(right_align("Crit%", 6)),
                ])
            } else {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Heal%", 7)),
                    Cell::from(right_align("ENCHPS", 9)),
                    Cell::from(right_align("Job", 5)),
                ])
            }
            .height(2)
            .style(header_style());
            let rows = s.rows.iter().map(|r| {
                if !heal_mode {
                    let share = right_align(&r.share_str, 7);
                    let enc = right_align(&r.encdps_str, 9);
                    let crit = right_align(&r.crit, 6);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                        Cell::from(crit),
                    ])
                } else {
                    let share = right_align(&r.heal_share_str, 7);
                    let enc = right_align(&r.enchps_str, 9);
                    let job = right_align(&r.job, 5);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                        Cell::from(job),
                    ])
                }
                .height(row_h)
            });
            let table = Table::new(
                rows,
                if !heal_mode {
                    [
                        Constraint::Percentage(54),
                        Constraint::Length(7),
                        Constraint::Length(9),
                        Constraint::Length(6),
                    ]
                } else {
                    [
                        Constraint::Percentage(58),
                        Constraint::Length(7),
                        Constraint::Length(9),
                        Constraint::Length(5),
                    ]
                },
            )
            .header(headers)
            .block(Block::default().borders(Borders::NONE))
            .column_spacing(1);
            f.render_widget(table, area);
        }
        Variant::Minimal => {
            let headers = if !heal_mode {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Share%", 6)),
                    Cell::from(right_align("ENCDPS", 9)),
                ])
            } else {
                Row::new([
                    Cell::from("Name"),
                    Cell::from(right_align("Heal%", 6)),
                    Cell::from(right_align("ENCHPS", 9)),
                ])
            }
            .height(2)
            .style(header_style());
            let rows = s.rows.iter().map(|r| {
                if !heal_mode {
                    let share = right_align(&r.share_str, 6);
                    let enc = right_align(&r.encdps_str, 9);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                    ])
                } else {
                    let share = right_align(&r.heal_share_str, 6);
                    let enc = right_align(&r.enchps_str, 9);
                    Row::new([
                        Cell::from(r.name.clone()).style(Style::default().fg(job_color(&r.job))),
                        Cell::from(share),
                        Cell::from(enc),
                    ])
                }
                .height(row_h)
            });
            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(64),
                    Constraint::Length(6),
                    Constraint::Length(9),
                ],
            )
            .header(headers)
            .block(Block::default().borders(Borders::NONE))
            .column_spacing(1);
            f.render_widget(table, area);
        }
        Variant::NameOnly => {
            // Compose a single column: "Name  [Share%]"
            let headers = Row::new([Cell::from(if !heal_mode {
                "Name (Share%)"
            } else {
                "Name (Heal%)"
            })])
            .height(2)
            .style(header_style());
            let rows = s.rows.iter().map(|r| {
                let text = if !heal_mode {
                    format!("{}  [{}]", r.name, r.share_str)
                } else {
                    format!("{}  [{}]", r.name, r.heal_share_str)
                };
                Row::new([Cell::from(text).style(Style::default().fg(job_color(&r.job)))])
                    .height(row_h)
            });
            let table = Table::new(rows, [Constraint::Percentage(100)])
                .header(headers)
                .block(Block::default().borders(Borders::NONE))
                .column_spacing(0);
            f.render_widget(table, area);
        }
    }

    // Always draw the thin header separator after rendering the table
    if area.height >= 2 {
        let sep_y = area.y.saturating_add(1);
        if sep_y < area.y + area.height {
            let width = area.width as usize;
            let line = "─".repeat(width);
            let rect = Rect {
                x: area.x,
                y: sep_y,
                width: area.width,
                height: 1,
            };
            let sep = Paragraph::new(Line::from(Span::styled(
                line,
                Style::default().fg(ratatui::style::Color::Rgb(170, 170, 180)),
            )));
            f.render_widget(sep, rect);
        }
    }

    // In underline mode, draw thin underline bars under each row
    if matches!(s.decoration, Decoration::Underline) {
        draw_underlines(f, area, s);
    }
}

fn draw_status(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    let (status_text, status_style) = if !s.connected {
        (
            Cow::Borrowed("Disconnected"),
            Style::default().fg(crate::theme::STATUS_DISCONNECTED),
        )
    } else if s.is_idle {
        (
            Cow::Borrowed("Connected (idle)"),
            Style::default().fg(crate::theme::STATUS_IDLE),
        )
    } else {
        (Cow::Borrowed("Connected"), value_style())
    };
    let status_span = Span::styled(status_text.clone(), status_style);
    let decor_label = s.decoration.short_label().trim_start_matches("decor:");
    let mode_label = s.mode.short_label().trim_start_matches("mode:");
    let w = area.width as usize;

    // Responsive footer variants, left-aligned
    let line = if w >= 90 {
        Line::from(vec![
            Span::styled(" q ", title_style()),
            Span::styled("quit", header_style()),
            Span::raw(" | "),
            Span::styled(" d ", title_style()),
            Span::styled(decor_label, header_style()),
            Span::raw(" | "),
            Span::styled(" m ", title_style()),
            Span::styled(mode_label, header_style()),
            Span::raw(" | "),
            Span::styled(" s ", title_style()),
            Span::styled("settings", header_style()),
            Span::raw(" | "),
            Span::styled("status", header_style()),
            Span::raw(" "),
            status_span.clone(),
        ])
    } else if w >= 60 {
        Line::from(vec![
            Span::styled(" q ", title_style()),
            Span::styled("quit", header_style()),
            Span::raw(" | "),
            Span::styled(" d ", title_style()),
            Span::styled(decor_label, header_style()),
            Span::raw(" | "),
            Span::styled(" m ", title_style()),
            Span::styled(mode_label, header_style()),
            Span::raw(" | "),
            Span::styled(" s ", title_style()),
            Span::styled("settings", header_style()),
            Span::raw(" | "),
            status_span.clone(),
        ])
    } else if w >= 36 {
        Line::from(vec![
            Span::styled(" q ", title_style()),
            Span::styled(" d ", title_style()),
            Span::styled(" m ", title_style()),
            Span::styled(" s ", title_style()),
            status_span,
        ])
    } else {
        Line::from(vec![Span::styled("qdms", title_style())])
    };

    let widget = Paragraph::new(line)
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Left);
    f.render_widget(widget, area);
}

fn draw_settings(f: &mut Frame, s: &AppSnapshot) {
    let area = centered_rect(60, 50, f.size());
    f.render_widget(Clear, area);

    let idle_line = Line::from(vec![
        Span::styled("Idle timeout:", header_style()),
        Span::raw(" "),
        Span::styled(format!("{}s", s.settings.idle_seconds), value_style()),
    ]);
    let hint_line = Line::from(vec![Span::styled(
        "Use ↑/↓ to adjust. Press 's' to close.",
        header_style(),
    )]);

    let lines = vec![
        Line::from(vec![Span::styled("Settings", title_style())]),
        Line::default(),
        idle_line,
        Line::default(),
        hint_line,
    ];

    let block = Block::default().title("Settings").borders(Borders::ALL);
    let widget = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);
    f.render_widget(widget, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(area);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(horizontal[1]);
    vertical[1]
}

fn draw_bg_meters(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    if area.height <= 2 {
        return;
    }
    // Determine max ENCDPS to scale bars
    let max_dps = s
        .rows
        .iter()
        .map(|r| r.encdps)
        .fold(0.0_f64, |a, b| a.max(b));
    if max_dps <= 0.0 {
        return;
    }
    let header_lines = 2u16; // table header consumes 2 lines
    let width = area.width as usize;
    let visible_rows = (area.height.saturating_sub(header_lines)) as usize;
    for (i, r) in s.rows.iter().take(visible_rows).enumerate() {
        let ratio = (r.encdps / max_dps).clamp(0.0, 1.0);
        let filled = (ratio * width as f64).round() as usize;
        let y = area.y + header_lines + i as u16; // row text line
        if y >= area.y + area.height {
            break;
        }
        let rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };
        let mut spans: Vec<Span> = Vec::with_capacity(2);
        if filled > 0 {
            spans.push(Span::styled(
                " ".repeat(filled),
                Style::default().bg(role_bar_color(&r.job)),
            ));
        }
        if width > filled {
            spans.push(Span::raw(" ".repeat(width - filled)));
        }
        let bg = Paragraph::new(Line::from(spans));
        f.render_widget(bg, rect);
    }
}

#[allow(dead_code)]
fn draw_underlines(f: &mut Frame, area: Rect, s: &AppSnapshot) {
    if area.height <= 1 {
        return;
    }
    let max_dps = s
        .rows
        .iter()
        .map(|r| r.encdps)
        .fold(0.0_f64, |a, b| if b > a { b } else { a });
    if max_dps <= 0.0 {
        return;
    }

    // Header consumes 2 lines; each row consumes 2 lines; underline on the second line
    let header_lines = 2u16;
    let usable_height = area.height.saturating_sub(header_lines);
    let visible_rows = (usable_height / 2) as usize;
    let width = area.width as usize;

    for (i, r) in s.rows.iter().take(visible_rows).enumerate() {
        let ratio = (r.encdps / max_dps).clamp(0.0, 1.0);
        let filled = (ratio * width as f64).round() as usize;
        let y = area.y + header_lines + (i as u16) * 2 + 1; // line directly under row
        if y >= area.y + area.height {
            break;
        }
        let bar_rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };

        // Solid role-colored bar, no gradient
        let mut line = String::with_capacity(width);
        for _ in 0..filled {
            line.push('▔');
        }
        for _ in filled..width {
            line.push(' ');
        }
        let para = Paragraph::new(Line::from(Span::styled(
            line,
            Style::default().fg(role_bar_color(&r.job)),
        )));

        f.render_widget(para, bar_rect);
    }
}
