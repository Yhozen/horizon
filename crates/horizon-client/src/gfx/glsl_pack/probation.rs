//! Watching a pack's pipelines and first frames for GPU validation errors.
//! Natively an error scope's verdict is waited for at once; in the browser it
//! arrives a few frames later, so the errors are collected as they come and
//! the game asks for them every frame (a rejected pack is removed and its
//! report says why, rather than leaving a black screen).

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

/// Frames after an install or a resize that run inside an error scope.
const FRAMES: u8 = 3;

#[derive(Default)]
pub(crate) struct Probation {
    frames: u8,
    errors: Rc<RefCell<Vec<String>>>,
}

impl Probation {
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
        if errors.is_empty() {
            return None;
        }
        let text = errors.join("\n");
        errors.clear();
        Some(text)
    }
}
