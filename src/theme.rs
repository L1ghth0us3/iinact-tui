use ratatui::style::{Color, Style};

// Dark purple / cyberpunk palette
pub const BG: Color = Color::Rgb(18, 10, 26);
pub const PANEL: Color = Color::Rgb(28, 16, 38);
pub const ACCENT: Color = Color::Rgb(200, 60, 255); // neon purple
pub const ACCENT_2: Color = Color::Rgb(0, 255, 200); // neon cyan-green
pub const TEXT: Color = Color::Rgb(220, 210, 230);
pub const MUTED: Color = Color::Rgb(140, 120, 160);

// Simple job color suggestions tuned toward purple/cyberpunk vibe
pub fn job_color(job: &str) -> Color {
    match job {
        // Tanks
        "PLD" => Color::Rgb(180, 160, 255),
        "WAR" => Color::Rgb(255, 120, 120),
        "DRK" => Color::Rgb(150, 60, 200),
        "GNB" => Color::Rgb(200, 120, 255),
        // Healers
        "WHM" => Color::Rgb(200, 220, 255),
        "SCH" => Color::Rgb(120, 200, 255),
        "AST" => Color::Rgb(255, 180, 255),
        "SGE" => Color::Rgb(120, 255, 230),
        // Melee
        "MNK" => Color::Rgb(255, 200, 140),
        "DRG" => Color::Rgb(140, 160, 255),
        "NIN" => Color::Rgb(255, 100, 200),
        "SAM" => Color::Rgb(255, 120, 160),
        "RPR" => Color::Rgb(180, 80, 180),
        "VPR" => Color::Rgb(220, 120, 255),
        // Ranged phys
        "BRD" => Color::Rgb(255, 200, 255),
        "MCH" => Color::Rgb(160, 255, 220),
        "DNC" => Color::Rgb(255, 160, 220),
        // Casters
        "BLM" => Color::Rgb(120, 120, 255),
        "SMN" => Color::Rgb(120, 255, 160),
        "RDM" => Color::Rgb(255, 160, 200),
        "PCT" => Color::Rgb(180, 220, 255),
        // Limited
        "BLU" => Color::Rgb(140, 200, 255),
        _ => ACCENT,
    }
}

pub fn header_style() -> Style { Style::default().fg(TEXT) }
pub fn title_style() -> Style { Style::default().fg(ACCENT) }
pub fn value_style() -> Style { Style::default().fg(ACCENT_2) }

