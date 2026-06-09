mod input;
mod mqtt;
mod state;
mod ui;

use std::io::stdout;
use std::sync::{Arc, Mutex};

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use mqtt::MqttStatus;
use state::AppState;
use ui::DisplayConfig;

#[derive(Parser)]
#[command(name = "ews-scale-sim", about = "Interactive multi-scale simulator that publishes weight over MQTT")]
struct Args {
    /// MQTT broker host
    #[arg(long, env = "BROKER", default_value = "localhost")]
    broker: String,

    /// MQTT broker port (ignored for ws/wss; port comes from the URL)
    #[arg(long, env = "PORT", default_value_t = 1883)]
    port: u16,

    /// MQTT username (optional)
    #[arg(long, env = "USERNAME")]
    username: Option<String>,

    /// MQTT password (optional)
    #[arg(long, env = "PASSWORD")]
    password: Option<String>,

    /// MQTT topic prefix; each scale publishes to `<prefix>/<serial>`
    #[arg(long = "topic-prefix", env = "TOPIC_PREFIX", default_value = "scale")]
    topic_prefix: String,

    /// Publish interval in milliseconds
    #[arg(long = "interval-ms", env = "INTERVAL_MS", default_value_t = 100)]
    interval_ms: u64,
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
}

fn main() -> anyhow::Result<()> {
    // Load .env (if present) before parsing; CLI flags still override.
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let app = Arc::new(Mutex::new(AppState::default()));
    let status = Arc::new(Mutex::new(MqttStatus::Connecting));

    // Only set credentials if both username and password are provided.
    let credentials = match (args.username.clone(), args.password.clone()) {
        (Some(u), Some(p)) => Some((u, p)),
        _ => None,
    };

    // MQTT spawns its own threads internally.
    mqtt::run(
        args.broker.clone(),
        args.port,
        args.topic_prefix.clone(),
        credentials,
        Arc::clone(&app),
        Arc::clone(&status),
        args.interval_ms,
    );

    // Guarantee terminal restore on panic.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    // Terminal setup.
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = ui::run(
        &mut terminal,
        Arc::clone(&app),
        Arc::clone(&status),
        DisplayConfig {
            broker: args.broker,
            port: args.port,
            topic_prefix: args.topic_prefix,
            interval_ms: args.interval_ms,
        },
    );

    // Terminal teardown.
    restore_terminal();

    result
}
