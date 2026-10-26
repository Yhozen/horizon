//! Event-driven counterpart to the native login/configuration receive loop.
//! WebSocket messages carry arbitrary portions of the TCP stream, never packet boundaries.
use super::{Event, NOT_SIGNED_IN, brand, json_text};
use horizon_protocol::{
    PROTOCOL_VERSION, Result,
    buf::Reader,
    connection::Counters,
    crypto::offline_uuid,
    frame,
    packets::{Empty, configuration as cfg, handshake, login, play},
    registry::Dynamic,
    text::{Language, plain},
};
use std::sync::{Arc, atomic::Ordering};

enum Phase {
    Login,
    Configuration,
    Play,
    Closed,
}

pub(super) struct Protocol {
    phase: Phase,
    buffer: Vec<u8>,
    threshold: Option<usize>,
    registries: Dynamic,
    profile: (u128, String),
    reconfiguring: bool,
    view_distance: i8,
    lang: Arc<Language>,
    pub events: Vec<Event>,
    pub outgoing: Vec<Vec<u8>>,
    pub counters: Arc<Counters>,
}

impl Protocol {
    pub fn new(
        host: &str,
        port: u16,
        name: &str,
        view_distance: i8,
        lang: Arc<Language>,
    ) -> Result<Self> {
        let mut this = Self {
            phase: Phase::Login,
            buffer: Vec::new(),
            threshold: None,
            registries: Dynamic::default(),
            profile: (offline_uuid(name), name.to_owned()),
            reconfiguring: false,
            view_distance,
            lang,
            events: vec![Event::Status("Connecting to the server...".into())],
            outgoing: Vec::new(),
            counters: Arc::new(Counters::default()),
        };
        this.send(
            &handshake::Serverbound::Intention(handshake::Intention {
                protocol: PROTOCOL_VERSION,
                host: host.to_owned(),
                port,
                intent: handshake::LOGIN,
            })
            .encode(),
        )?;
        this.send(
            &login::Serverbound::Hello(login::ServerHello {
                name: name.to_owned(),
                uuid: this.profile.0,
            })
            .encode(),
        )?;
        Ok(this)
    }

    pub fn send(&mut self, body: &[u8]) -> Result<()> {
        let bytes = frame::encode(body, self.threshold)?;
        self.counters.sent.fetch_add(1, Ordering::Relaxed);
        self.counters
            .bytes_out
            .fetch_add(bytes.len() as i64, Ordering::Relaxed);
        self.outgoing.push(bytes);
        Ok(())
    }

    pub fn closed(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }

    pub fn end(&mut self, reason: String, from_server: bool) {
        if !self.closed() {
            self.phase = Phase::Closed;
            self.buffer.clear();
            self.events.push(Event::Disconnected {
                reason,
                from_server,
            });
        }
    }

    pub fn receive(&mut self, bytes: &[u8]) -> Result<()> {
        // Bound queued input even if a proxy sends an incomplete or malicious frame.
        if self.buffer.len() + bytes.len() > 32 * 1024 * 1024 {
            return Err(horizon_protocol::Error::new(
                "Server input exceeded the browser buffer limit",
            ));
        }
        self.counters
            .bytes_in
            .fetch_add(bytes.len() as i64, Ordering::Relaxed);
        self.buffer.extend_from_slice(bytes);
        while !self.closed() {
            let Some(frame) = frame::split(&mut self.buffer)? else {
                break;
            };
            let body = frame::decompress(frame, self.threshold)?;
            let mut reader = Reader::new(&body);
            let id = reader.varint()?;
            let payload = &body[reader.position()..];
            self.counters.received.fetch_add(1, Ordering::Relaxed);
            match self.phase {
                Phase::Login => self.login(id, payload)?,
                Phase::Configuration => self.configure(id, payload)?,
                Phase::Play => self.play(id, payload)?,
                Phase::Closed => break,
            }
            if self.events.len() > 4096 {
                return Err(horizon_protocol::Error::new(
                    "The browser fell too far behind the server; reconnect to continue",
                ));
            }
        }
        Ok(())
    }

    fn login(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        match login::Clientbound::decode(id, payload)? {
            login::Clientbound::Compression(p) => {
                self.threshold = usize::try_from(p.threshold).ok()
            }
            login::Clientbound::LoginFinished(p) => {
                self.profile = (p.profile.uuid, p.profile.name);
                self.send(&login::Serverbound::LoginAcknowledged(Empty).encode())?;
                self.begin_configuration()?;
            }
            login::Clientbound::Disconnect(p) => {
                let reason = serde_json::from_str(&p.reason_json)
                    .map(|v| json_text(&v, &self.lang))
                    .unwrap_or(p.reason_json);
                self.end(reason, true);
            }
            login::Clientbound::Hello(_) => self.end(NOT_SIGNED_IN.into(), true),
            login::Clientbound::CustomQuery(q) => self.send(
                &login::Serverbound::CustomQueryAnswer(login::CustomQueryAnswer {
                    transaction: q.transaction,
                    payload: None,
                })
                .encode(),
            )?,
            login::Clientbound::CookieRequest(q) => self.send(
                &login::Serverbound::CookieResponse(login::CookieResponse {
                    key: q.key,
                    payload: None,
                })
                .encode(),
            )?,
            _ => {}
        }
        Ok(())
    }

    fn begin_configuration(&mut self) -> Result<()> {
        self.phase = Phase::Configuration;
        self.registries = Dynamic::default();
        self.events.push(Event::Status("Joining world...".into()));
        self.send(
            &cfg::Serverbound::ClientInformation(cfg::ClientInformation::new(self.view_distance))
                .encode(),
        )?;
        self.send(
            &cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                channel: "minecraft:brand".into(),
                data: brand(),
            })
            .encode(),
        )
    }

    fn brand(&mut self, p: &cfg::CustomPayload) {
        if p.channel == "minecraft:brand"
            && let Ok(value) = Reader::new(&p.data).string(32767)
        {
            self.events.push(Event::Brand(value));
        }
    }

    fn configure(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        let reply = match cfg::Clientbound::decode(id, payload)? {
            cfg::Clientbound::KeepAlive(k) => Some(cfg::Serverbound::KeepAlive(k)),
            cfg::Clientbound::Ping(p) => Some(cfg::Serverbound::Pong(p)),
            cfg::Clientbound::CustomPayload(p) => {
                self.brand(&p);
                None
            }
            cfg::Clientbound::SelectKnownPacks(_) => {
                Some(cfg::Serverbound::SelectKnownPacks(cfg::SelectKnownPacks {
                    packs: vec![],
                }))
            }
            cfg::Clientbound::RegistryData(r) => {
                self.registries.add(r);
                None
            }
            cfg::Clientbound::CodeOfConduct(_) => {
                Some(cfg::Serverbound::AcceptCodeOfConduct(Empty))
            }
            cfg::Clientbound::ResourcePackPush(p) => {
                Some(cfg::Serverbound::ResourcePack(cfg::ResourcePackResponse {
                    id: p.id,
                    action: 1,
                }))
            }
            cfg::Clientbound::CookieRequest(q) => {
                Some(cfg::Serverbound::CookieResponse(cfg::CookieResponse {
                    key: q.key,
                    payload: None,
                }))
            }
            cfg::Clientbound::Disconnect(d) => {
                self.end(plain(&d.reason, &self.lang), true);
                None
            }
            cfg::Clientbound::FinishConfiguration(_) => {
                self.phase = Phase::Play;
                let registries = Arc::new(std::mem::take(&mut self.registries));
                self.events.push(if self.reconfiguring {
                    Event::Reconfigured(registries)
                } else {
                    Event::Joined {
                        registries,
                        uuid: self.profile.0,
                        name: self.profile.1.clone(),
                    }
                });
                Some(cfg::Serverbound::FinishConfiguration(Empty))
            }
            _ => None,
        };
        if let Some(reply) = reply {
            self.send(&reply.encode())?;
        }
        Ok(())
    }

    fn play(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        let packet = match play::Clientbound::decode(id, payload) {
            Ok(packet) => packet,
            Err(error) => {
                self.events
                    .push(Event::DecodeError(format!("packet {id}: {error}")));
                return Ok(());
            }
        };
        match packet {
            play::Clientbound::KeepAlive(k) => {
                self.send(&play::Serverbound::KeepAlive(k).encode())?
            }
            play::Clientbound::BundleDelimiter(_) => {}
            play::Clientbound::ResourcePackPush(p) => self.send(
                &play::Serverbound::ResourcePack(cfg::ResourcePackResponse {
                    id: p.id,
                    action: 1,
                })
                .encode(),
            )?,
            play::Clientbound::StartConfiguration(_) => {
                self.send(&play::Serverbound::ConfigurationAcknowledged(Empty).encode())?;
                self.reconfiguring = true;
                self.begin_configuration()?;
            }
            play::Clientbound::Disconnect(d) => self.end(plain(&d.reason, &self.lang), true),
            play::Clientbound::CustomPayload(p) if p.channel == "minecraft:brand" => self.brand(&p),
            packet => self.events.push(Event::Packet(Box::new(packet))),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Protocol {
        Protocol::new(
            "localhost",
            25565,
            "BrowserOne",
            3,
            Arc::new(Language::new()),
        )
        .unwrap()
    }

    fn login_finished() -> Vec<u8> {
        login::Clientbound::LoginFinished(login::LoginFinished {
            profile: login::GameProfile {
                uuid: offline_uuid("BrowserOne"),
                name: "BrowserOne".into(),
                properties: vec![],
            },
            session: 1,
        })
        .encode()
    }

    fn decode(bytes: &mut Vec<Vec<u8>>, threshold: Option<usize>) -> Vec<(i32, Vec<u8>)> {
        std::mem::take(bytes)
            .into_iter()
            .map(|mut b| {
                let frame = frame::split(&mut b).unwrap().unwrap();
                let body = frame::decompress(frame, threshold).unwrap();
                let mut r = Reader::new(&body);
                let id = r.varint().unwrap();
                (id, body[r.position()..].to_vec())
            })
            .collect()
    }

    #[test]
    fn fragmented_tcp_stream_changes_compression_and_enters_play() {
        let mut c = client();
        let initial = decode(&mut c.outgoing, None);
        let hello = login::Serverbound::decode(initial[1].0, &initial[1].1).unwrap();
        assert!(matches!(hello, login::Serverbound::Hello(p) if p.name == "BrowserOne"));
        let mut stream = frame::encode(
            &login::Clientbound::Compression(login::Compression { threshold: 0 }).encode(),
            None,
        )
        .unwrap();
        for body in [
            login_finished(),
            cfg::Clientbound::SelectKnownPacks(cfg::SelectKnownPacks { packs: vec![] }).encode(),
            cfg::Clientbound::FinishConfiguration(Empty).encode(),
        ] {
            stream.extend(frame::encode(&body, Some(0)).unwrap());
        }
        // Deliberately split every VarInt and zlib header across separate WebSocket messages.
        for byte in stream {
            c.receive(&[byte]).unwrap();
        }
        assert!(
            c.events
                .iter()
                .any(|e| matches!(e, Event::Joined { name, .. } if name == "BrowserOne"))
        );
        let replies = decode(&mut c.outgoing, Some(0));
        assert_eq!(replies.len(), 5); // Login ack, info, brand, no known packs, finish.
        assert!(
            matches!(cfg::Serverbound::decode(replies[3].0, &replies[3].1).unwrap(),
            cfg::Serverbound::SelectKnownPacks(p) if p.packs.is_empty())
        );
        assert!(matches!(c.phase, Phase::Play));
    }

    #[test]
    fn keepalive_is_immediate_but_play_ping_stays_in_game_order_and_reconfiguration_works() {
        let mut c = client();
        for body in [
            login_finished(),
            cfg::Clientbound::FinishConfiguration(Empty).encode(),
        ] {
            c.receive(&frame::encode(&body, None).unwrap()).unwrap();
        }
        c.events.clear();
        c.outgoing.clear();
        for body in [
            play::Clientbound::KeepAlive(cfg::KeepAlive { id: 42 }).encode(),
            play::Clientbound::Ping(cfg::Ping { id: 7 }).encode(),
        ] {
            c.receive(&frame::encode(&body, None).unwrap()).unwrap();
        }
        let replies = decode(&mut c.outgoing, None);
        assert_eq!(replies.len(), 1);
        assert!(
            matches!(play::Serverbound::decode(replies[0].0, &replies[0].1).unwrap(), play::Serverbound::KeepAlive(k) if k.id == 42)
        );
        assert!(
            matches!(&c.events[0], Event::Packet(p) if matches!(p.as_ref(), play::Clientbound::Ping(k) if k.id == 7))
        );
        c.receive(
            &frame::encode(&play::Clientbound::StartConfiguration(Empty).encode(), None).unwrap(),
        )
        .unwrap();
        c.receive(
            &frame::encode(&cfg::Clientbound::FinishConfiguration(Empty).encode(), None).unwrap(),
        )
        .unwrap();
        assert!(c.events.iter().any(|e| matches!(e, Event::Reconfigured(_))));
    }

    #[test]
    fn login_refusals_are_terminal_and_malformed_frames_are_rejected() {
        let mut c = client();
        let body = login::Clientbound::Disconnect(login::Disconnect {
            reason_json: "{\"text\":\"Name already in use\"}".into(),
        })
        .encode();
        c.receive(&frame::encode(&body, None).unwrap()).unwrap();
        c.end("duplicate".into(), false);
        assert!(c.closed());
        assert_eq!(c.events.iter().filter(|e| matches!(e, Event::Disconnected { reason, from_server: true } if reason == "Name already in use")).count(), 1);
        let mut c = client();
        let hello = login::Clientbound::Hello(login::Hello {
            server_id: String::new(),
            public_key: vec![],
            challenge: vec![],
            should_authenticate: true,
        });
        c.receive(&frame::encode(&hello.encode(), None).unwrap())
            .unwrap();
        assert!(c.events.iter().any(|e| matches!(e, Event::Disconnected { reason, from_server: true } if reason == NOT_SIGNED_IN)));
        assert!(client().receive(&[0xff, 0xff, 0xff]).is_err());
    }
}
