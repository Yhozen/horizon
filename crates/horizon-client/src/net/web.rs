//! Browser WebSockets are asynchronous: callbacks run independently of animation frames,
//! so keep-alives still receive replies when the tab is in the background.
use super::{Event, browser::Protocol};
use crate::auth::AuthSession;
use horizon_protocol::{connection::Counters, packets::play, text::Language};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{BinaryType, CloseEvent, MessageEvent, WebSocket};

struct State {
    protocol: Protocol,
    socket: Option<WebSocket>,
}

impl State {
    fn end(&mut self, reason: String, from_server: bool) {
        self.protocol.end(reason, from_server);
        if let Some(socket) = &self.socket {
            let _ = socket.close();
        }
    }

    fn flush(&mut self) {
        let Some(socket) = self.socket.as_ref() else {
            return;
        };
        if socket.ready_state() != WebSocket::OPEN {
            return;
        }
        if socket.buffered_amount() > 8 * 1024 * 1024 {
            self.end(
                "Connection is too slow; reconnect to continue".into(),
                false,
            );
            return;
        }
        for bytes in std::mem::take(&mut self.protocol.outgoing) {
            if socket.send_with_u8_array(&bytes).is_err() {
                self.end("Could not send to the multiplayer gateway".into(), false);
                return;
            }
        }
        if self.protocol.closed() {
            let _ = socket.close();
        }
    }
}

pub struct Session {
    state: Rc<RefCell<State>>,
    // Removing the handlers before dropping these closures also breaks their JS references.
    _handlers: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    pub address: String,
    pub name: String,
    pub counters: Arc<Mutex<Option<Arc<Counters>>>>,
}

fn endpoint(address: &str) -> Result<(String, String, u16), String> {
    if address.starts_with("ws://") || address.starts_with("wss://") {
        let url = web_sys::Url::new(address).map_err(|_| "Invalid WebSocket server URL")?;
        return Ok((url.href(), url.hostname(), 25565));
    }
    if address.starts_with(super::FRIEND) {
        return Err(
            "Invite-code tunnels are not available in the browser; use the multiplayer gateway"
                .into(),
        );
    }
    let (host, port) =
        horizon_protocol::servers::parse_address(address).ok_or("Invalid server address")?;
    if host.len() > 255 {
        return Err("Server hostname is too long".into());
    }
    let location = web_sys::window()
        .ok_or("Missing browser window")?
        .location();
    let scheme = if location.protocol().map_err(|_| "Missing page URL")? == "https:" {
        "wss"
    } else {
        "ws"
    };
    let origin = location.host().map_err(|_| "Missing page host")?;
    Ok((format!("{scheme}://{origin}/connect"), host, port))
}

impl Session {
    pub fn connect(
        address: &str,
        name: &str,
        _auth: Option<Arc<AuthSession>>,
        view_distance: i8,
        lang: Arc<Language>,
    ) -> Self {
        let target = endpoint(address);
        let (host, port) = target
            .as_ref()
            .map(|(_, h, p)| (h.as_str(), *p))
            .unwrap_or(("server", 25565));
        let valid_name = !name.is_empty()
            && name.len() <= 16
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        let protocol = Protocol::new(
            host,
            port,
            if valid_name { name } else { "WebPlayer" },
            view_distance,
            lang,
        )
        .expect("small login packets fit a protocol frame");
        let counters = Arc::new(Mutex::new(Some(protocol.counters.clone())));
        let state = Rc::new(RefCell::new(State {
            protocol,
            socket: None,
        }));
        let mut session = Self {
            state,
            _handlers: Vec::new(),
            counters,
            address: address.into(),
            name: name.into(),
        };
        if !valid_name {
            session.state.borrow_mut().end(
                "Choose a player name with 1–16 letters, digits or underscores".into(),
                true,
            );
            return session;
        }
        let socket = match target.and_then(|(url, _, _)| {
            WebSocket::new(&url)
                .map_err(|e| format!("Could not open the multiplayer gateway: {e:?}"))
        }) {
            Ok(socket) => socket,
            Err(reason) => {
                session.state.borrow_mut().end(reason, true);
                return session;
            }
        };
        socket.set_binary_type(BinaryType::Arraybuffer);
        session.state.borrow_mut().socket = Some(socket.clone());
        for kind in ["open", "message", "error", "close"] {
            let weak = Rc::downgrade(&session.state);
            let handler = Closure::<dyn FnMut(web_sys::Event)>::new(
                move |event: web_sys::Event| {
                    let Some(state) = weak.upgrade() else { return };
                    let mut state = state.borrow_mut();
                    if state.protocol.closed() {
                        return;
                    }
                    match kind {
                    "open" => state.flush(),
                    "message" => {
                        let data = event.unchecked_ref::<MessageEvent>().data();
                        if !data.is_instance_of::<js_sys::ArrayBuffer>() {
                            state.end("The multiplayer gateway sent a nonbinary message".into(), false);
                            return;
                        }
                        let bytes = js_sys::Uint8Array::new(&data).to_vec();
                        if let Err(error) = state.protocol.receive(&bytes) {
                            state.end(format!("Protocol error: {error}"), false);
                        }
                        state.flush();
                    }
                    "error" => state.end("Cannot reach the multiplayer gateway. Check your connection and sign in again if your session expired.".into(), false),
                    "close" => {
                        let event = event.unchecked_ref::<CloseEvent>();
                        let reason = if event.reason().is_empty() {
                            format!("Connection closed by the server ({})", event.code())
                        } else { event.reason() };
                        state.end(reason, false);
                    }
                    _ => {}
                }
                },
            );
            let callback = Some(handler.as_ref().unchecked_ref());
            match kind {
                "open" => socket.set_onopen(callback),
                "message" => socket.set_onmessage(callback),
                "error" => socket.set_onerror(callback),
                "close" => socket.set_onclose(callback),
                _ => unreachable!(),
            }
            session._handlers.push(handler);
        }
        session
    }

    pub fn connect_local(
        _pipe: horizon_protocol::connection::Pipe,
        name: &str,
        view_distance: i8,
        lang: Arc<Language>,
    ) -> Self {
        let session = Self::connect("friend:unsupported", name, None, view_distance, lang);
        session.state.borrow_mut().end(
            "Use in-frame single player or join the multiplayer gateway".into(),
            true,
        );
        session
    }

    pub fn poll(&self) -> Vec<Event> {
        std::mem::take(&mut self.state.borrow_mut().protocol.events)
    }

    pub fn send(&self, packet: play::Serverbound) {
        let mut state = self.state.borrow_mut();
        if state.protocol.closed() {
            return;
        }
        if let Err(error) = state.protocol.send(&packet.encode()) {
            state.end(error.to_string(), false);
        }
        state.flush();
    }

    pub fn disconnect(&mut self) {
        let mut state = self.state.borrow_mut();
        if let Some(socket) = &state.socket {
            socket.set_onopen(None);
            socket.set_onmessage(None);
            socket.set_onerror(None);
            socket.set_onclose(None);
        }
        state.end("Disconnected".into(), true);
        self._handlers.clear();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.disconnect();
    }
}
