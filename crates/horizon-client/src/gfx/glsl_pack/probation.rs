//! Watching a pack's pipelines and first frames for GPU validation errors.
//! Natively an error scope's verdict is waited for at once; in the browser it
//! arrives a few frames later, so the errors are collected as they come and
//! the game asks for them every frame (a rejected pack is removed and its
//! report says why, rather than leaving a black screen).
//!
//! Errors outside those scopes (a program first drawn in rain, minutes in)
//! reach the device's uncaptured-error handler: while a pack is installed
//! they go into the same list, so the pack is turned off with its error in
//! the report instead of wgpu's default handler panicking.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// Frames after an install or a resize that run inside an error scope.
const FRAMES: u8 = 3;

#[derive(Default)]
pub(crate) struct Probation {
    frames: u8,
    errors: Rc<RefCell<Vec<String>>>,
    late: Uncaptured,
}

/// The device's uncaptured errors while a pack is installed.
#[derive(Clone, Default)]
pub(crate) struct Uncaptured(Arc<Mutex<UncapturedState>>);

#[derive(Default)]
struct UncapturedState {
    pack: bool,
    errors: Vec<String>,
}

impl Uncaptured {
    /// Takes the device's uncaptured errors from now on.
    pub fn handle(&self, device: &wgpu::Device) {
        let me = self.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            let text = e.to_string();
            if !me.record(&text) {
                // No pack: what wgpu does by default (natively it panics).
                #[cfg(not(target_arch = "wasm32"))]
                panic!("wgpu error: {text}\n");
                #[cfg(target_arch = "wasm32")]
                log::error!("wgpu error: {text}");
            }
        }));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, UncapturedState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether a pack is installed (errors are kept rather than fatal).
    pub fn set_pack(&self, on: bool) {
        let mut s = self.lock();
        s.pack = on;
        if !on {
            s.errors.clear();
        }
    }

    /// Keeps an error while a pack is installed; false without one.
    fn record(&self, error: &str) -> bool {
        let mut s = self.lock();
        if s.pack {
            s.errors.push(format!("late: {error}"));
        }
        s.pack
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().errors)
    }
}

impl Probation {
    /// Watching `device`'s uncaptured errors too.
    pub fn new(device: &wgpu::Device) -> Self {
        let late = Uncaptured::default();
        late.handle(device);
        Self {
            late,
            ..Self::default()
        }
    }

    /// A fresh watch with no pack installed, keeping the device's handler.
    pub fn reset(&mut self) {
        self.frames = 0;
        self.errors = Rc::default();
        self.late.set_pack(false);
    }

    /// The pack is installed: the device's uncaptured errors are its own from now on.
    pub fn installed(&mut self) {
        self.late.set_pack(true);
    }
    /// Whether this frame should run inside an error scope; counts it down.
    pub fn watching(&mut self) -> bool {
        if self.frames == 0 {
            return false;
        }
        self.frames -= 1;
        true
    }

    /// Watch the next frames again (after a resize rebuilt the targets).
    pub fn rewatch(&mut self) {
        self.frames = FRAMES;
    }

    /// Collects the error, if any, of a popped scope.
    pub fn watch(
        &mut self,
        what: &'static str,
        verdict: impl Future<Output = Option<wgpu::Error>> + 'static,
    ) {
        self.frames = self.frames.max(u8::from(what == "pipelines") * FRAMES);
        let errors = self.errors.clone();
        let record = move |e: Option<wgpu::Error>| {
            if let Some(e) = e {
                errors.borrow_mut().push(format!("{what}: {e}"));
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        record(pollster::block_on(verdict));
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { record(verdict.await) });
    }

    /// The errors seen so far, joined (taking them).
    pub fn error(&mut self) -> Option<String> {
        let mut errors = self.errors.borrow_mut();
        errors.extend(self.late.take());
        if errors.is_empty() {
            return None;
        }
        let text = errors.join("\n");
        errors.clear();
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_errors_are_kept_only_while_a_pack_is_installed() {
        let mut p = Probation::default();
        assert!(!p.late.record("before"), "no pack: the error stays fatal");
        p.installed();
        assert!(
            p.late
                .record("Validation Error in pipeline 'gbuffers_weather'")
        );
        let error = p.error().expect("the late error reaches the report");
        assert!(error.contains("gbuffers_weather"), "{error}");
        assert_eq!(p.error(), None, "taken once");
        p.reset();
        assert!(!p.late.record("after"));
        assert_eq!(p.error(), None);
    }
}
