//! The real controllers in the browser build (WEB.md): the page's Gamepad API, read once per
//! frame with `navigator.getGamepads()`, in its standard mapping (`gamepad::standard_state`), and
//! vibration through the pad's `vibrationActuator` where the browser has one. gilrs has no
//! WebAssembly backend; a DualSense's adaptive triggers and lights would need WebHID, so they are
//! left out. Browsers show a pad to the page only after one of its buttons is pressed.
use super::{super::Game, Pads};
use horizon_client::gamepad::{self, GlyphStyle, PadState, Pulse, Rumble};
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};

/// A pad the page sees.
struct Pad {
    index: u32,
    id: String,
    /// In the standard mapping; other layouts are unknown, so they are not read.
    standard: bool,
    /// Something on it was pushed at the last poll.
    touched: bool,
}

/// The pads the page saw at the last poll and the one in use.
#[derive(Default)]
pub(super) struct Devices {
    pads: Vec<Pad>,
    /// The pad last touched (its `Gamepad.index`); only its input counts.
    active: Option<u32>,
}

/// The connected pads; none where the page may not read them (an insecure context, a
/// permissions policy) or the browser has no Gamepad API.
fn gamepads() -> Vec<web_sys::Gamepad> {
    let Some(list) = web_sys::window().and_then(|w| w.navigator().get_gamepads().ok()) else {
        return Vec::new();
    };
    // The list has holes (`null`) where pads were unplugged.
    list.iter()
        .filter_map(|p| p.dyn_into::<web_sys::Gamepad>().ok())
        .filter(|p| p.connected())
        .collect()
}

fn read_pad(pad: &web_sys::Gamepad) -> PadState {
    let buttons: Vec<(bool, f32)> = pad
        .buttons()
        .iter()
        .map(|b| {
            b.dyn_into::<web_sys::GamepadButton>()
                .map_or((false, 0.0), |b| (b.pressed(), b.value() as f32))
        })
        .collect();
    let axes: Vec<f32> = pad
        .axes()
        .iter()
        .map(|a| a.as_f64().unwrap_or(0.0) as f32)
        .collect();
    gamepad::standard_state(&buttons, &axes)
}

/// Pushed far enough to make this the pad in use, as gilrs' backend does on its events.
fn touched(state: &PadState) -> bool {
    let far = |v: [f32; 2]| v[0].abs() > 0.5 || v[1].abs() > 0.5;
    state.buttons != 0
        || far(state.left)
        || far(state.right)
        || state.triggers.iter().any(|t| *t > 0.5)
}

impl Pads {
    /// Read every connected pad, note connections, and return the active pad's state.
    pub(super) fn read(&mut self) -> PadState {
        let mut now = Vec::new();
        let mut active = None;
        for pad in gamepads() {
            let (index, id) = (pad.index(), pad.id());
            let standard = pad.mapping() == web_sys::GamepadMappingType::Standard;
            let old = self
                .devices
                .pads
                .iter()
                .find(|p| p.index == index && p.id == id);
            if old.is_none() {
                log::info!("Controller connected: {}", gamepad::browser_name(&id));
                if !standard {
                    log::warn!(
                        "Horizon: this browser has no standard mapping for {id}; it is not read"
                    );
                }
            }
            let state = if standard {
                read_pad(&pad)
            } else {
                PadState::default()
            };
            let touched = touched(&state);
            if touched && !old.is_some_and(|p| p.touched) {
                self.devices.active = Some(index);
            }
            if self.devices.active == Some(index) {
                active = Some((state, id.clone()));
            }
            now.push(Pad {
                index,
                id,
                standard,
                touched,
            });
        }
        for old in &self.devices.pads {
            if !now.iter().any(|p| p.index == old.index && p.id == old.id) {
                log::info!(
                    "Controller disconnected: {}",
                    gamepad::browser_name(&old.id)
                );
            }
        }
        self.devices.pads = now;
        let Some((state, id)) = active else {
            self.devices.active = None;
            return PadState::default();
        };
        self.style = GlyphStyle::of_vendor(gamepad::browser_ids(&id).0);
        state
    }
}

impl Game<'_> {
    pub(super) fn pad_output_off(&mut self) {}
    pub(super) fn pad_output_frame(&mut self) {}
    pub(in crate::game) fn pad_close_output(&mut self) {}

    /// Play a vibration on the active pad with `vibrationActuator.playEffect("dual-rumble")`
    /// (Chrome and Edge; others have no actuator, and the pad stays still).
    pub(super) fn pad_vibrate(&mut self, _: Rumble, pulse: Pulse) {
        let Some(index) = self.pad.devices.active else {
            return;
        };
        let Some(pad) = gamepads().into_iter().find(|p| p.index() == index) else {
            return;
        };
        // Called through Reflect: web-sys has the actuator only behind its unstable APIs.
        let get = |target: &JsValue, key: &str| js_sys::Reflect::get(target, &key.into()).ok();
        let Some(actuator) = get(&pad, "vibrationActuator").filter(|a| a.is_object()) else {
            return;
        };
        let Some(play) =
            get(&actuator, "playEffect").and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        else {
            return;
        };
        let params = js_sys::Object::new();
        for (key, value) in [
            ("duration", f64::from(pulse.millis)),
            ("startDelay", 0.0),
            ("strongMagnitude", f64::from(pulse.strong.clamp(0.0, 1.0))),
            ("weakMagnitude", f64::from(pulse.weak.clamp(0.0, 1.0))),
        ] {
            let _ = js_sys::Reflect::set(&params, &key.into(), &value.into());
        }
        match play.call2(&actuator, &"dual-rumble".into(), &params) {
            // Awaited so a refusal (the effect is unsupported) is not an unhandled rejection.
            Ok(promise) => {
                if let Ok(promise) = promise.dyn_into::<js_sys::Promise>() {
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
                            log::debug!("Horizon: controller vibration: {e:?}");
                        }
                    });
                }
            }
            Err(e) => log::debug!("Horizon: controller vibration: {e:?}"),
        }
    }

    /// The hardware part of `pad status`.
    pub(super) fn pad_devices_status(&self) -> Value {
        let devices = &self.pad.devices;
        let pads: Vec<Value> = devices
            .pads
            .iter()
            .map(|p| {
                let (vendor, product) = gamepad::browser_ids(&p.id);
                json!({
                    "name": gamepad::browser_name(&p.id),
                    "vendor": vendor,
                    "product": product,
                    "standard_mapping": p.standard,
                    "active": devices.active == Some(p.index),
                })
            })
            .collect();
        json!({ "backend": true, "api": "gamepad", "controllers": pads })
    }
}
