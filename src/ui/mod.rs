use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::model::AppSnapshot;
use crate::{ui_history, ui_idle};

mod header;
mod settings;
mod status;
mod table;

pub fn draw(f: &mut Frame, snapshot: &AppSnapshot) {
    if snapshot.history.visible {
        ui_history::draw_history(f, snapshot);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(f.size());

    header::draw(f, chunks[0], snapshot);

    if snapshot.is_idle && snapshot.show_idle_overlay {
        ui_idle::draw_idle(f, chunks[1], snapshot);
    } else {
        table::draw(f, chunks[1], snapshot);
    }

    status::draw(f, chunks[2], snapshot);

    if snapshot.show_settings {
        settings::draw(f, snapshot);
    }
}
