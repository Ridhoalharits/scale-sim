# ews-scale-sim — Claude Code Build Prompt

Build a Rust terminal app called `ews-scale-sim` — an interactive scale simulator that publishes weight data over MQTT.

---

## Tech Stack

```toml
[dependencies]
ratatui = "0.28"
crossterm = "0.28"
rumqttc = "0.24"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
```

Do **not** use Tokio. Use `std::thread` + `Arc<Mutex<>>` for shared state.

---

## Project Structure

```
src/
├── main.rs      # CLI args, terminal setup/teardown, thread spawning
├── state.rs     # ScaleState struct
├── mqtt.rs      # MQTT client, event loop thread, publish loop
└── ui.rs        # Ratatui rendering + keyboard input loop
```

---

## State (`src/state.rs`)

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleState {
    pub serial_number: String,
    pub weight_value: f64,  // 3 decimal precision
    pub unit: String,       // always "kg"
    pub tare_value: f64,    // always 0.0 for now
}
```

Methods needed:

- `new(serial_number: String) -> Self` — defaults: weight=0.0, unit="kg", tare=0.0
- `adjust_weight(&mut self, delta: f64)` — round result to 3 decimal places after adjustment
- `zero(&mut self)` — sets weight_value to 0.0

---

## MQTT (`src/mqtt.rs`)

**MqttStatus enum:** `Connecting | Connected | Disconnected | Error(String)` — implement `Display`.

**`run()` function** signature:

```rust
pub fn run(
    broker: String,
    port: u16,
    topic: String,
    state: Arc<Mutex<ScaleState>>,
    status: Arc<Mutex<MqttStatus>>,
    last_publish: Arc<Mutex<Option<std::time::Instant>>>,
    interval_ms: u64,
)
```

Implementation:

- Create `rumqttc::Client` with client ID `"ews-scale-sim"`, keep-alive 5s
- Spawn a thread that drives `connection.iter()` (the rumqttc event loop)
- Inside that thread, on `ConnAck` → set status to `Connected`; on `Err` → set status to `Error(...)`
- Spawn a second thread (publish loop) that:
  - Sleeps `interval_ms` milliseconds
  - Locks state, serializes to JSON, calls `client.publish(topic, QoS::AtLeastOnce, false, payload)`
  - Updates `last_publish` with `Some(Instant::now())` after each successful publish

Published JSON format (exact):

```json
{
  "serialNumber": "SIM-001",
  "weightValue": -0.043,
  "unit": "kg",
  "tareValue": 0.0
}
```

---

## TUI (`src/ui.rs`)

**`run()` function** signature:

```rust
pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: Arc<Mutex<ScaleState>>,
    mqtt_status: Arc<Mutex<MqttStatus>>,
    last_publish: Arc<Mutex<Option<std::time::Instant>>>,
    interval_ms: u64,
) -> anyhow::Result<()>
```

**Layout — 3 vertical blocks:**

```
┌─────────────────────────────────┐
│       eWS Scale Simulator       │  ← title block
├─────────────────────────────────┤
│  Serial  : SIM-001              │
│  Weight  :      -0.043 kg       │  ← weight value bold, right-aligned
│  Tare    :       0.000 kg       │
│                                 │
│  MQTT    : ● Connected          │  ← colored dot
│  Topic   : scale/data           │
│  Interval: 500ms                │
│  Last TX : 0.3s ago             │  ← "never" if no publish yet
├─────────────────────────────────┤
│  ↑/↓  ±0.001 kg                 │
│  PgUp/PgDn  ±0.100 kg           │
│  Z  zero    Q  quit             │  ← controls hint block
└─────────────────────────────────┘
```

**Color rules:**

| Element | Color |
|---|---|
| Weight value — negative | Yellow |
| Weight value — positive | Green |
| Weight value — zero | White |
| MQTT dot — Connected | Green |
| MQTT dot — Connecting | Yellow |
| MQTT dot — Disconnected / Error | Red |

**Event loop:**

- Poll crossterm events with `event::poll(Duration::from_millis(100))`
- Redraw on every loop tick, not only on input
- Handle `KeyEventKind::Press` only:

| Key | Action |
|---|---|
| `↑` | `adjust_weight(+0.001)` |
| `↓` | `adjust_weight(-0.001)` |
| `PageUp` | `adjust_weight(+0.100)` |
| `PageDown` | `adjust_weight(-0.100)` |
| `z` / `Z` | `zero()` |
| `q` / `Q` / `Esc` | break loop, exit |

---

## CLI (`src/main.rs`)

| Flag | Default | Description |
|---|---|---|
| `--broker` | `localhost` | MQTT broker host |
| `--port` | `1883` | MQTT broker port |
| `--topic` | `scale/data` | MQTT publish topic |
| `--serial` | `SIM-001` | Scale serial number |
| `--interval-ms` | `500` | Publish interval in milliseconds |

**Terminal setup/teardown:**

- On start: `enable_raw_mode()` + `EnterAlternateScreen`
- On exit (normal or panic): always run `disable_raw_mode()` + `LeaveAlternateScreen` + `show_cursor()`
- Register a panic hook before entering the TUI to guarantee terminal restore on crash

**Thread spawning sequence:**

1. Parse CLI args
2. Create `Arc<Mutex<ScaleState>>`, `Arc<Mutex<MqttStatus>>`, `Arc<Mutex<Option<Instant>>>`
3. Call `mqtt::run(...)` — it spawns its own threads internally
4. Setup terminal
5. Call `ui::run(...)` — blocks until user quits
6. Teardown terminal

---

## Constraints

- No Tokio, no async/await anywhere
- Keep all modules under ~100 lines each
- No config file — CLI args only
- No reconnect logic for v1 — if MQTT disconnects, show the error in status, keep TUI running
