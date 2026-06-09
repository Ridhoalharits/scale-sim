use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::state::{AddScaleInput, AppState, InputStage, WeightInput};

pub enum Action {
    Continue,
    Quit,
}

pub fn handle_key(app: &mut AppState, key: KeyEvent) -> Action {
    if key.kind != KeyEventKind::Press {
        return Action::Continue;
    }

    if app.input.is_some() {
        handle_input_mode(app, key);
        return Action::Continue;
    }

    if app.weight_input.is_some() {
        handle_weight_input(app, key);
        return Action::Continue;
    }

    match key.code {
        KeyCode::Up => app.select_prev(),
        KeyCode::Down => app.select_next(),
        KeyCode::Left => app.adjust_selected(-0.001),
        KeyCode::Right => app.adjust_selected(0.001),
        KeyCode::PageUp => app.adjust_selected(0.100),
        KeyCode::PageDown => app.adjust_selected(-0.100),
        KeyCode::Char('z') | KeyCode::Char('Z') => app.zero_selected(),
        KeyCode::Char('w') | KeyCode::Char('W') => {
            // Prefill with the current weight so it can be tweaked in place.
            if let Some(w) = app.selected_weight() {
                app.weight_input = Some(WeightInput {
                    buffer: format!("{w:.3}"),
                });
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => app.input = Some(AddScaleInput::default()),
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Action::Quit,
        _ => {}
    }
    Action::Continue
}

/// Drives the "set weight" popup: the user types a number and presses
/// Enter to apply it to the selected scale. Invalid input keeps the form
/// open so it can be corrected.
fn handle_weight_input(app: &mut AppState, key: KeyEvent) {
    let mut input = app.weight_input.take().unwrap();

    match key.code {
        KeyCode::Esc => {} // dropped = cancelled
        KeyCode::Enter => match input.parsed() {
            Some(value) => app.set_selected_weight(value),
            None => app.weight_input = Some(input), // keep open to fix
        },
        KeyCode::Backspace => {
            input.buffer.pop();
            app.weight_input = Some(input);
        }
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' || c == '-' => {
            input.buffer.push(c);
            app.weight_input = Some(input);
        }
        _ => app.weight_input = Some(input),
    }
}

/// Drives the add-scale popup. Takes ownership of the input so the borrow
/// checker lets us mutate `app` (e.g. `add_scale`) inside; we put it back
/// unless the form is cancelled or submitted.
fn handle_input_mode(app: &mut AppState, key: KeyEvent) {
    let mut input = app.input.take().unwrap();

    match input.stage {
        InputStage::Serial => match key.code {
            KeyCode::Esc => {} // dropped = cancelled
            KeyCode::Enter => {
                if !input.serial.trim().is_empty() {
                    input.stage = InputStage::Unit;
                }
                app.input = Some(input);
            }
            KeyCode::Backspace => {
                input.serial.pop();
                app.input = Some(input);
            }
            KeyCode::Char(c) => {
                input.serial.push(c);
                app.input = Some(input);
            }
            _ => app.input = Some(input),
        },
        InputStage::Unit => match key.code {
            KeyCode::Esc => {} // dropped = cancelled
            KeyCode::Left => {
                input.cycle_unit(false);
                app.input = Some(input);
            }
            KeyCode::Right => {
                input.cycle_unit(true);
                app.input = Some(input);
            }
            KeyCode::Enter => {
                // If add fails (blank/dup), reopen the form on the serial field.
                if !app.add_scale(input.serial.clone(), input.unit().to_string()) {
                    input.stage = InputStage::Serial;
                    app.input = Some(input);
                }
            }
            _ => app.input = Some(input),
        },
    }
}
