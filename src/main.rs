use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use rand::seq::SliceRandom;
use rand::Rng;
use serde::Deserialize;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

// ─── Data ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct QBank {
    #[serde(default)]
    name: String,
    questions: Vec<Question>,
}

#[derive(Deserialize, Clone)]
struct Question {
    domain: String,
    question: String,
    answers: HashMap<String, String>,
    correct: String,
    explanation: String,
}

#[derive(Clone, Copy, PartialEq)]
enum GameMode {
    Normal,
    Beat,
}

struct BeatState {
    score: u64,
    combo: usize,
    max_combo: usize,
    correct: usize,
    wrong: usize,
    timeouts: usize,
    response_times: Vec<f64>,
    last_pts: u64,
}

impl BeatState {
    fn new() -> Self {
        BeatState {
            score: 0,
            combo: 0,
            max_combo: 0,
            correct: 0,
            wrong: 0,
            timeouts: 0,
            response_times: Vec::new(),
            last_pts: 0,
        }
    }

    fn combo_label(&self) -> String {
        match self.combo {
            0 => String::new(),
            1 => "×1".into(),
            2..=3 => format!("×{}  HOT", self.combo),
            4..=5 => format!("×{}  ON FIRE", self.combo),
            6..=9 => format!("×{}  UNSTOPPABLE", self.combo),
            _ => format!("×{}  MAX COMBO", self.combo),
        }
    }

    fn combo_color(&self) -> Color {
        match self.combo {
            0..=1 => Color::White,
            2..=3 => Color::Yellow,
            4..=5 => Color::Magenta,
            _ => Color::Red,
        }
    }

    fn calc_points(elapsed: f64, limit: f64, combo: usize) -> u64 {
        let pct_remaining = ((limit - elapsed) / limit).clamp(0.0, 1.0);
        let time_mult = 1.0 + pct_remaining * 4.0; // 1x slow → 5x instant
        let combo_mult = match combo {
            0..=1 => 1.0f64,
            2..=3 => 1.5,
            4..=5 => 2.0,
            6..=9 => 2.5,
            _ => 3.0,
        };
        (100.0 * time_mult * combo_mult).round() as u64
    }

    fn avg_time(&self) -> f64 {
        if self.response_times.is_empty() {
            0.0
        } else {
            self.response_times.iter().sum::<f64>() / self.response_times.len() as f64
        }
    }
}

const BEAT_TIME: f64 = 20.0; // seconds per question

// ─── Strings ─────────────────────────────────────────────────────────────────

const WRONG_MSGS: &[&str] = &[
    "Nope. Not even close, champ.",
    "Oof. That one hurt to watch.",
    "Wrong! But your confidence was admirable.",
    "Almost... just kidding, not at all.",
    "CompTIA sends their condolences.",
    "Error 404: Correct answer not found in your brain.",
    "That's a paddlin'.",
    "Bold choice. Security+ disagrees.",
    "The answer was RIGHT THERE.",
    "Your future self just cringed.",
    "Incorrect! But you're still a good person.",
    "Nice try. No points though.",
    "The judges have deliberated: no.",
    "Wrong answer speedrun any%",
    "That answer has left the building.",
    "Have you tried reading the study guide?",
    "Wow. Just... wow.",
];

const CORRECT_BANNERS: &[&str] = &[
    "✓  CORRECT!",
    "★  NAILED IT!",
    "◆  BOOM! RIGHT!",
    "●  LET'S GO!",
    "▲  LOCKED IN!",
    "★  SECURITY+ ENERGY!",
];

// fuse spark chars — cycles on each tick
const SPARK: &[char] = &['✸', '✺', '◈', '*', '✦', '·', '✸', '◈'];

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn word_wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current.clone());
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn wait_for_key() {
    loop {
        if let Ok(Event::Key(_)) = event::read() {
            break;
        }
    }
}

fn color_for_pct(pct: usize) -> Color {
    if pct >= 75 {
        Color::Green
    } else if pct >= 60 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn fmt_score(n: u64) -> String {
    // add thousands separators
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

// ─── Drawing ─────────────────────────────────────────────────────────────────

fn draw_header_normal(out: &mut impl Write, width: u16, q_num: usize, total: usize, score: usize) {
    let completed = q_num.saturating_sub(1);
    let pct = if completed > 0 { score * 100 / completed } else { 0 };
    let bar = format!(
        " ╔══ QUIZZICAL ══╗  Q {}/{} │ Score: {}/{} ({}%)",
        q_num, total, score, completed, pct
    );
    let _ = queue!(
        out,
        MoveTo(0, 0),
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        Print(format!("{:<width$}", bar, width = width as usize)),
        ResetColor
    );
}

fn draw_header_beat(out: &mut impl Write, width: u16, q_num: usize, total: usize, bs: &BeatState) {
    let combo_str = if bs.combo >= 2 {
        format!(" │ {}", bs.combo_label())
    } else {
        String::new()
    };
    let bar = format!(
        " ⚡ BEAT MODE  Q {}/{}  │  {} pts{}  ",
        q_num,
        total,
        fmt_score(bs.score),
        combo_str,
    );
    let combo_color = bs.combo_color();
    let _ = queue!(
        out,
        MoveTo(0, 0),
        SetForegroundColor(combo_color),
        SetAttribute(Attribute::Bold),
        Print(format!("{:<width$}", bar, width = width as usize)),
        ResetColor
    );
}

fn draw_progress_bar(out: &mut impl Write, x: u16, y: u16, width: u16, done: usize, total: usize) {
    let bar_w = width as usize;
    let filled = if total > 0 { done * bar_w / total } else { 0 };
    let bar: String = (0..bar_w)
        .map(|i| if i < filled { '█' } else { '░' })
        .collect();
    let _ = queue!(
        out,
        MoveTo(x, y),
        SetForegroundColor(Color::DarkGrey),
        Print(bar),
        ResetColor
    );
}

/// Draw the burning fuse at row `y`.
/// pct_remaining: 1.0 = full time, 0.0 = time's up.
/// tick: increments ~20/sec, used for spark animation.
fn draw_fuse(out: &mut impl Write, width: u16, y: u16, pct_remaining: f64, tick: usize) {
    // Layout: " [!] " + fuel + spark + ash + "  " + time_str
    let prefix = " [!] ";
    let time_secs = pct_remaining * BEAT_TIME;
    let time_str = format!(" {:4.1}s ", time_secs);
    let fuse_w = (width as usize)
        .saturating_sub(prefix.len() + time_str.len())
        .max(3);

    // fuel is to the LEFT of spark (toward bomb), ash to the RIGHT (already burned)
    // spark starts at right (full time) and moves left toward bomb as time runs out
    let fuel_w = ((pct_remaining * fuse_w as f64) as usize).min(fuse_w.saturating_sub(1));
    let ash_w = fuse_w.saturating_sub(1 + fuel_w);

    let spark_ch = SPARK[tick % SPARK.len()];

    // color transitions as time runs out
    let (fuel_color, spark_color, fuel_ch) = if pct_remaining > 0.5 {
        (Color::Yellow, Color::Yellow, '━')
    } else if pct_remaining > 0.25 {
        (Color::DarkYellow, Color::Yellow, '━')
    } else if pct_remaining > 0.10 {
        (Color::Red, Color::Red, '≈')
    } else {
        // critical: alternate characters for flicker
        let ch = if tick % 2 == 0 { '≈' } else { '~' };
        (Color::Red, Color::Red, ch)
    };

    // bomb label — flashes red when critical
    let bomb_color = if pct_remaining <= 0.25 && tick % 2 == 0 {
        Color::Red
    } else {
        Color::DarkGrey
    };

    // clear the fuse row first
    let _ = queue!(out, MoveTo(0, y), Print(" ".repeat(width as usize)));

    // bomb label
    let _ = queue!(
        out,
        MoveTo(0, y),
        SetForegroundColor(bomb_color),
        SetAttribute(Attribute::Bold),
        Print(prefix),
        ResetColor
    );

    let fuse_x = prefix.len() as u16;

    // fuel (left side, remaining time)
    if fuel_w > 0 {
        let fuel: String = std::iter::repeat(fuel_ch).take(fuel_w).collect();
        let bold = if pct_remaining <= 0.10 {
            Attribute::Bold
        } else {
            Attribute::Reset
        };
        let _ = queue!(
            out,
            MoveTo(fuse_x, y),
            SetForegroundColor(fuel_color),
            SetAttribute(bold),
            Print(&fuel),
            ResetColor
        );
    }

    // spark
    let spark_x = fuse_x + fuel_w as u16;
    let _ = queue!(
        out,
        MoveTo(spark_x, y),
        SetForegroundColor(spark_color),
        SetAttribute(Attribute::Bold),
        Print(spark_ch),
        ResetColor
    );

    // ash (right side, elapsed time)
    if ash_w > 0 {
        let ash: String = std::iter::repeat('╌').take(ash_w).collect();
        let _ = queue!(
            out,
            MoveTo(spark_x + 1, y),
            SetForegroundColor(Color::DarkGrey),
            Print(&ash),
            ResetColor
        );
    }

    // time remaining (right-aligned)
    let time_x = (prefix.len() + fuse_w) as u16;
    let time_color = if pct_remaining > 0.5 {
        Color::Green
    } else if pct_remaining > 0.25 {
        Color::Yellow
    } else {
        Color::Red
    };
    let _ = queue!(
        out,
        MoveTo(time_x, y),
        SetForegroundColor(time_color),
        SetAttribute(Attribute::Bold),
        Print(&time_str),
        ResetColor
    );
}

fn draw_combo_row(out: &mut impl Write, width: u16, y: u16, bs: &BeatState, tick: usize) {
    let _ = queue!(out, MoveTo(0, y), Print(" ".repeat(width as usize)));
    if bs.combo < 2 {
        return;
    }
    // pulse the combo label on each tick
    let label = format!("  {} ", bs.combo_label());
    let pts_label = format!("  +{}  ", fmt_score(bs.last_pts));
    let color = if tick % 2 == 0 { bs.combo_color() } else { Color::White };
    let _ = queue!(
        out,
        MoveTo(0, y),
        SetForegroundColor(color),
        SetAttribute(Attribute::Bold),
        Print(&label),
        ResetColor,
        MoveTo(width.saturating_sub(pts_label.len() as u16 + 2), y),
        SetForegroundColor(Color::DarkGrey),
        Print(&pts_label),
        ResetColor
    );
}

// ─── Screens ─────────────────────────────────────────────────────────────────

fn show_title(out: &mut impl Write, width: u16, height: u16, total: usize, deck_name: &str) -> Option<GameMode> {
    let mut mode = GameMode::Normal;
    let cx = width / 2;
    let cy = height / 2;

    let subtitle = format!("{} edition", deck_name);
    let art_static = [
        r"  ___  _   _ ___ __________ ___ ____    _    _     ",
        r" / _ \| | | |_ _|__  /__  /|_ _/ ___|  / \  | |   ",
        r"| | | | | | || |  / /  / /  | | |     / _ \ | |   ",
        r"| |_| | |_| || | / /_ / /_ _| | |___ / ___ \| |___",
        r" \__\_\\___/|___/____/____(_)___\____/_/   \_\_____|",
        r"                                                    ",
    ];
    let art_lines: Vec<&str> = art_static.iter().copied().chain(std::iter::once(subtitle.as_str())).collect();

    loop {
        let _ = queue!(out, Clear(ClearType::All));

        let art_start = cy.saturating_sub((art_lines.len() + 8) as u16 / 2);
        for (i, line) in art_lines.iter().enumerate() {
            let color = match i % 3 {
                0 => Color::Cyan,
                1 => Color::Yellow,
                _ => Color::Green,
            };
            let _ = queue!(
                out,
                MoveTo(cx.saturating_sub(line.len() as u16 / 2), art_start + i as u16),
                SetForegroundColor(color),
                SetAttribute(Attribute::Bold),
                Print(line),
                ResetColor
            );
        }

        let sub = format!("{} questions loaded", total);
        let _ = queue!(
            out,
            MoveTo(cx.saturating_sub(sub.len() as u16 / 2), art_start + art_lines.len() as u16 + 1),
            SetForegroundColor(Color::DarkGrey),
            Print(&sub),
            ResetColor
        );

        // mode toggle
        let toggle_y = art_start + art_lines.len() as u16 + 3;
        let normal_label = "  NORMAL  ";
        let beat_label = "  ⚡ BEAT  ";
        let gap = "    ";

        let total_toggle_w = normal_label.len() + gap.len() + beat_label.len() + 4;
        let tx = cx.saturating_sub(total_toggle_w as u16 / 2);

        // NORMAL box
        if mode == GameMode::Normal {
            let _ = queue!(
                out,
                MoveTo(tx, toggle_y),
                SetBackgroundColor(Color::Cyan),
                SetForegroundColor(Color::Black),
                SetAttribute(Attribute::Bold),
                Print(normal_label),
                ResetColor
            );
        } else {
            let _ = queue!(
                out,
                MoveTo(tx, toggle_y),
                SetForegroundColor(Color::DarkGrey),
                Print(normal_label),
                ResetColor
            );
        }

        let _ = queue!(out, MoveTo(tx + normal_label.len() as u16, toggle_y), Print(gap));

        // BEAT box
        let beat_x = tx + normal_label.len() as u16 + gap.len() as u16;
        if mode == GameMode::Beat {
            let _ = queue!(
                out,
                MoveTo(beat_x, toggle_y),
                SetBackgroundColor(Color::Red),
                SetForegroundColor(Color::White),
                SetAttribute(Attribute::Bold),
                Print(beat_label),
                ResetColor
            );
        } else {
            let _ = queue!(
                out,
                MoveTo(beat_x, toggle_y),
                SetForegroundColor(Color::DarkGrey),
                Print(beat_label),
                ResetColor
            );
        }

        let hint1 = "[ ← → ] or [ B ] to toggle mode";
        let hint2 = "[ Enter ] to start   [ q ] to quit";
        let _ = queue!(
            out,
            MoveTo(cx.saturating_sub(hint1.len() as u16 / 2), toggle_y + 2),
            SetForegroundColor(Color::DarkGrey),
            Print(hint1),
            ResetColor,
            MoveTo(cx.saturating_sub(hint2.len() as u16 / 2), toggle_y + 3),
            SetForegroundColor(Color::DarkGrey),
            Print(hint2),
            ResetColor
        );

        if mode == GameMode::Beat {
            let beat_info = format!("  Timed: {}s/question  │  Points: faster = more  │  Chain for combo  ", BEAT_TIME as u32);
            let _ = queue!(
                out,
                MoveTo(cx.saturating_sub(beat_info.len() as u16 / 2), toggle_y + 5),
                SetForegroundColor(Color::Yellow),
                Print(&beat_info),
                ResetColor
            );
        }

        out.flush().unwrap();

        match event::read().ok() {
            Some(Event::Key(k)) => match k.code {
                KeyCode::Enter => return Some(mode),
                KeyCode::Char('q') | KeyCode::Esc => return None,
                KeyCode::Char('b') | KeyCode::Char('B')
                | KeyCode::Left | KeyCode::Right
                | KeyCode::Tab => {
                    mode = if mode == GameMode::Normal {
                        GameMode::Beat
                    } else {
                        GameMode::Normal
                    };
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// Draw the static question content.
/// `bottom_reserve` = how many rows to leave at the bottom for fuse/prompt.
fn draw_question_content(
    out: &mut impl Write,
    q: &Question,
    width: u16,
    height: u16,
    bottom_reserve: u16,
) {
    let usable_w = (width as usize).saturating_sub(6);
    let mut row = 3u16;
    let max_row = height.saturating_sub(bottom_reserve);

    // Domain tag
    let _ = queue!(
        out,
        MoveTo(2, row),
        SetForegroundColor(Color::Yellow),
        Print(format!("[{}]", q.domain)),
        ResetColor
    );
    row += 2;

    // Question
    for line in &word_wrap(&q.question, usable_w) {
        if row >= max_row {
            break;
        }
        let _ = queue!(
            out,
            MoveTo(3, row),
            SetAttribute(Attribute::Bold),
            Print(line),
            ResetColor
        );
        row += 1;
    }
    row += 1;

    // Divider
    if row < max_row {
        let _ = queue!(
            out,
            MoveTo(2, row),
            SetForegroundColor(Color::DarkGrey),
            Print("─".repeat((width as usize).saturating_sub(4))),
            ResetColor
        );
        row += 2;
    }

    // Answer options
    for letter in &["A", "B", "C", "D"] {
        if let Some(text) = q.answers.get(*letter) {
            let label = format!("  {}.  ", letter);
            let indent = label.len();
            let ans_lines = word_wrap(text, usable_w.saturating_sub(indent + 2));
            if row >= max_row {
                break;
            }
            let _ = queue!(
                out,
                MoveTo(3, row),
                SetForegroundColor(Color::Yellow),
                SetAttribute(Attribute::Bold),
                Print(&label),
                ResetColor
            );
            if let Some(first) = ans_lines.first() {
                let _ = queue!(out, MoveTo(3 + indent as u16, row), Print(first));
            }
            row += 1;
            for extra in ans_lines.iter().skip(1) {
                if row >= max_row {
                    break;
                }
                let _ = queue!(out, MoveTo(3 + indent as u16, row), Print(extra));
                row += 1;
            }
            row += 1;
        }
    }
}

fn show_question_normal(
    out: &mut impl Write,
    q: &Question,
    q_num: usize,
    total: usize,
    score: usize,
    width: u16,
    height: u16,
) {
    let _ = queue!(out, Clear(ClearType::All));
    draw_header_normal(out, width, q_num, total, score);
    draw_progress_bar(out, 0, 1, width, q_num - 1, total);
    draw_question_content(out, q, width, height, 3);
    let _ = queue!(
        out,
        MoveTo(2, height - 2),
        SetForegroundColor(Color::White),
        SetAttribute(Attribute::Bold),
        Print("  Answer [A / B / C / D]  or  [q] to quit: "),
        ResetColor
    );
    out.flush().unwrap();
}

fn show_question_beat(
    out: &mut impl Write,
    q: &Question,
    q_num: usize,
    total: usize,
    bs: &BeatState,
    width: u16,
    height: u16,
) {
    let _ = queue!(out, Clear(ClearType::All));
    draw_header_beat(out, width, q_num, total, bs);
    draw_progress_bar(out, 0, 1, width, q_num - 1, total);
    draw_question_content(out, q, width, height, 5);
    let _ = queue!(
        out,
        MoveTo(2, height - 2),
        SetForegroundColor(Color::White),
        SetAttribute(Attribute::Bold),
        Print("  Answer [A / B / C / D]  or  [q] to quit: "),
        ResetColor
    );
    // initial fuse (full)
    draw_fuse(out, width, height - 4, 1.0, 0);
    out.flush().unwrap();
}

// ─── Beat question loop ───────────────────────────────────────────────────────

enum BeatResult {
    Answer(String, f64), // chosen letter, elapsed seconds
    Timeout,
    Quit,
}

fn run_beat_question(
    out: &mut impl Write,
    q: &Question,
    q_num: usize,
    total: usize,
    bs: &BeatState,
    width: u16,
    height: u16,
) -> BeatResult {
    show_question_beat(out, q, q_num, total, bs, width, height);
    let start = Instant::now();
    let mut tick: usize = 0;

    loop {
        let elapsed = start.elapsed().as_secs_f64();
        let remaining = (BEAT_TIME - elapsed).max(0.0);
        let pct = remaining / BEAT_TIME;

        // update just the dynamic rows (fuse + combo), no full clear
        draw_fuse(out, width, height - 4, pct, tick);
        draw_combo_row(out, width, height - 3, bs, tick);
        out.flush().unwrap();

        if remaining <= 0.0 {
            return BeatResult::Timeout;
        }

        // poll for 50 ms
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(k)) => {
                    let ch = match k.code {
                        KeyCode::Char('a') | KeyCode::Char('A') => "A",
                        KeyCode::Char('b') | KeyCode::Char('B') => "B",
                        KeyCode::Char('c') | KeyCode::Char('C') => "C",
                        KeyCode::Char('d') | KeyCode::Char('D') => "D",
                        KeyCode::Char('q') | KeyCode::Esc => return BeatResult::Quit,
                        _ => {
                            tick += 1;
                            continue;
                        }
                    };
                    return BeatResult::Answer(ch.to_string(), elapsed);
                }
                Ok(Event::Resize(w2, h2)) => {
                    show_question_beat(out, q, q_num, total, bs, w2, h2);
                }
                _ => {}
            }
        }

        tick += 1;
    }
}

// ─── Result screens ───────────────────────────────────────────────────────────

fn burst_animation(out: &mut impl Write, width: u16, height: u16) {
    let cx = (width / 2) as f64;
    let cy = (height / 2) as f64;
    let mut rng = rand::thread_rng();

    let particle_chars = ['★', '✦', '◆', '●', '▲', '✸', '*', '+', '·', '!'];
    let particle_colors = [
        Color::Yellow, Color::Green, Color::Cyan,
        Color::Magenta, Color::Red, Color::White,
    ];
    let trail_colors = [
        Color::DarkYellow, Color::DarkGreen, Color::DarkCyan,
        Color::DarkMagenta, Color::DarkRed, Color::Grey,
    ];

    struct Particle {
        x: f64, y: f64, vx: f64, vy: f64,
        ch: char, color: Color, trail_color: Color,
        history: Vec<(f64, f64)>,
    }

    let gravity = 0.13f64;
    let n: usize = 44;
    let mut particles: Vec<Particle> = (0..n)
        .map(|i| {
            let angle = (i as f64 / n as f64) * 2.0 * PI + rng.gen_range(-0.12..0.12);
            let speed = rng.gen_range(0.9..2.1);
            let ci = i % particle_colors.len();
            Particle {
                x: cx, y: cy,
                vx: angle.cos() * speed * 2.5,
                vy: angle.sin() * speed - rng.gen_range(0.1..0.6),
                ch: particle_chars[i % particle_chars.len()],
                color: particle_colors[ci],
                trail_color: trail_colors[ci],
                history: Vec::new(),
            }
        })
        .collect();

    let banner = CORRECT_BANNERS[rng.gen_range(0..CORRECT_BANNERS.len())];
    let banner_colors = [Color::Green, Color::Yellow, Color::Cyan];

    for frame in 1u16..=28 {
        let _ = queue!(out, Clear(ClearType::All));
        for p in &mut particles {
            p.history.push((p.x, p.y));
            if p.history.len() > 5 { p.history.remove(0); }
            p.x += p.vx;
            p.y += p.vy;
            p.vy += gravity;
            let trail_len = p.history.len();
            for (ti, &(hx, hy)) in p.history.iter().enumerate() {
                if hx < 1.0 || hx >= (width - 1) as f64 || hy < 1.0 || hy >= (height - 1) as f64 { continue; }
                let tch = if ti < trail_len / 2 { '·' } else { p.ch };
                let _ = queue!(out, MoveTo(hx as u16, hy as u16), SetForegroundColor(p.trail_color), Print(tch), ResetColor);
            }
            if p.x >= 1.0 && p.x < (width - 1) as f64 && p.y >= 1.0 && p.y < (height - 1) as f64 {
                let _ = queue!(out, MoveTo(p.x as u16, p.y as u16), SetForegroundColor(p.color), SetAttribute(Attribute::Bold), Print(p.ch), ResetColor);
            }
        }
        let banner_color = banner_colors[(frame as usize) % banner_colors.len()];
        let bx = (cx as u16).saturating_sub(banner.len() as u16 / 2);
        let _ = queue!(out, MoveTo(bx, cy as u16), SetForegroundColor(banner_color), SetAttribute(Attribute::Bold), Print(banner), ResetColor);
        out.flush().unwrap();
        thread::sleep(Duration::from_millis(45));
    }
    thread::sleep(Duration::from_millis(250));
}

fn show_correct(out: &mut impl Write, q: &Question, width: u16, height: u16) {
    burst_animation(out, width, height);
    let _ = queue!(out, Clear(ClearType::All));
    let cx = width / 2;
    let usable_w = (width as usize).saturating_sub(8);
    let mut row = 2u16;

    let hdr = "✓  CORRECT!";
    let _ = queue!(out, MoveTo(cx.saturating_sub(hdr.len() as u16 / 2), row), SetForegroundColor(Color::Green), SetAttribute(Attribute::Bold), Print(hdr), ResetColor);
    row += 3;

    let _ = queue!(out, MoveTo(2, row), SetForegroundColor(Color::DarkGrey), Print("─".repeat((width as usize).saturating_sub(4))), ResetColor);
    row += 2;

    let _ = queue!(out, MoveTo(3, row), SetForegroundColor(Color::Cyan), SetAttribute(Attribute::Bold), Print("Explanation:"), ResetColor);
    row += 2;

    for line in word_wrap(&q.explanation, usable_w) {
        if row >= height.saturating_sub(4) { break; }
        let _ = queue!(out, MoveTo(3, row), Print(line));
        row += 1;
    }

    let cont = "[ press any key for next question ]";
    let _ = queue!(out, MoveTo(cx.saturating_sub(cont.len() as u16 / 2), height - 2), SetForegroundColor(Color::DarkGrey), Print(cont), ResetColor);
    out.flush().unwrap();
    wait_for_key();
}

fn show_correct_beat(out: &mut impl Write, q: &Question, pts: u64, bs: &BeatState, width: u16, height: u16) {
    burst_animation(out, width, height);
    let _ = queue!(out, Clear(ClearType::All));
    let cx = width / 2;
    let usable_w = (width as usize).saturating_sub(8);
    let mut row = 2u16;

    let hdr = "✓  CORRECT!";
    let _ = queue!(out, MoveTo(cx.saturating_sub(hdr.len() as u16 / 2), row), SetForegroundColor(Color::Green), SetAttribute(Attribute::Bold), Print(hdr), ResetColor);
    row += 2;

    // Points + combo callout
    let pts_str = format!("+{}  pts", fmt_score(pts));
    let _ = queue!(out, MoveTo(cx.saturating_sub(pts_str.len() as u16 / 2), row), SetForegroundColor(Color::Yellow), SetAttribute(Attribute::Bold), Print(&pts_str), ResetColor);
    row += 1;

    if bs.combo >= 2 {
        let combo_str = format!("Combo: {}", bs.combo_label());
        let _ = queue!(out, MoveTo(cx.saturating_sub(combo_str.len() as u16 / 2), row), SetForegroundColor(bs.combo_color()), SetAttribute(Attribute::Bold), Print(&combo_str), ResetColor);
    }
    row += 3;

    let _ = queue!(out, MoveTo(2, row), SetForegroundColor(Color::DarkGrey), Print("─".repeat((width as usize).saturating_sub(4))), ResetColor);
    row += 2;

    let _ = queue!(out, MoveTo(3, row), SetForegroundColor(Color::Cyan), SetAttribute(Attribute::Bold), Print("Answer:"), ResetColor);
    row += 1;

    if let Some(ans) = q.answers.get(&q.correct) {
        for line in word_wrap(ans, usable_w) {
            if row >= height.saturating_sub(4) { break; }
            let _ = queue!(out, MoveTo(3, row), Print(line));
            row += 1;
        }
    }

    let total_str = format!("Total: {} pts", fmt_score(bs.score));
    let _ = queue!(out, MoveTo(cx.saturating_sub(total_str.len() as u16 / 2), height - 3), SetForegroundColor(Color::DarkGrey), Print(&total_str), ResetColor);

    let cont = "[ press any key ]";
    let _ = queue!(out, MoveTo(cx.saturating_sub(cont.len() as u16 / 2), height - 2), SetForegroundColor(Color::DarkGrey), Print(cont), ResetColor);
    out.flush().unwrap();
    wait_for_key();
}

fn show_wrong(out: &mut impl Write, q: &Question, chosen: &str, width: u16, height: u16) {
    let _ = queue!(out, Clear(ClearType::All));
    let mut rng = rand::thread_rng();
    let taunt = WRONG_MSGS[rng.gen_range(0..WRONG_MSGS.len())];
    let cx = width / 2;
    let usable_w = (width as usize).saturating_sub(8);
    let mut row = 2u16;

    let hdr = format!("✗  WRONG  ─  You chose {}  ─  Correct: {}", chosen, q.correct);
    let _ = queue!(out, MoveTo(cx.saturating_sub(hdr.len() as u16 / 2), row), SetForegroundColor(Color::Red), SetAttribute(Attribute::Bold), Print(&hdr), ResetColor);
    row += 2;

    let _ = queue!(out, MoveTo(cx.saturating_sub(taunt.len() as u16 / 2), row), SetForegroundColor(Color::Red), Print(taunt), ResetColor);
    row += 3;

    let _ = queue!(out, MoveTo(2, row), SetForegroundColor(Color::DarkGrey), Print("─".repeat((width as usize).saturating_sub(4))), ResetColor);
    row += 2;

    if let Some(ans_text) = q.answers.get(&q.correct) {
        let label = format!("  Correct ({})  ", q.correct);
        let _ = queue!(out, MoveTo(3, row), SetForegroundColor(Color::Green), SetAttribute(Attribute::Bold), Print(&label), ResetColor);
        let ans_lines = word_wrap(ans_text, usable_w.saturating_sub(label.len()));
        if let Some(first) = ans_lines.first() {
            let _ = queue!(out, MoveTo(3 + label.len() as u16, row), Print(first));
        }
        row += 1;
        for extra in ans_lines.iter().skip(1) {
            let _ = queue!(out, MoveTo(3 + label.len() as u16, row), Print(extra));
            row += 1;
        }
    }
    row += 2;

    let _ = queue!(out, MoveTo(3, row), SetForegroundColor(Color::Cyan), SetAttribute(Attribute::Bold), Print("Explanation:"), ResetColor);
    row += 2;

    for line in word_wrap(&q.explanation, usable_w) {
        if row >= height.saturating_sub(4) { break; }
        let _ = queue!(out, MoveTo(3, row), Print(line));
        row += 1;
    }

    let cont = "[ press any key for next question ]";
    let _ = queue!(out, MoveTo(cx.saturating_sub(cont.len() as u16 / 2), height - 2), SetForegroundColor(Color::DarkGrey), Print(cont), ResetColor);
    out.flush().unwrap();
    wait_for_key();
}

fn show_wrong_beat(out: &mut impl Write, q: &Question, chosen: &str, prev_combo: usize, width: u16, height: u16) {
    let _ = queue!(out, Clear(ClearType::All));
    let mut rng = rand::thread_rng();
    let taunt = WRONG_MSGS[rng.gen_range(0..WRONG_MSGS.len())];
    let cx = width / 2;
    let usable_w = (width as usize).saturating_sub(8);
    let mut row = 2u16;

    // COMBO BROKEN banner if they had a streak
    if prev_combo >= 2 {
        let broken = format!("COMBO BROKEN  (was ×{})", prev_combo);
        let _ = queue!(out, MoveTo(cx.saturating_sub(broken.len() as u16 / 2), row), SetForegroundColor(Color::Red), SetAttribute(Attribute::Bold), Print(&broken), ResetColor);
        row += 2;
    }

    let hdr = format!("✗  WRONG  ─  You chose {}  ─  Correct: {}", chosen, q.correct);
    let _ = queue!(out, MoveTo(cx.saturating_sub(hdr.len() as u16 / 2), row), SetForegroundColor(Color::Red), SetAttribute(Attribute::Bold), Print(&hdr), ResetColor);
    row += 2;

    let _ = queue!(out, MoveTo(cx.saturating_sub(taunt.len() as u16 / 2), row), SetForegroundColor(Color::Red), Print(taunt), ResetColor);
    row += 3;

    let _ = queue!(out, MoveTo(2, row), SetForegroundColor(Color::DarkGrey), Print("─".repeat((width as usize).saturating_sub(4))), ResetColor);
    row += 2;

    if let Some(ans_text) = q.answers.get(&q.correct) {
        let label = format!("  Correct ({})  ", q.correct);
        let _ = queue!(out, MoveTo(3, row), SetForegroundColor(Color::Green), SetAttribute(Attribute::Bold), Print(&label), ResetColor);
        let ans_lines = word_wrap(ans_text, usable_w.saturating_sub(label.len()));
        if let Some(first) = ans_lines.first() {
            let _ = queue!(out, MoveTo(3 + label.len() as u16, row), Print(first));
        }
        row += 1;
        for extra in ans_lines.iter().skip(1) {
            let _ = queue!(out, MoveTo(3 + label.len() as u16, row), Print(extra));
            row += 1;
        }
    }

    let cont = "[ press any key ]";
    let _ = queue!(out, MoveTo(cx.saturating_sub(cont.len() as u16 / 2), height - 2), SetForegroundColor(Color::DarkGrey), Print(cont), ResetColor);
    out.flush().unwrap();
    wait_for_key();
}

fn show_timeout(out: &mut impl Write, q: &Question, width: u16, height: u16) {
    // flash grey a couple times
    for _ in 0..3 {
        let _ = queue!(out, Clear(ClearType::All));
        out.flush().unwrap();
        thread::sleep(Duration::from_millis(60));
        let _ = queue!(out, MoveTo(width / 2 - 6, height / 2), SetForegroundColor(Color::DarkGrey), SetAttribute(Attribute::Bold), Print("TIME'S UP!"), ResetColor);
        out.flush().unwrap();
        thread::sleep(Duration::from_millis(80));
    }

    let _ = queue!(out, Clear(ClearType::All));
    let cx = width / 2;
    let usable_w = (width as usize).saturating_sub(8);
    let mut row = 2u16;

    let hdr = "  TIME'S UP!  ";
    let _ = queue!(out, MoveTo(cx.saturating_sub(hdr.len() as u16 / 2), row), SetForegroundColor(Color::DarkGrey), SetAttribute(Attribute::Bold), Print(hdr), ResetColor);
    row += 3;

    let _ = queue!(out, MoveTo(2, row), SetForegroundColor(Color::DarkGrey), Print("─".repeat((width as usize).saturating_sub(4))), ResetColor);
    row += 2;

    let correct_label = format!("  Correct answer was {}:  ", q.correct);
    let _ = queue!(out, MoveTo(3, row), SetForegroundColor(Color::Green), SetAttribute(Attribute::Bold), Print(&correct_label), ResetColor);
    row += 1;

    if let Some(ans) = q.answers.get(&q.correct) {
        for line in word_wrap(ans, usable_w) {
            if row >= height.saturating_sub(4) { break; }
            let _ = queue!(out, MoveTo(3, row), Print(line));
            row += 1;
        }
    }

    let cont = "[ press any key ]";
    let _ = queue!(out, MoveTo(cx.saturating_sub(cont.len() as u16 / 2), height - 2), SetForegroundColor(Color::DarkGrey), Print(cont), ResetColor);
    out.flush().unwrap();
    wait_for_key();
}

fn show_final(out: &mut impl Write, score: usize, total: usize, width: u16, height: u16) {
    let _ = queue!(out, Clear(ClearType::All));
    let pct = if total > 0 { score * 100 / total } else { 0 };
    let cx = width / 2;
    let cy = height / 2;

    let (grade, sub) = if pct >= 90 {
        ("★  OUTSTANDING", "You might actually be ready for the real thing.")
    } else if pct >= 75 {
        ("✓  SOLID PASS", "Keep reviewing the ones you missed.")
    } else if pct >= 60 {
        ("~  GETTING THERE", "Focus on the wrong answers. Run it again.")
    } else {
        ("✗  KEEP STUDYING", "Go back through the study guides. You've got this.")
    };

    let color = color_for_pct(pct);
    let score_line = format!("Final Score:  {} / {}  ({}%)", score, total, pct);

    let lines: Vec<(&str, Color, bool)> = vec![
        ("╔══════════════════════════════════╗", color, true),
        ("║         QUIZ  COMPLETE!          ║", color, true),
        ("╚══════════════════════════════════╝", color, true),
        ("", Color::Reset, false),
        (&score_line, color, true),
        ("", Color::Reset, false),
        (grade, color, true),
        (sub, Color::DarkGrey, false),
        ("", Color::Reset, false),
        ("[ press any key to exit ]", Color::DarkGrey, false),
    ];

    let start = cy.saturating_sub(lines.len() as u16 / 2);
    for (i, (line, clr, bold)) in lines.iter().enumerate() {
        let x = cx.saturating_sub(line.len() as u16 / 2);
        let y = start + i as u16;
        if *bold {
            let _ = queue!(out, MoveTo(x, y), SetForegroundColor(*clr), SetAttribute(Attribute::Bold), Print(line), ResetColor);
        } else {
            let _ = queue!(out, MoveTo(x, y), SetForegroundColor(*clr), Print(line), ResetColor);
        }
    }
    out.flush().unwrap();
    wait_for_key();
}

fn show_final_beat(out: &mut impl Write, bs: &BeatState, answered: usize, total: usize, width: u16, height: u16) {
    let _ = queue!(out, Clear(ClearType::All));
    let cx = width / 2;
    let cy = height / 2;

    let pct = if answered > 0 { bs.correct * 100 / answered } else { 0 };
    let color = color_for_pct(pct);

    let (grade, sub) = if bs.score == 0 {
        ("✗  ROUGH SESSION", "Sometimes the fuse just burns too fast.")
    } else if bs.max_combo >= 10 {
        ("★  LEGENDARY", "That combo chain. Unreal.")
    } else if bs.max_combo >= 6 {
        ("◆  ELITE", "Speed AND accuracy. That's the combo.")
    } else if pct >= 80 {
        ("✓  SOLID SPEED RUN", "Clean answers, decent pace.")
    } else {
        ("~  KEEP GRINDING", "Speed will come with reps.")
    };

    let lines = vec![
        format!("╔══════════════════════════════════════╗"),
        format!("║           BEAT MODE OVER!            ║"),
        format!("╚══════════════════════════════════════╝"),
        String::new(),
        format!("  Score          {:>10} pts", fmt_score(bs.score)),
        format!("  Questions      {:>4} / {}", answered, total),
        format!("  Correct        {:>4}   Wrong: {}   Timeouts: {}", bs.correct, bs.wrong, bs.timeouts),
        format!("  Avg. Time      {:>7.2}s", bs.avg_time()),
        format!("  Max Combo      ×{}", bs.max_combo),
        String::new(),
        grade.to_string(),
        sub.to_string(),
        String::new(),
        "[ press any key to exit ]".to_string(),
    ];

    let start = cy.saturating_sub(lines.len() as u16 / 2);
    for (i, line) in lines.iter().enumerate() {
        let x = cx.saturating_sub(line.len() as u16 / 2);
        let y = start + i as u16;
        let line_color = match i {
            0..=2 => color,
            4..=8 => Color::White,
            10 => color,
            11 => Color::DarkGrey,
            _ => Color::DarkGrey,
        };
        let bold = matches!(i, 0..=2 | 10);
        if bold {
            let _ = queue!(out, MoveTo(x, y), SetForegroundColor(line_color), SetAttribute(Attribute::Bold), Print(line), ResetColor);
        } else {
            let _ = queue!(out, MoveTo(x, y), SetForegroundColor(line_color), Print(line), ResetColor);
        }
    }
    out.flush().unwrap();
    wait_for_key();
}

// ─── File picker ─────────────────────────────────────────────────────────────

fn find_json_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(".")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();
    files
}

fn pick_file(out: &mut impl Write, width: u16, height: u16) -> Option<PathBuf> {
    let files = find_json_files();
    if files.is_empty() {
        return None;
    }
    if files.len() == 1 {
        return Some(files.into_iter().next().unwrap());
    }

    let mut selected: usize = 0;
    loop {
        let _ = queue!(out, Clear(ClearType::All));
        let cx = width / 2;
        let cy = height / 2;

        let title = "── SELECT A QUIZ DECK ──";
        let _ = queue!(
            out,
            MoveTo(cx.saturating_sub(title.len() as u16 / 2), cy.saturating_sub(files.len() as u16 / 2 + 3)),
            SetForegroundColor(Color::Cyan), SetAttribute(Attribute::Bold), Print(title), ResetColor
        );
        let hint = "↑ ↓ to navigate   Enter to load   q to quit";
        let _ = queue!(
            out,
            MoveTo(cx.saturating_sub(hint.len() as u16 / 2), height - 2),
            SetForegroundColor(Color::DarkGrey), Print(hint), ResetColor
        );

        let list_start = cy.saturating_sub(files.len() as u16 / 2 + 1);
        for (i, path) in files.iter().enumerate() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("???");
            let y = list_start + i as u16;
            if i == selected {
                let row = format!("  ▶  {}  ", name);
                let x = cx.saturating_sub(row.len() as u16 / 2);
                let _ = queue!(out, MoveTo(x, y), SetForegroundColor(Color::Black), SetAttribute(Attribute::Bold), SetBackgroundColor(Color::Yellow), Print(&row), ResetColor);
            } else {
                let row = format!("     {}  ", name);
                let x = cx.saturating_sub(row.len() as u16 / 2);
                let _ = queue!(out, MoveTo(x, y), SetForegroundColor(Color::White), Print(&row), ResetColor);
            }
        }
        out.flush().unwrap();

        match event::read().ok()? {
            Event::Key(k) => match k.code {
                KeyCode::Up | KeyCode::Char('k') => { if selected > 0 { selected -= 1; } }
                KeyCode::Down | KeyCode::Char('j') => { if selected < files.len() - 1 { selected += 1; } }
                KeyCode::Enter => return Some(files[selected].clone()),
                KeyCode::Char('q') | KeyCode::Esc => return None,
                _ => {}
            },
            _ => {}
        }
    }
}

// ─── Entry ────────────────────────────────────────────────────────────────────

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let cli_path = args.windows(2)
        .find(|w| w[0] == "--file")
        .map(|w| PathBuf::from(&w[1]));

    let mut out = stdout();
    terminal::enable_raw_mode()?;
    execute!(out, EnterAlternateScreen, Hide)?;

    let json_path = if let Some(p) = cli_path {
        p
    } else {
        let (w, h) = terminal::size()?;
        match pick_file(&mut out, w, h) {
            Some(p) => p,
            None => {
                execute!(out, LeaveAlternateScreen, Show)?;
                terminal::disable_raw_mode()?;
                println!("No deck selected.");
                return Ok(());
            }
        }
    };

    let raw = std::fs::read_to_string(&json_path)
        .map_err(|_| format!("Cannot read {:?}", json_path))?;
    let bank: QBank = serde_json::from_str(&raw)?;
    let deck_name = if bank.name.is_empty() { "quiz".to_string() } else { bank.name.clone() };
    let mut questions = bank.questions;
    questions.shuffle(&mut rand::thread_rng());
    let total = questions.len();

    let (w, h) = terminal::size()?;
    let mode = match show_title(&mut out, w, h, total, &deck_name) {
        Some(m) => m,
        None => return Ok(()),
    };

    // ── Normal mode ──────────────────────────────────────────────────────────
    if mode == GameMode::Normal {
        let mut score = 0usize;
        for (idx, q) in questions.iter().enumerate() {
            let (w, h) = terminal::size()?;
            show_question_normal(&mut out, q, idx + 1, total, score, w, h);

            let chosen = loop {
                match event::read()? {
                    Event::Key(k) => {
                        let ch = match k.code {
                            KeyCode::Char('a') | KeyCode::Char('A') => "A",
                            KeyCode::Char('b') | KeyCode::Char('B') => "B",
                            KeyCode::Char('c') | KeyCode::Char('C') => "C",
                            KeyCode::Char('d') | KeyCode::Char('D') => "D",
                            KeyCode::Char('q') | KeyCode::Esc => {
                                execute!(out, LeaveAlternateScreen, Show)?;
                                terminal::disable_raw_mode()?;
                                println!("\nBailed early. Score: {}/{}", score, idx);
                                return Ok(());
                            }
                            _ => continue,
                        };
                        break ch;
                    }
                    Event::Resize(w2, h2) => {
                        show_question_normal(&mut out, q, idx + 1, total, score, w2, h2);
                    }
                    _ => continue,
                }
            };

            let (w, h) = terminal::size()?;
            if chosen == q.correct {
                score += 1;
                show_correct(&mut out, q, w, h);
            } else {
                show_wrong(&mut out, q, chosen, w, h);
            }
        }
        let (w, h) = terminal::size()?;
        show_final(&mut out, score, total, w, h);

    // ── Beat mode ────────────────────────────────────────────────────────────
    } else {
        let mut bs = BeatState::new();
        let mut answered = 0usize;

        'outer: for (idx, q) in questions.iter().enumerate() {
            let (w, h) = terminal::size()?;
            let result = run_beat_question(&mut out, q, idx + 1, total, &bs, w, h);

            match result {
                BeatResult::Quit => {
                    // show stats even on early quit
                    let (w, h) = terminal::size()?;
                    show_final_beat(&mut out, &bs, answered, total, w, h);
                    break 'outer;
                }
                BeatResult::Timeout => {
                    bs.timeouts += 1;
                    bs.combo = 0;
                    answered += 1;
                    let (w, h) = terminal::size()?;
                    show_timeout(&mut out, q, w, h);
                }
                BeatResult::Answer(chosen, elapsed) => {
                    answered += 1;
                    if chosen == q.correct {
                        let pts = BeatState::calc_points(elapsed, BEAT_TIME, bs.combo);
                        bs.score += pts;
                        bs.combo += 1;
                        bs.max_combo = bs.max_combo.max(bs.combo);
                        bs.correct += 1;
                        bs.last_pts = pts;
                        bs.response_times.push(elapsed);
                        let (w, h) = terminal::size()?;
                        show_correct_beat(&mut out, q, pts, &bs, w, h);
                    } else {
                        let prev_combo = bs.combo;
                        bs.combo = 0;
                        bs.wrong += 1;
                        bs.last_pts = 0;
                        let (w, h) = terminal::size()?;
                        show_wrong_beat(&mut out, q, &chosen, prev_combo, w, h);
                    }
                }
            }
        }

        // completed all questions
        if answered == total {
            let (w, h) = terminal::size()?;
            show_final_beat(&mut out, &bs, answered, total, w, h);
        }
    }

    execute!(out, LeaveAlternateScreen, Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        let _ = terminal::disable_raw_mode();
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
