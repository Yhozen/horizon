//! WebGPU startup and failure signals for the browser menu.
use std::cell::{Cell, RefCell};

use wasm_bindgen::{JsCast, JsValue, closure::Closure};

thread_local! {
    static STARTUP: RefCell<Option<wgpu::ErrorScopeGuard>> = const { RefCell::new(None) };
    static FAILED: Cell<bool> = const { Cell::new(false) };
}

fn dispatch(name: &str, detail: &JsValue) {
    let options = web_sys::CustomEventInit::new();
    options.set_detail(detail);
    if let (Some(window), Ok(event)) = (
        web_sys::window(),
        web_sys::CustomEvent::new_with_event_init_dict(name, &options),
    ) {
        let _ = window.dispatch_event(&event);
    }
}

pub fn report_error(message: &str) {
    if !FAILED.replace(true) {
        dispatch("horizon-render-error", &JsValue::from_str(message));
    }
}

pub fn failed() -> bool {
    FAILED.get()
}

pub(super) fn monitor_device(device: &wgpu::Device) -> Result<(), JsValue> {
    let raw = device.as_webgpu().expect("browser WebGPU device");
    let target = raw.unchecked_ref::<web_sys::EventTarget>();
    let handler = Closure::<dyn FnMut(JsValue)>::new(|event| {
        let message = js_sys::Reflect::get(&event, &"error".into())
            .and_then(|error| js_sys::Reflect::get(&error, &"message".into()))
            .ok()
            .and_then(|message| message.as_string())
            .unwrap_or_default();
        report_error(&format!("WebGPU error: {message}"));
    });
    // Some Safari versions dispatch the event without supporting the handler property.
    target.add_event_listener_with_callback("uncapturederror", handler.as_ref().unchecked_ref())?;
    handler.forget();
    device.set_device_lost_callback(|reason, message| {
        report_error(&format!("WebGPU device lost ({reason:?}): {message}"));
    });
    Ok(())
}

pub(super) fn begin_startup(device: &wgpu::Device) {
    STARTUP.replace(Some(device.push_error_scope(wgpu::ErrorFilter::Validation)));
}

pub(super) fn first_frame_submitted() {
    if let Some(scope) = STARTUP.take() {
        let result = scope.pop();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(error) = result.await {
                report_error(&format!("WebGPU startup failed: {error}"));
            } else if !failed() {
                dispatch("horizon-render-ready", &JsValue::NULL);
            }
        });
    }
}
