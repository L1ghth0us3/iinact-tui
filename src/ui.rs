use std::borrow::Cow;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::model::{AppSnapshot, CombatantRow, Decoration, SettingsField, ViewMode};
use crate::theme::{header_style, job_color, role_bar_color, title_style, value_style, TEXT};
use crate::ui_idle;

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
    if s.is_idle {
        ui_idle::draw_idle(f, chunks[1], s);
    } else {
        draw_table(f, chunks[1], s);
    }
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

#[derive(Copy, Clone)]
enum TableVariant {
    Full,
    NoDeaths,
    NoDhDeaths,
    Minimal,
    NameOnly,
}

impl TableVariant {
    fn from_width(width: usize) -> Self {
        if width >= 90 {
            TableVariant::Full
        } else if width >= 72 {
            TableVariant::NoDeaths
        } else if width >= 58 {
            TableVariant::NoDhDeaths
        } else if width >= 44 {
            TableVariant::Minimal
        } else {
            TableVariant::NameOnly
        }
    }
}

enum Align {
    Left,
    Right { width: usize },
}

impl Align {
    fn format(&self, text: &str) -> String {
        match self {
            Align::Left => text.to_string(),
            Align::Right { width } => right_align(text, *width),
        }
    }
}

struct ColumnSpec {
    header: &'static str,
    align: Align,
    width: Constraint,
    value: fn(&CombatantRow) -> String,
    style: Option<fn(&CombatantRow) -> Style>,
}

impl ColumnSpec {
    fn header_cell(&self) -> Cell<'static> {
        Cell::from(self.align.format(self.header))
    }

    fn data_cell(&self, row: &CombatantRow) -> Cell<'static> {
        let text = (self.value)(row);
        let formatted = self.align.format(&text);
        let mut cell = Cell::from(formatted);
        if let Some(style_fn) = self.style {
            cell = cell.style(style_fn(row));
        }
        cell
    }
}

struct LayoutSpec {
    columns: Vec<ColumnSpec>,
    header_height: u16,
    column_spacing: u16,
}

impl LayoutSpec {
    fn new(columns: Vec<ColumnSpec>) -> Self {
        LayoutSpec {
            columns,
            header_height: 2,
            column_spacing: 1,
        }
    }

    fn with_spacing(mut self, spacing: u16) -> Self {
        self.column_spacing = spacing;
        self
    }

    fn header_row(&self) -> Row<'static> {
        Row::new(self.columns.iter().map(|col| col.header_cell()))
            .style(header_style())
            .height(self.header_height)
    }

    fn data_row(&self, row: &CombatantRow, row_height: u16) -> Row<'static> {
        Row::new(self.columns.iter().map(|col| col.data_cell(row))).height(row_height)
    }

    fn widths(&self) -> Vec<Constraint> {
        self.columns.iter().map(|col| col.width).collect()
    }
}

fn name_style(row: &CombatantRow) -> Style {
    Style::default().fg(job_color(&row.job))
}

fn value_name(row: &CombatantRow) -> String {
    row.name.clone()
}

fn value_share(row: &CombatantRow) -> String {
    row.share_str.clone()
}

fn value_heal_share(row: &CombatantRow) -> String {
    row.heal_share_str.clone()
}

fn value_encdps(row: &CombatantRow) -> String {
    row.encdps_str.clone()
}

fn value_enchps(row: &CombatantRow) -> String {
    row.enchps_str.clone()
}

fn value_job(row: &CombatantRow) -> String {
    row.job.clone()
}

fn value_crit(row: &CombatantRow) -> String {
    row.crit.clone()
}

fn value_dh(row: &CombatantRow) -> String {
    row.dh.clone()
}

fn value_deaths(row: &CombatantRow) -> String {
    row.deaths.clone()
}

fn value_overheal(row: &CombatantRow) -> String {
    row.overheal_pct.clone()
}

fn value_name_with_share(row: &CombatantRow) -> String {
    format!("{}  [{}]", row.name, row.share_str)
}

fn value_name_with_heal_share(row: &CombatantRow) -> String {
    format!("{}  [{}]", row.name, row.heal_share_str)
}

fn name_column(width: Constraint) -> ColumnSpec {
    ColumnSpec {
        header: "Name",
        align: Align::Left,
        width,
        value: value_name,
        style: Some(name_style),
    }
}

fn right_column(
    header: &'static str,
    align_width: usize,
    width: Constraint,
    value: fn(&CombatantRow) -> String,
) -> ColumnSpec {
    ColumnSpec {
        header,
        align: Align::Right { width: align_width },
        width,
        value,
        style: None,
    }
}

fn left_column(
    header: &'static str,
    width: Constraint,
    value: fn(&CombatantRow) -> String,
    style: Option<fn(&CombatantRow) -> Style>,
) -> ColumnSpec {
    ColumnSpec {
        header,
        align: Align::Left,
        width,
        value,
        style,
    }
}

fn layout_for(mode: ViewMode, variant: TableVariant) -> LayoutSpec {
    match (mode, variant) {
        (ViewMode::Dps, TableVariant::Full) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(34)),
            right_column("Share%", 7, Constraint::Length(7), value_share),
            right_column("ENCDPS", 10, Constraint::Length(10), value_encdps),
            right_column("Job", 5, Constraint::Length(5), value_job),
            right_column("Crit%", 8, Constraint::Length(8), value_crit),
            right_column("DH%", 8, Constraint::Length(8), value_dh),
            right_column("Deaths", 8, Constraint::Length(8), value_deaths),
        ]),
        (ViewMode::Heal, TableVariant::Full) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(34)),
            right_column("Heal%", 7, Constraint::Length(7), value_heal_share),
            right_column("ENCHPS", 10, Constraint::Length(10), value_enchps),
            right_column("Job", 5, Constraint::Length(5), value_job),
            right_column("Overheal%", 10, Constraint::Length(10), value_overheal),
            right_column("Deaths", 8, Constraint::Length(8), value_deaths),
        ]),
        (ViewMode::Dps, TableVariant::NoDeaths) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(38)),
            right_column("Share%", 7, Constraint::Length(7), value_share),
            right_column("ENCDPS", 9, Constraint::Length(9), value_encdps),
            right_column("Job", 5, Constraint::Length(5), value_job),
            right_column("Crit%", 6, Constraint::Length(6), value_crit),
            right_column("DH%", 6, Constraint::Length(6), value_dh),
        ]),
        (ViewMode::Heal, TableVariant::NoDeaths) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(44)),
            right_column("Heal%", 7, Constraint::Length(7), value_heal_share),
            right_column("ENCHPS", 9, Constraint::Length(9), value_enchps),
            right_column("Job", 5, Constraint::Length(5), value_job),
            right_column("Overheal%", 9, Constraint::Length(9), value_overheal),
        ]),
        (ViewMode::Dps, TableVariant::NoDhDeaths) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(54)),
            right_column("Share%", 7, Constraint::Length(7), value_share),
            right_column("ENCDPS", 9, Constraint::Length(9), value_encdps),
            right_column("Crit%", 6, Constraint::Length(6), value_crit),
        ]),
        (ViewMode::Heal, TableVariant::NoDhDeaths) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(58)),
            right_column("Heal%", 7, Constraint::Length(7), value_heal_share),
            right_column("ENCHPS", 9, Constraint::Length(9), value_enchps),
            right_column("Job", 5, Constraint::Length(5), value_job),
        ]),
        (ViewMode::Dps, TableVariant::Minimal) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(64)),
            right_column("Share%", 6, Constraint::Length(6), value_share),
            right_column("ENCDPS", 9, Constraint::Length(9), value_encdps),
        ]),
        (ViewMode::Heal, TableVariant::Minimal) => LayoutSpec::new(vec![
            name_column(Constraint::Percentage(64)),
            right_column("Heal%", 6, Constraint::Length(6), value_heal_share),
            right_column("ENCHPS", 9, Constraint::Length(9), value_enchps),
        ]),
        (ViewMode::Dps, TableVariant::NameOnly) => LayoutSpec::new(vec![left_column(
            "Name (Share%)",
            Constraint::Percentage(100),
            value_name_with_share,
            Some(name_style),
        )])
        .with_spacing(0),
        (ViewMode::Heal, TableVariant::NameOnly) => LayoutSpec::new(vec![left_column(
            "Name (Heal%)",
            Constraint::Percentage(100),
            value_name_with_heal_share,
            Some(name_style),
        )])
        .with_spacing(0),
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
    let variant = TableVariant::from_width(w);
    let layout = layout_for(s.mode, variant);

    if matches!(s.decoration, Decoration::Background) {
        draw_bg_meters(f, area, s, layout.header_height);
    }

    let table = Table::new(
        s.rows.iter().map(|r| layout.data_row(r, row_h)),
        layout.widths(),
    )
    .header(layout.header_row())
    .block(Block::default().borders(Borders::NONE))
    .column_spacing(layout.column_spacing);

    f.render_widget(table, area);

    // Draw the thin header separator after rendering the table
    if area.height > layout.header_height && layout.header_height > 0 {
        let sep_offset = layout.header_height.saturating_sub(1);
        let sep_y = area.y.saturating_add(sep_offset);
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

    if matches!(s.decoration, Decoration::Underline) {
        draw_underlines(f, area, s, layout.header_height);
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

    let idle_selected = matches!(s.settings_cursor, SettingsField::IdleTimeout);
    let decor_selected = matches!(s.settings_cursor, SettingsField::DefaultDecoration);
    let mode_selected = matches!(s.settings_cursor, SettingsField::DefaultMode);

    let mut lines = Vec::new();
    lines.push(Line::from(vec![Span::styled("Settings", title_style())]));
    lines.push(Line::default());

    lines.push(setting_line(
        idle_selected,
        "Idle timeout",
        format!("{}s", s.settings.idle_seconds),
    ));
    lines.push(Line::from(vec![
        Span::raw("   "),
        Span::styled("Set to 0 to disable idle mode.", header_style()),
    ]));
    lines.push(Line::default());

    lines.push(setting_line(
        decor_selected,
        "Default decoration",
        s.settings.default_decoration.label().to_string(),
    ));
    lines.push(setting_line(
        mode_selected,
        "Default mode",
        s.settings.default_mode.label().to_string(),
    ));
    lines.push(Line::default());

    lines.push(Line::from(vec![Span::styled(
        "Use ↑/↓ to select, ←/→ to adjust. Press 's' to close.",
        header_style(),
    )]));

    let block = Block::default().title("Settings").borders(Borders::ALL);
    let widget = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);
    f.render_widget(widget, area);
}

fn setting_line(selected: bool, label: &str, value: String) -> Line {
    let marker = if selected { "▶" } else { " " };
    let label_style = if selected {
        title_style()
    } else {
        header_style()
    };
    Line::from(vec![
        Span::styled(format!("{} {}:", marker, label), label_style),
        Span::raw(" "),
        Span::styled(value, value_style()),
    ])
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

fn draw_bg_meters(f: &mut Frame, area: Rect, s: &AppSnapshot, header_lines: u16) {
    if area.height <= header_lines {
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
fn draw_underlines(f: &mut Frame, area: Rect, s: &AppSnapshot, header_lines: u16) {
    if area.height <= header_lines {
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

    // Each row consumes 2 lines; underline on the second line
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
