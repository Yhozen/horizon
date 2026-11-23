//! The relay's connections: it accepts open players, logs each into the
//! upstream server as an offline-mode player of the same name, then runs one
//! thread per direction through [`crate::translate`].
use crate::{tables::Tables, translate};
use horizon_protocol::{
    PROTOCOL_VERSION,
    connection::Connection,
    crypto::offline_uuid,
    packets::{handshake, login, status},
};
use std::{
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

/// Where the relay listens and what it joins.
#[derive(Clone, Debug)]
pub struct Config {
    pub listen: String,
    /// The upstream server, `HOST:PORT`.
    pub upstream: String,
    /// At most this many players at once.
    pub max_connections: usize,
    /// Log every dropped packet kind when a connection ends.
    pub debug: bool,
}

/// Split `HOST:PORT` (the port defaulting to 25565).
pub fn host_port(address: &str) -> Result<(String, u16), String> {
    match address.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => Ok((
            host.trim_matches(['[', ']']).to_owned(),
            port.parse()
                .map_err(|_| format!("invalid port in {address}"))?,
        )),
        _ if !address.is_empty() => Ok((address.to_owned(), 25565)),
        _ => Err("an empty address".into()),
    }
}

pub fn serve(config: Config, tables: Arc<Tables>) -> Result<(), String> {
    let listener =
        TcpListener::bind(&config.listen).map_err(|e| format!("{}: {e}", config.listen))?;
    eprintln!(
        "horizon-open-relay: listening on {} for {} (brand {})",
        listener.local_addr().map_err(|e| e.to_string())?,
        config.upstream,
        tables.brand
    );
    serve_on(listener, config, tables)
}

/// Accept on an existing listener (tests bind port 0 first).
pub fn serve_on(listener: TcpListener, config: Config, tables: Arc<Tables>) -> Result<(), String> {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if active.load(Ordering::SeqCst) >= config.max_connections {
            drop(stream);
            continue;
        }
        active.fetch_add(1, Ordering::SeqCst);
        let (config, tables, active) = (config.clone(), tables.clone(), active.clone());
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        let spawned = std::thread::Builder::new()
            .name("relay-conn".into())
            .spawn(move || {
                if let Err(e) = connection(stream, &config, &tables) {
                    eprintln!("horizon-open-relay: {peer}: {e}");
                }
                active.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            eprintln!("horizon-open-relay: could not start a connection thread");
        }
    }
    Ok(())
}

fn err(e: horizon_protocol::Error) -> String {
    e.to_string()
}

/// A login-phase disconnect with a plain reason.
fn refuse(client: &mut Connection, reason: &str) -> Result<(), String> {
    let json = serde_json::json!({ "text": reason }).to_string();
    client
        .send(&login::Clientbound::Disconnect(login::Disconnect { reason_json: json }).encode())
        .map_err(err)
}

fn connection(stream: TcpStream, config: &Config, tables: &Arc<Tables>) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let client_socket = stream.try_clone().map_err(|e| e.to_string())?;
    let mut client = Connection::from_stream(stream).map_err(err)?;
    let (id, payload) = client.recv().map_err(err)?;
    let handshake::Serverbound::Intention(intent) =
        handshake::Serverbound::decode(id, &payload).map_err(err)?
    else {
        return Err("expected a handshake".into());
    };
    let (host, port) = host_port(&config.upstream)?;
    // The upstream socket is kept to close it from the other direction's thread.
    let connect = || -> Result<(Connection, TcpStream), String> {
        use std::net::ToSocketAddrs;
        let mut last = format!("unknown host {host}");
        for addr in (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| format!("{host}: {e}"))?
        {
            match TcpStream::connect_timeout(&addr, Duration::from_secs(10)) {
                Ok(s) => {
                    let keep = s.try_clone().map_err(|e| e.to_string())?;
                    return Ok((Connection::from_stream(s).map_err(err)?, keep));
                }
                Err(e) => last = format!("{addr}: {e}"),
            }
        }
        Err(last)
    };
    let intention = |intent: i32| {
        handshake::Serverbound::Intention(handshake::Intention {
            protocol: PROTOCOL_VERSION,
            host: host.clone(),
            port,
            intent,
        })
        .encode()
    };
    if intent.intent == handshake::STATUS {
        let (mut upstream, _) = connect()?;
        upstream.send(&intention(handshake::STATUS)).map_err(err)?;
        return status_proxy(&mut client, &mut upstream);
    }
    if intent.intent != handshake::LOGIN {
        return Err(format!("unsupported intent {}", intent.intent));
    }
    let (id, payload) = client.recv().map_err(err)?;
    let login::Serverbound::Hello(hello) = login::Serverbound::decode(id, &payload).map_err(err)?
    else {
        return Err("expected a login hello".into());
    };
    if intent.protocol != PROTOCOL_VERSION {
        return refuse(
            &mut client,
            "This relay serves Minecraft 26.3 (protocol 777) clients",
        );
    }
    let (mut upstream, upstream_socket) = match connect() {
        Ok(c) => c,
        Err(e) => {
            refuse(
                &mut client,
                "The Java server is unavailable; please try again",
            )?;
            return Err(e);
        }
    };
    upstream.send(&intention(handshake::LOGIN)).map_err(err)?;
    upstream
        .send(
            &login::Serverbound::Hello(login::ServerHello {
                name: hello.name.clone(),
                uuid: offline_uuid(&hello.name),
            })
            .encode(),
        )
        .map_err(err)?;
    loop {
        let (id, payload) = upstream.recv().map_err(err)?;
        match login::Clientbound::decode(id, &payload).map_err(err)? {
            login::Clientbound::Compression(p) => {
                upstream.set_compression(p.threshold);
                client
                    .send(&login::Clientbound::Compression(p.clone()).encode())
                    .map_err(err)?;
                client.set_compression(p.threshold);
            }
            login::Clientbound::Hello(_) => {
                return refuse(
                    &mut client,
                    "This Java server is in online mode: the relay joins offline-mode servers only",
                );
            }
            login::Clientbound::CustomQuery(q) => upstream
                .send(
                    &login::Serverbound::CustomQueryAnswer(login::CustomQueryAnswer {
                        transaction: q.transaction,
                        payload: None,
                    })
                    .encode(),
                )
                .map_err(err)?,
            login::Clientbound::CookieRequest(q) => upstream
                .send(
                    &login::Serverbound::CookieResponse(login::CookieResponse {
                        key: q.key,
                        payload: None,
                    })
                    .encode(),
                )
                .map_err(err)?,
            disconnect @ login::Clientbound::Disconnect(_) => {
                return client.send(&disconnect.encode()).map_err(err);
            }
            finished @ login::Clientbound::LoginFinished(_) => {
                client.send(&finished.encode()).map_err(err)?;
                break;
            }
            login::Clientbound::Unknown(..) => {}
        }
    }
    // The client acknowledges the login; only then does the server configure.
    loop {
        let (id, payload) = client.recv().map_err(err)?;
        let ack = id == horizon_protocol::ids::login::serverbound::LOGIN_ACKNOWLEDGED;
        upstream.send(&translate::body(id, &payload)).map_err(err)?;
        if ack {
            break;
        }
    }
    eprintln!("horizon-open-relay: {} joined", hello.name);
    let _ = client_socket.set_read_timeout(None);
    let (mut client_reader, mut client_writer) = client.split();
    let (mut upstream_reader, mut upstream_writer) = upstream.split();
    let close = {
        let a = Arc::new(client_socket.try_clone().map_err(|e| e.to_string())?);
        let b = Arc::new(upstream_socket);
        move || {
            let _ = a.shutdown(Shutdown::Both);
            let _ = b.shutdown(Shutdown::Both);
        }
    };
    let close_down = close.clone();
    let down_tables = tables.clone();
    // Recipe display ids the server's book packets teach, for the player's picks.
    let displays = crate::recipes::Displays::default();
    let down_displays = displays.clone();
    let debug = config.debug;
    let name = hello.name.clone();
    // Server → player.
    let down = std::thread::Builder::new()
        .name("relay-down".into())
        .spawn(move || {
            let mut t = translate::Clientbound::new(&down_tables).with_displays(down_displays);
            let result = (|| -> Result<(), String> {
                loop {
                    let (id, payload) = upstream_reader.recv().map_err(err)?;
                    if let Some(out) = t.translate(id, &payload) {
                        client_writer.send(&out).map_err(err)?;
                    }
                }
            })();
            if debug {
                eprintln!("horizon-open-relay: {name}: {:?}", t.counts);
            }
            close_down();
            result
        })
        .map_err(|e| e.to_string())?;
    // Player → server.
    let mut t = translate::Serverbound::new(tables).with_displays(displays);
    let up = (|| -> Result<(), String> {
        loop {
            let (id, payload) = client_reader.recv().map_err(err)?;
            if let Some(out) = t.translate(id, &payload) {
                upstream_writer.send(&out).map_err(err)?;
            }
        }
    })();
    close();
    let down = down.join().unwrap_or(Ok(()));
    eprintln!(
        "horizon-open-relay: {} left ({})",
        hello.name,
        down.err().or(up.err()).unwrap_or_default()
    );
    Ok(())
}

/// Server list pings pass through as they are (status has no registry ids).
fn status_proxy(client: &mut Connection, upstream: &mut Connection) -> Result<(), String> {
    loop {
        let (id, payload) = match client.recv() {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        upstream.send(&translate::body(id, &payload)).map_err(err)?;
        let (id, payload) = upstream.recv().map_err(err)?;
        let reply = status::Clientbound::decode(id, &payload).map_err(err)?;
        client.send(&reply.encode()).map_err(err)?;
        if matches!(reply, status::Clientbound::PongResponse(_)) {
            return Ok(());
        }
    }
}
