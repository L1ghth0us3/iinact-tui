use ratatui::style::{Color, Style};

// Dark purple / cyberpunk palette (foreground-only to preserve terminal background)
pub const ACCENT: Color = Color::Rgb(200, 60, 255); // neon purple
pub const ACCENT_2: Color = Color::Rgb(0, 255, 200); // neon cyan-green
pub const TEXT: Color = Color::Rgb(220, 210, 230);

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

pub fn header_style() -> Style {
    Style::default().fg(TEXT)
}
pub fn title_style() -> Style {
    Style::default().fg(ACCENT)
}
pub fn value_style() -> Style {
    Style::default().fg(ACCENT_2)
}

// Role-based color for DPS bars (xterm 256-indexed colors)
// Tanks → blue(75), Healers → green(41), DPS → red(124)
pub fn role_bar_color(job: &str) -> Color {
    match job {
        // Tanks
        "PLD" | "WAR" | "DRK" | "GNB" => Color::Indexed(75),
        // Healers
        "WHM" | "SCH" | "AST" | "SGE" => Color::Indexed(41),
        // Everything else treated as DPS
        _ => Color::Indexed(124),
    }
}

// 4-step dimming palettes per role (bright → dim) using xterm 256 indices
#[allow(dead_code)]
pub fn role_bar_palette(job: &str) -> [Color; 4] {
    match job {
        // Tanks (blue family)
        "PLD" | "WAR" | "DRK" | "GNB" => [
            Color::Indexed(75),
            Color::Indexed(69),
            Color::Indexed(63),
            Color::Indexed(57),
        ],
        // Healers (green/cyan family)
        "WHM" | "SCH" | "AST" | "SGE" => [
            Color::Indexed(41),
            Color::Indexed(40),
            Color::Indexed(35),
            Color::Indexed(29),
        ],
        // DPS (red/magenta family)
        _ => [
            Color::Indexed(124),
            Color::Indexed(88),
            Color::Indexed(52),
            Color::Indexed(1),
        ],
    }
}

// Base RGB for role bars (approximate xterm colors)
pub fn role_bar_rgb(job: &str) -> (u8, u8, u8) {
    match job {
        // Tanks → blue(75)
        "PLD" | "WAR" | "DRK" | "GNB" => (95, 135, 255),
        // Healers → green(41)
        "WHM" | "SCH" | "AST" | "SGE" => (0, 215, 95),
        // DPS → red(124)
        _ => (175, 0, 0),
    }
}

pub fn text_rgb() -> (u8, u8, u8) {
    (220, 210, 230)
}

pub fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    let (ar, ag, ab) = a;
    let (br, bg, bb) = b;
    let tr = (ar as f32 + (br as f32 - ar as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8;
    let tg = (ag as f32 + (bg as f32 - ag as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8;
    let tb = (ab as f32 + (bb as f32 - ab as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8;
    Color::Rgb(tr, tg, tb)
}
