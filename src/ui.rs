use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};

use crate::input::{self, Action};
use crate::mqtt::MqttStatus;
use crate::state::{AppState, InputStage, Scale, WeightInput, UNITS};

/// Static config shown in the footer.
pub struct DisplayConfig {
    pub broker: String,
    pub port: u16,
    pub topic_prefix: String,
    pub interval_ms: u64,
}

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: Arc<Mutex<AppState>>,
    mqtt_status: Arc<Mutex<MqttStatus>>,
    cfg: DisplayConfig,
) -> anyhow::Result<()> {
    loop {
        {
            let state = app.lock().unwrap();
            let status = mqtt_status.lock().unwrap();
            terminal.draw(|f| draw(f, &state, &status, &cfg))?;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut state = app.lock().unwrap();
                if let Action::Quit = input::handle_key(&mut state, key) {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn weight_color(w: f64) -> Color {
    if w < 0.0 {
        Color::Yellow
    } else if w > 0.0 {
        Color::Green
    } else {
        Color::White
    }
}

fn status_color(status: &MqttStatus) -> Color {
    match status {
        MqttStatus::Connected => Color::Green,
        MqttStatus::Connecting => Color::Yellow,
        MqttStatus::Disconnected | MqttStatus::Error(_) => Color::Red,
    }
}

fn last_tx(scale: &Scale) -> String {
    match scale.last_publish {
        Some(t) => format!("{:.1}s ago", t.elapsed().as_secs_f64()),
        None => "never".to_string(),
    }
}

fn draw(f: &mut Frame, app: &AppState, status: &MqttStatus, cfg: &DisplayConfig) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(6),
        ])
        .split(f.area());

    // Title.
    let title = Paragraph::new(Line::from(Span::styled(
        "eWS Scale Simulator",
        Style::default().add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    draw_scales(f, chunks[1], app);
    draw_footer(f, chunks[2], status, cfg);

    if let Some(input) = &app.input {
        draw_popup(f, input);
    }

    if let Some(weight_input) = &app.weight_input {
        draw_weight_popup(f, app, weight_input);
    }
}

fn draw_scales(f: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Scales ({}) ", app.scales.len()));

    if app.scales.is_empty() {
        let empty = Paragraph::new("\n  No scales yet. Press 'a' to add one.")
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .scales
        .iter()
        .map(|s| {
            ListItem::new(Line::from(vec![
                Span::raw(format!("{:<14}", s.serial_number)),
                Span::styled(
                    format!("{:>10.3} {:<3}", s.weight_value, s.unit),
                    Style::default()
                        .fg(weight_color(s.weight_value))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  TX: {}", last_tx(s))),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    let list = List::new(items)
        .block(block)
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_stateful_widget(list, area, &mut list_state);
}

fn draw_footer(f: &mut Frame, area: Rect, status: &MqttStatus, cfg: &DisplayConfig) {
    let lines = vec![
        Line::from(vec![
            Span::raw("  MQTT : "),
            Span::styled("●", Style::default().fg(status_color(status))),
            Span::raw(format!(" {status}    Broker: {}:{}", cfg.broker, cfg.port)),
        ]),
        Line::from(format!(
            "  Topic: {}/<serial>    Interval: {}ms",
            cfg.topic_prefix, cfg.interval_ms
        )),
        Line::from(""),
        Line::from("  ↑/↓ select   ←/→ ±0.001   PgUp/PgDn ±0.100"),
        Line::from("  w set weight   a add   z zero   q quit"),
    ];
    let footer = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, area);
}

fn draw_popup(f: &mut Frame, input: &crate::state::AddScaleInput) {
    let area = centered_rect(46, 8, f.area());
    f.render_widget(Clear, area);

    let serial_active = input.stage == InputStage::Serial;
    let active = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    let serial_text = if serial_active {
        format!("{}_", input.serial)
    } else {
        input.serial.clone()
    };

    let unit_text = format!("< {} >", input.unit());
    let units_hint = format!("  (of {})", UNITS.join(", "));

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Serial : ", if serial_active { active } else { dim }),
            Span::raw(serial_text),
        ]),
        Line::from(vec![
            Span::styled("  Unit   : ", if !serial_active { active } else { dim }),
            Span::raw(unit_text),
            Span::styled(units_hint, dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            if serial_active {
                "  type serial · Enter next · Esc cancel"
            } else {
                "  ←/→ unit · Enter add · Esc cancel"
            },
            dim,
        )),
    ];

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Add Scale "),
    );
    f.render_widget(popup, area);
}

fn draw_weight_popup(f: &mut Frame, app: &AppState, input: &WeightInput) {
    let area = centered_rect(46, 8, f.area());
    f.render_widget(Clear, area);

    let active = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    let serial = app
        .scales
        .get(app.selected)
        .map(|s| s.serial_number.clone())
        .unwrap_or_default();

    let valid = input.parsed().is_some();
    let value_style = if input.buffer.is_empty() || valid {
        active
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  Scale  : {serial}"), dim)),
        Line::from(vec![
            Span::styled("  Weight : ", active),
            Span::styled(format!("{}_", input.buffer), value_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  type value · Enter apply · Esc cancel",
            dim,
        )),
    ];

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Set Weight "),
    );
    f.render_widget(popup, area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
