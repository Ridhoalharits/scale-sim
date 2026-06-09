use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rumqttc::{Client, Event, MqttOptions, Packet, QoS, Transport};

use crate::state::AppState;

#[derive(Clone)]
pub enum MqttStatus {
    Connecting,
    Connected,
    // Part of the spec'd status API and color rules; v1 has no reconnect
    // logic, so nothing constructs it yet.
    #[allow(dead_code)]
    Disconnected,
    Error(String),
}

impl fmt::Display for MqttStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MqttStatus::Connecting => write!(f, "Connecting"),
            MqttStatus::Connected => write!(f, "Connected"),
            MqttStatus::Disconnected => write!(f, "Disconnected"),
            MqttStatus::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

/// Extracts the serial number from a command topic of the form
/// `{prefix}/{serial}/command`. Returns None for anything else.
fn command_serial<'a>(topic: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = topic.strip_prefix(prefix)?.strip_prefix('/')?;
    let serial = rest.strip_suffix("/command")?;
    if serial.is_empty() || serial.contains('/') {
        return None;
    }
    Some(serial)
}

pub fn run(
    broker: String,
    port: u16,
    topic_prefix: String,
    credentials: Option<(String, String)>,
    app: Arc<Mutex<AppState>>,
    status: Arc<Mutex<MqttStatus>>,
    interval_ms: u64,
) {
    // For ws://, wss:// the broker string is the full URL; rumqttc needs the
    // matching transport. Anything else is treated as a plain TCP host.
    let mut opts = MqttOptions::new("ews-scale-sim", &broker, port);
    opts.set_keep_alive(Duration::from_secs(5));
    if broker.starts_with("wss://") {
        opts.set_transport(Transport::wss_with_default_config());
    } else if broker.starts_with("ws://") {
        opts.set_transport(Transport::Ws);
    }
    if let Some((username, password)) = credentials {
        opts.set_credentials(username, password);
    }

    let (client, mut connection) = Client::new(opts, 10);

    // Event loop thread: drives the rumqttc connection, tracks status, and
    // applies remote commands received on `{prefix}/{serial}/command`.
    let event_status = Arc::clone(&status);
    let event_app = Arc::clone(&app);
    let sub_client = client.clone();
    let command_filter = format!("{topic_prefix}/+/command");
    let event_prefix = topic_prefix.clone();
    thread::spawn(move || {
        for notification in connection.iter() {
            match notification {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    *event_status.lock().unwrap() = MqttStatus::Connected;
                    // (Re)subscribe to the command topic for every scale.
                    let _ = sub_client.subscribe(&command_filter, QoS::AtLeastOnce);
                }
                Ok(Event::Incoming(Packet::Publish(p))) => {
                    if let Some(serial) = command_serial(&p.topic, &event_prefix) {
                        let cmd = String::from_utf8_lossy(&p.payload);
                        if cmd.trim() == "zero{Z}" {
                            event_app.lock().unwrap().zero_by_serial(serial);
                        }
                    }
                }
                Err(e) => {
                    *event_status.lock().unwrap() = MqttStatus::Error(e.to_string());
                }
                _ => {}
            }
        }
    });

    // Publish loop thread: every interval, publish each scale to its own
    // topic `{prefix}/{serialNumber}`.
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(interval_ms));

        // Snapshot (topic, payload, serial) under the lock, then release it
        // before doing any network work.
        let outgoing: Vec<(String, String, String)> = {
            let state = app.lock().unwrap();
            state
                .scales
                .iter()
                .filter_map(|s| {
                    serde_json::to_string(s)
                        .ok()
                        .map(|json| (format!("{topic_prefix}/{}", s.serial_number), json, s.serial_number.clone()))
                })
                .collect()
        };

        for (topic, payload, serial) in outgoing {
            if client.publish(topic, QoS::AtLeastOnce, false, payload).is_ok() {
                app.lock().unwrap().mark_published(&serial, Instant::now());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::command_serial;

    #[test]
    fn parses_serial_from_command_topic() {
        assert_eq!(command_serial("scale/SIM-001/command", "scale"), Some("SIM-001"));
    }

    #[test]
    fn rejects_non_command_and_mismatched_topics() {
        // Publish topic, not a command topic.
        assert_eq!(command_serial("scale/SIM-001", "scale"), None);
        // Wrong prefix.
        assert_eq!(command_serial("other/SIM-001/command", "scale"), None);
        // Missing serial.
        assert_eq!(command_serial("scale//command", "scale"), None);
        // Prefix-only false match (no trailing slash).
        assert_eq!(command_serial("scaleX/SIM-001/command", "scale"), None);
    }
}
