use std::time::Instant;

use serde::Serialize;

/// Selectable units of measure.
pub const UNITS: [&str; 4] = ["kg", "g", "lb", "oz"];

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scale {
    pub serial_number: String,
    pub weight_value: f64, // 3 decimal precision
    pub unit: String,      // one of UNITS
    pub tare_value: f64,   // always 0.0 for now
    #[serde(skip)]
    pub last_publish: Option<Instant>,
}

impl Scale {
    pub fn new(serial_number: String, unit: String) -> Self {
        Self {
            serial_number,
            weight_value: 0.0,
            unit,
            tare_value: 0.0,
            last_publish: None,
        }
    }

    pub fn adjust_weight(&mut self, delta: f64) {
        self.weight_value = round3(self.weight_value + delta);
    }

    pub fn set_weight(&mut self, value: f64) {
        self.weight_value = round3(value);
    }

    pub fn zero(&mut self) {
        self.weight_value = 0.0;
    }
}

/// Which field of the add-scale form is being edited.
#[derive(Clone, Copy, PartialEq)]
pub enum InputStage {
    Serial,
    Unit,
}

/// Transient state for the "add a scale" popup.
#[derive(Clone)]
pub struct AddScaleInput {
    pub serial: String,
    pub unit_index: usize,
    pub stage: InputStage,
}

impl Default for AddScaleInput {
    fn default() -> Self {
        Self {
            serial: String::new(),
            unit_index: 0,
            stage: InputStage::Serial,
        }
    }
}

impl AddScaleInput {
    pub fn unit(&self) -> &'static str {
        UNITS[self.unit_index]
    }

    pub fn cycle_unit(&mut self, forward: bool) {
        let n = UNITS.len();
        self.unit_index = if forward {
            (self.unit_index + 1) % n
        } else {
            (self.unit_index + n - 1) % n
        };
    }
}

/// Transient state for the "set weight" popup, where the user types an
/// exact weight value for the selected scale instead of nudging it with
/// the arrow keys.
#[derive(Clone, Default)]
pub struct WeightInput {
    pub buffer: String,
}

impl WeightInput {
    /// Parses the typed buffer into a weight, or `None` if it isn't a
    /// valid number (e.g. empty, lone "-", or "1.2.3").
    pub fn parsed(&self) -> Option<f64> {
        self.buffer.trim().parse::<f64>().ok()
    }
}

#[derive(Default)]
pub struct AppState {
    pub scales: Vec<Scale>,
    pub selected: usize,
    pub input: Option<AddScaleInput>,
    pub weight_input: Option<WeightInput>,
}

impl AppState {
    fn selected_scale_mut(&mut self) -> Option<&mut Scale> {
        self.scales.get_mut(self.selected)
    }

    pub fn select_next(&mut self) {
        if !self.scales.is_empty() {
            self.selected = (self.selected + 1) % self.scales.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.scales.is_empty() {
            let n = self.scales.len();
            self.selected = (self.selected + n - 1) % n;
        }
    }

    pub fn adjust_selected(&mut self, delta: f64) {
        if let Some(s) = self.selected_scale_mut() {
            s.adjust_weight(delta);
        }
    }

    pub fn set_selected_weight(&mut self, value: f64) {
        if let Some(s) = self.selected_scale_mut() {
            s.set_weight(value);
        }
    }

    /// Current weight of the selected scale, if any — used to prefill the
    /// "set weight" popup.
    pub fn selected_weight(&self) -> Option<f64> {
        self.scales.get(self.selected).map(|s| s.weight_value)
    }

    pub fn zero_selected(&mut self) {
        if let Some(s) = self.selected_scale_mut() {
            s.zero();
        }
    }

    /// Zeroes the scale with this serial. Returns false if no match —
    /// used to apply a remote `Z` command received over MQTT.
    pub fn zero_by_serial(&mut self, serial: &str) -> bool {
        match self.scales.iter_mut().find(|s| s.serial_number == serial) {
            Some(s) => {
                s.zero();
                true
            }
            None => false,
        }
    }

    /// Adds a scale. Returns false (no-op) if the serial is blank or a
    /// duplicate — the caller keeps the form open so the user can fix it.
    pub fn add_scale(&mut self, serial: String, unit: String) -> bool {
        let serial = serial.trim().to_string();
        if serial.is_empty() || self.scales.iter().any(|s| s.serial_number == serial) {
            return false;
        }
        self.scales.push(Scale::new(serial, unit));
        self.selected = self.scales.len() - 1;
        true
    }

    /// Records a successful publish for the scale with this serial.
    pub fn mark_published(&mut self, serial: &str, at: Instant) {
        if let Some(s) = self.scales.iter_mut().find(|s| s.serial_number == serial) {
            s.last_publish = Some(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_scale_has_defaults() {
        let s = Scale::new("SIM-001".to_string(), "kg".to_string());
        assert_eq!(s.serial_number, "SIM-001");
        assert_eq!(s.weight_value, 0.0);
        assert_eq!(s.unit, "kg");
        assert_eq!(s.tare_value, 0.0);
        assert!(s.last_publish.is_none());
    }

    #[test]
    fn adjust_weight_rounds_and_handles_drift() {
        let mut s = Scale::new("SIM-001".to_string(), "kg".to_string());
        for _ in 0..10 {
            s.adjust_weight(0.1);
        }
        assert_eq!(s.weight_value, 1.0);
        s.adjust_weight(-1.043);
        assert_eq!(s.weight_value, -0.043);
    }

    #[test]
    fn zero_resets_weight() {
        let mut s = Scale::new("SIM-001".to_string(), "g".to_string());
        s.adjust_weight(1.234);
        s.zero();
        assert_eq!(s.weight_value, 0.0);
    }

    #[test]
    fn serializes_camel_case_without_last_publish() {
        let mut s = Scale::new("SIM-001".to_string(), "kg".to_string());
        s.adjust_weight(-0.043);
        s.last_publish = Some(Instant::now());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(
            json,
            r#"{"serialNumber":"SIM-001","weightValue":-0.043,"unit":"kg","tareValue":0.0}"#
        );
    }

    #[test]
    fn zero_by_serial_targets_named_scale() {
        let mut app = AppState::default();
        app.add_scale("A".to_string(), "kg".to_string());
        app.add_scale("B".to_string(), "kg".to_string());
        app.scales[0].adjust_weight(1.5);
        app.scales[1].adjust_weight(2.5);

        assert!(app.zero_by_serial("A"));
        assert_eq!(app.scales[0].weight_value, 0.0);
        assert_eq!(app.scales[1].weight_value, 2.5);

        assert!(!app.zero_by_serial("MISSING"));
    }

    #[test]
    fn add_scale_rejects_blank_and_duplicate() {
        let mut app = AppState::default();
        assert!(app.add_scale("  ".to_string(), "kg".to_string()) == false);
        assert!(app.add_scale("SIM-001".to_string(), "kg".to_string()));
        assert!(app.add_scale("SIM-001".to_string(), "g".to_string()) == false);
        assert_eq!(app.scales.len(), 1);
    }

    #[test]
    fn add_scale_trims_and_selects_new() {
        let mut app = AppState::default();
        app.add_scale("A".to_string(), "kg".to_string());
        app.add_scale("  B  ".to_string(), "lb".to_string());
        assert_eq!(app.scales[1].serial_number, "B");
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn selection_wraps_and_adjust_targets_selected() {
        let mut app = AppState::default();
        app.add_scale("A".to_string(), "kg".to_string());
        app.add_scale("B".to_string(), "kg".to_string());
        app.select_next(); // wrap 1 -> 0
        assert_eq!(app.selected, 0);
        app.select_prev(); // wrap 0 -> 1
        assert_eq!(app.selected, 1);
        app.adjust_selected(0.5);
        assert_eq!(app.scales[1].weight_value, 0.5);
        assert_eq!(app.scales[0].weight_value, 0.0);
    }

    #[test]
    fn adjust_on_empty_is_noop() {
        let mut app = AppState::default();
        app.adjust_selected(0.1);
        app.zero_selected();
        app.select_next();
        assert!(app.scales.is_empty());
    }

    #[test]
    fn set_selected_weight_rounds_and_targets_selected() {
        let mut app = AppState::default();
        app.add_scale("A".to_string(), "kg".to_string());
        app.add_scale("B".to_string(), "kg".to_string());
        app.selected = 0;
        app.set_selected_weight(1.23456);
        assert_eq!(app.scales[0].weight_value, 1.235);
        assert_eq!(app.scales[1].weight_value, 0.0);
        assert_eq!(app.selected_weight(), Some(1.235));
    }

    #[test]
    fn set_selected_weight_on_empty_is_noop() {
        let mut app = AppState::default();
        app.set_selected_weight(5.0);
        assert!(app.scales.is_empty());
        assert_eq!(app.selected_weight(), None);
    }

    #[test]
    fn weight_input_parses_valid_and_rejects_invalid() {
        let valid = WeightInput { buffer: " -2.5 ".to_string() };
        assert_eq!(valid.parsed(), Some(-2.5));
        for bad in ["", "-", ".", "1.2.3", "abc"] {
            let wi = WeightInput { buffer: bad.to_string() };
            assert_eq!(wi.parsed(), None, "expected {bad:?} to be rejected");
        }
    }

    #[test]
    fn cycle_unit_wraps_both_directions() {
        let mut i = AddScaleInput::default();
        assert_eq!(i.unit(), "kg");
        i.cycle_unit(false);
        assert_eq!(i.unit(), UNITS[UNITS.len() - 1]);
        i.cycle_unit(true);
        assert_eq!(i.unit(), "kg");
    }
}
