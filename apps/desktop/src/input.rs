use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
        MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
        MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
        MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, SendInput,
    },
    WindowsAndMessaging::{SetCursorPos, XBUTTON1, XBUTTON2},
};

pub const MAX_INPUT_BYTES: usize = 256;
const MAX_EVENTS_PER_SECOND: u16 = 120;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteInput {
    PointerMove {
        x: u16,
        y: u16,
    },
    PointerButton {
        button: MouseButton,
        down: bool,
        x: u16,
        y: u16,
    },
    PointerWheel {
        delta: i16,
        horizontal: bool,
        x: u16,
        y: u16,
    },
    Key {
        code: String,
        down: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[cfg(test)]
fn parse_input(bytes: &[u8]) -> Result<RemoteInput, &'static str> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("remote input payload is too large");
    }
    let input: RemoteInput = serde_json::from_slice(bytes).map_err(|_| "invalid remote input")?;
    validate_input(&input)?;
    Ok(input)
}

pub fn validate_input(input: &RemoteInput) -> Result<(), &'static str> {
    if let RemoteInput::PointerWheel { delta, .. } = input {
        if !(-1200..=1200).contains(delta) {
            return Err("unsupported wheel delta");
        }
    }
    if let RemoteInput::Key { code, .. } = &input {
        if virtual_key(code).is_none() {
            return Err("unsupported key code");
        }
    }
    Ok(())
}

pub struct InputRateLimit {
    window_start: Instant,
    events: u16,
}

impl InputRateLimit {
    pub fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            events: 0,
        }
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.events = 0;
        }
        if self.events >= MAX_EVENTS_PER_SECOND {
            return false;
        }
        self.events += 1;
        true
    }
}

fn virtual_key(code: &str) -> Option<u16> {
    match code {
        "Enter" => Some(0x0D),
        "Escape" => Some(0x1B),
        "Backspace" => Some(0x08),
        "Tab" => Some(0x09),
        "Space" => Some(0x20),
        "NumpadEnter" => Some(0x0D),
        "ArrowLeft" => Some(0x25),
        "ArrowUp" => Some(0x26),
        "ArrowRight" => Some(0x27),
        "ArrowDown" => Some(0x28),
        "ShiftLeft" | "ShiftRight" => Some(0x10),
        "ControlLeft" | "ControlRight" => Some(0x11),
        "AltLeft" | "AltRight" => Some(0x12),
        "Delete" => Some(0x2E),
        "Insert" => Some(0x2D),
        "Home" => Some(0x24),
        "End" => Some(0x23),
        "PageUp" => Some(0x21),
        "PageDown" => Some(0x22),
        "CapsLock" => Some(0x14),
        "NumLock" => Some(0x90),
        "ScrollLock" => Some(0x91),
        "PrintScreen" => Some(0x2C),
        "Pause" => Some(0x13),
        "ContextMenu" => Some(0x5D),
        "NumpadMultiply" => Some(0x6A),
        "NumpadAdd" => Some(0x6B),
        "NumpadComma" => Some(0x6C),
        "NumpadSubtract" => Some(0x6D),
        "NumpadDecimal" => Some(0x6E),
        "NumpadDivide" => Some(0x6F),
        "NumpadEqual" => Some(0xBB),
        "IntlBackslash" => Some(0xE2),
        "MetaLeft" => Some(0x5B),
        "MetaRight" => Some(0x5C),
        "Minus" => Some(0xBD),
        "Equal" => Some(0xBB),
        "BracketLeft" => Some(0xDB),
        "BracketRight" => Some(0xDD),
        "Backslash" => Some(0xDC),
        "Semicolon" => Some(0xBA),
        "Quote" => Some(0xDE),
        "Backquote" => Some(0xC0),
        "Comma" => Some(0xBC),
        "Period" => Some(0xBE),
        "Slash" => Some(0xBF),
        _ => {
            if let Some(number) = code
                .strip_prefix('F')
                .and_then(|number| number.parse::<u8>().ok())
            {
                if (1..=24).contains(&number) {
                    return Some(0x70 + u16::from(number - 1));
                }
            }
            let bytes = code.as_bytes();
            match bytes {
                [b'K', b'e', b'y', letter @ b'A'..=b'Z'] => Some(*letter as u16),
                [b'D', b'i', b'g', b'i', b't', digit @ b'0'..=b'9'] => Some(*digit as u16),
                [b'N', b'u', b'm', b'p', b'a', b'd', digit @ b'0'..=b'9'] => {
                    Some(0x60 + u16::from(*digit - b'0'))
                }
                _ => None,
            }
        }
    }
}

#[derive(Default)]
pub struct HeldInput {
    keys: HashSet<u16>,
    buttons: HashSet<MouseButton>,
}

#[derive(Clone, Copy)]
pub struct MonitorGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl MonitorGeometry {
    fn pixel_at(self, x: u16, y: u16) -> (i32, i32) {
        let px = self.x
            + (u64::from(x) * u64::from(self.width.saturating_sub(1)) / u64::from(u16::MAX)) as i32;
        let py = self.y
            + (u64::from(y) * u64::from(self.height.saturating_sub(1)) / u64::from(u16::MAX))
                as i32;
        (px, py)
    }
    pub fn primary() -> Result<Self, String> {
        let monitor = crate::screen::primary_monitor()?;
        Ok(Self {
            x: monitor.x().map_err(|error| error.to_string())?,
            y: monitor.y().map_err(|error| error.to_string())?,
            width: monitor.width().map_err(|error| error.to_string())?,
            height: monitor.height().map_err(|error| error.to_string())?,
        })
    }
}

impl HeldInput {
    fn release_all_with(
        &mut self,
        mut release_key: impl FnMut(u16) -> bool,
        mut release_button: impl FnMut(MouseButton) -> bool,
    ) {
        self.keys.retain(|key| !release_key(*key));
        self.buttons.retain(|button| !release_button(*button));
    }

    #[cfg(target_os = "windows")]
    pub fn apply(
        &mut self,
        input: RemoteInput,
        monitor: MonitorGeometry,
    ) -> Result<(), &'static str> {
        match input {
            RemoteInput::PointerMove { x, y } => move_pointer(monitor, x, y),
            RemoteInput::PointerButton { button, down, x, y } => {
                move_pointer(monitor, x, y)?;
                let flags = mouse_flags(button, down);
                send(INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: mouse_data(button),
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                })?;
                if down {
                    self.buttons.insert(button);
                } else {
                    self.buttons.remove(&button);
                }
                Ok(())
            }
            RemoteInput::PointerWheel {
                delta,
                horizontal,
                x,
                y,
            } => {
                move_pointer(monitor, x, y)?;
                send(INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEINPUT {
                            mouseData: delta as i32 as u32,
                            dwFlags: if horizontal {
                                MOUSEEVENTF_HWHEEL
                            } else {
                                MOUSEEVENTF_WHEEL
                            },
                            ..Default::default()
                        },
                    },
                })
            }
            RemoteInput::Key { code, down } => {
                let key = virtual_key(&code).ok_or("unsupported key code")?;
                send(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: key,
                            wScan: 0,
                            dwFlags: if down { 0 } else { KEYEVENTF_KEYUP },
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                })?;
                if down {
                    self.keys.insert(key);
                } else {
                    self.keys.remove(&key);
                }
                Ok(())
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub fn release_all(&mut self) {
        self.release_all_with(
            |key| {
                send(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: key,
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                })
                .is_ok()
            },
            |button| {
                send(INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEINPUT {
                            dwFlags: mouse_flags(button, false),
                            mouseData: mouse_data(button),
                            ..Default::default()
                        },
                    },
                })
                .is_ok()
            },
        );
    }
}

#[cfg(target_os = "windows")]
fn move_pointer(monitor: MonitorGeometry, x: u16, y: u16) -> Result<(), &'static str> {
    let (px, py) = monitor.pixel_at(x, y);
    if unsafe { SetCursorPos(px, py) } == 0 {
        Err("Windows rejected remote pointer input")
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn mouse_flags(button: MouseButton, down: bool) -> u32 {
    match (button, down) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        (MouseButton::Back, true) | (MouseButton::Forward, true) => MOUSEEVENTF_XDOWN,
        (MouseButton::Back, false) | (MouseButton::Forward, false) => MOUSEEVENTF_XUP,
    }
}

#[cfg(target_os = "windows")]
fn mouse_data(button: MouseButton) -> u32 {
    match button {
        MouseButton::Back => u32::from(XBUTTON1),
        MouseButton::Forward => u32::from(XBUTTON2),
        _ => 0,
    }
}

#[cfg(target_os = "windows")]
fn send(input: INPUT) -> Result<(), &'static str> {
    if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } == 1 {
        Ok(())
    } else {
        Err("Windows rejected remote input")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HeldInput, InputRateLimit, MonitorGeometry, MouseButton, RemoteInput, parse_input,
        validate_input,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn rejects_unknown_keys_and_out_of_range_pointer_values() {
        assert!(parse_input(br#"{"type":"key","code":"KeyA","down":true}"#).is_ok());
        assert!(parse_input(br#"{"type":"key","code":"WinKey","down":true}"#).is_err());
        assert!(parse_input(br#"{"type":"pointer_move","x":70000,"y":1}"#).is_err());
        assert!(
            validate_input(&RemoteInput::PointerWheel {
                delta: 2000,
                horizontal: false,
                x: 0,
                y: 0
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_oversized_payloads_and_unsupported_buttons() {
        assert!(parse_input(&vec![b'a'; 257]).is_err());
        assert!(
            parse_input(br#"{"type":"pointer_button","button":"x9","down":true,"x":0,"y":0}"#)
                .is_err()
        );
        assert!(
            parse_input(br#"{"type":"pointer_button","button":"back","down":true,"x":0,"y":0}"#)
                .is_ok()
        );
    }

    #[test]
    fn rate_limit_resets_after_one_second() {
        let now = Instant::now();
        let mut limit = InputRateLimit::new(now);
        for _ in 0..120 {
            assert!(limit.allow(now));
        }
        assert!(!limit.allow(now));
        assert!(limit.allow(now + Duration::from_secs(1)));
    }

    #[test]
    fn input_messages_round_trip() {
        let input = RemoteInput::Key {
            code: "KeyA".into(),
            down: true,
        };
        let bytes = serde_json::to_vec(&input).unwrap();
        assert_eq!(parse_input(&bytes).unwrap(), input);
    }

    #[test]
    fn maps_function_and_numpad_keys() {
        assert!(parse_input(br#"{"type":"key","code":"F24","down":true}"#).is_ok());
        assert!(parse_input(br#"{"type":"key","code":"Numpad7","down":true}"#).is_ok());
        assert!(parse_input(br#"{"type":"key","code":"F25","down":true}"#).is_err());
    }

    #[test]
    fn pointer_coordinates_map_to_capture_monitor_edges() {
        let monitor = MonitorGeometry {
            x: -1920,
            y: 120,
            width: 1920,
            height: 1080,
        };

        assert_eq!(monitor.pixel_at(0, 0), (-1920, 120));
        assert_eq!(monitor.pixel_at(u16::MAX, 0), (-1, 120));
        assert_eq!(monitor.pixel_at(0, u16::MAX), (-1920, 1199));
        assert_eq!(monitor.pixel_at(u16::MAX, u16::MAX), (-1, 1199));
    }

    #[test]
    fn revoking_control_releases_all_held_keys_and_buttons() {
        let mut held = HeldInput::default();
        held.keys.insert(0x11);
        held.buttons.insert(MouseButton::Left);

        let mut released_keys = Vec::new();
        let mut released_buttons = Vec::new();
        held.release_all_with(
            |key| {
                released_keys.push(key);
                true
            },
            |button| {
                released_buttons.push(button);
                true
            },
        );
        assert_eq!(released_keys, vec![0x11]);
        assert_eq!(released_buttons, vec![MouseButton::Left]);
        assert!(held.keys.is_empty());
        assert!(held.buttons.is_empty());
    }

    #[test]
    fn held_input_is_kept_for_retry_when_release_dispatch_fails() {
        let mut held = HeldInput::default();
        held.keys.insert(0x11);
        held.buttons.insert(MouseButton::Left);
        held.release_all_with(|_| false, |_| false);
        assert_eq!(held.keys.len(), 1);
        assert_eq!(held.buttons.len(), 1);

        held.release_all_with(|_| true, |_| true);
        assert!(held.keys.is_empty());
        assert!(held.buttons.is_empty());
    }
}
