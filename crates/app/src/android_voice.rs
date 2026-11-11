//! Android group voice over Galène.
//!
//! The Android build has no native WebRTC media backend. Galène already speaks
//! WebRTC, so the WebView's Chromium stack is the media engine and this module
//! only bridges the `VoicePort` contract to a small JavaScript client.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;
use xmpp_core::voice::{VoiceChannel, VoiceEvent, VoiceJoin, VoiceParticipant, VoicePort};
use xmpp_core::XmppClient;

const JS: &str = include_str!("android_voice.js");

#[derive(Clone)]
struct Config {
    server: String,
    group: String,
    password: String,
}

struct State {
    channel: RwLock<Option<VoiceChannel>>,
    joined: AtomicBool,
    muted: AtomicBool,
    participants: RwLock<Vec<VoiceParticipant>>,
    events: broadcast::Sender<VoiceEvent>,
}

enum Command {
    Join {
        request: VoiceJoin,
        endpoint: String,
        ice_servers: Value,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Leave {
        reply: oneshot::Sender<Result<(), String>>,
    },
    Mute {
        muted: bool,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Volume {
        id: String,
        volume: f64,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

pub struct GaleneWebVoicePort {
    config: RwLock<Config>,
    state: Arc<State>,
    commands: mpsc::UnboundedSender<Command>,
}

static PORT: OnceLock<Arc<GaleneWebVoicePort>> = OnceLock::new();

/// Install the Android voice port on a client. The JavaScript bridge is
/// created once and shared by every account session.
pub fn install(client: &XmppClient, server: String, group: String, password: String) {
    let port = PORT.get_or_init(|| {
        let (commands, command_rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(64);
        let state = Arc::new(State {
            channel: RwLock::new(None),
            joined: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            participants: RwLock::new(Vec::new()),
            events,
        });
        let port = Arc::new(GaleneWebVoicePort {
            config: RwLock::new(Config {
                server: server.clone(),
                group: group.clone(),
                password: password.clone(),
            }),
            state,
            commands,
        });
        dioxus::prelude::spawn(bridge(port.clone(), command_rx));
        port
    });
    *port.config.write() = Config {
        server,
        group,
        password,
    };
    client.voice.set_port(port.clone());
}

async fn bridge(port: Arc<GaleneWebVoicePort>, mut command_rx: mpsc::UnboundedReceiver<Command>) {
    let mut eval = dioxus::document::eval(JS);
    let mut pending: HashMap<String, oneshot::Sender<Result<(), String>>> = HashMap::new();

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                let request = Uuid::new_v4().to_string();
                let payload = match command {
                    Command::Join {
                        request: join,
                        endpoint,
                        ice_servers,
                        reply,
                    } => {
                        pending.insert(request.clone(), reply);
                        json!({
                            "cmd": "join",
                            "req": request,
                            "endpoint": endpoint,
                            "group": port.config.read().group,
                            "username": join.username,
                            "password": port.config.read().password,
                            "muted": join.muted,
                            "ice_servers": ice_servers,
                            "self_id": Uuid::new_v4().simple().to_string(),
                        })
                    }
                    Command::Leave { reply } => {
                        pending.insert(request.clone(), reply);
                        json!({ "cmd": "leave", "req": request })
                    }
                    Command::Mute { muted, reply } => {
                        pending.insert(request.clone(), reply);
                        json!({ "cmd": "mute", "req": request, "muted": muted })
                    }
                    Command::Volume { id, volume, reply } => {
                        pending.insert(request.clone(), reply);
                        json!({
                            "cmd": "volume",
                            "req": request,
                            "id": id,
                            "volume": volume,
                        })
                    }
                };
                if eval.send(payload).is_err() {
                    if let Some(reply) = pending.remove(&request) {
                        let _ = reply.send(Err("voice bridge stopped".to_string()));
                    }
                }
            }
            message = eval.recv::<Value>() => {
                match message {
                    Ok(message) => handle_bridge_message(&port, &mut pending, message),
                    Err(_) => break,
                }
            }
        }
    }

    for (_, reply) in pending {
        let _ = reply.send(Err("voice bridge stopped".to_string()));
    }
}

fn handle_bridge_message(
    port: &Arc<GaleneWebVoicePort>,
    pending: &mut HashMap<String, oneshot::Sender<Result<(), String>>>,
    message: Value,
) {
    match message
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "result" => {
            let Some(request) = message.get("req").and_then(Value::as_str) else {
                return;
            };
            let Some(reply) = pending.remove(request) else {
                return;
            };
            if message.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                let _ = reply.send(Ok(()));
            } else {
                let error = message
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("voice command failed")
                    .to_string();
                let _ = reply.send(Err(error));
            }
        }
        "event" => match message
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "participants" => {
                let participants = message
                    .get("participants")
                    .map(parse_participants)
                    .unwrap_or_default();
                *port.state.participants.write() = participants.clone();
                let _ = port
                    .state
                    .events
                    .send(VoiceEvent::Participants(participants));
            }
            "joined" => {
                port.state.joined.store(true, Ordering::Relaxed);
                let _ = port.state.events.send(VoiceEvent::Joined);
            }
            "left" => {
                port.state.joined.store(false, Ordering::Relaxed);
                *port.state.channel.write() = None;
                port.state.participants.write().clear();
                let _ = port.state.events.send(VoiceEvent::Left);
            }
            "error" => {
                let error = message
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("voice error")
                    .to_string();
                tracing::warn!("Galene voice error: {error}");
                let _ = port.state.events.send(VoiceEvent::Error(error));
            }
            "media" => {
                if let Some(state) = message.get("state").and_then(Value::as_str) {
                    tracing::info!("Galene voice media state: {state}");
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn parse_participants(value: &Value) -> Vec<VoiceParticipant> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(VoiceParticipant {
                        id: item.get("id")?.as_str()?.to_string(),
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("participant")
                            .to_string(),
                        muted: item.get("muted").and_then(Value::as_bool).unwrap_or(false),
                        speaking: item
                            .get("speaking")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        screen_sharing: item
                            .get("screen_sharing")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        is_self: item
                            .get("is_self")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        volume: item.get("volume").and_then(Value::as_f64).unwrap_or(1.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn group_status(config: &Config) -> anyhow::Result<(String, Value)> {
    let status_url = format!(
        "{}/{}/.status",
        config.server.trim_end_matches('/'),
        config.group.trim_matches('/')
    );
    let status: Value = reqwest::Client::new()
        .get(&status_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let base = reqwest::Url::parse(&config.server)?;
    let host = base
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Galène server has no host"))?;
    let secure = base.scheme() == "https";
    let mut endpoint = status
        .get("endpoint")
        .and_then(Value::as_str)
        .and_then(|value| reqwest::Url::parse(value).ok())
        .unwrap_or_else(|| {
            reqwest::Url::parse(&format!("ws://{host}/ws")).expect("valid default endpoint")
        });
    if secure {
        let _ = endpoint.set_scheme("wss");
    }
    if endpoint
        .host_str()
        .is_some_and(|value| value == "127.0.0.1" || value == "localhost")
    {
        let _ = endpoint.set_host(Some(host));
    }
    let ice_servers = status
        .get("rtcConfiguration")
        .and_then(|value| value.get("iceServers"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    Ok((endpoint.to_string(), ice_servers))
}

async fn send_command(
    commands: &mpsc::UnboundedSender<Command>,
    command: impl FnOnce(oneshot::Sender<Result<(), String>>) -> Command,
) -> anyhow::Result<()> {
    let (reply, response) = oneshot::channel();
    commands
        .send(command(reply))
        .map_err(|_| anyhow::anyhow!("voice bridge is not running"))?;
    response
        .await
        .map_err(|_| anyhow::anyhow!("voice bridge is not running"))?
        .map_err(anyhow::Error::msg)
}

#[async_trait]
impl VoicePort for GaleneWebVoicePort {
    async fn join(&self, request: VoiceJoin) -> anyhow::Result<()> {
        let config = self.config.read().clone();
        let (endpoint, ice_servers) = group_status(&config).await?;
        send_command(&self.commands, |reply| Command::Join {
            request: request.clone(),
            endpoint,
            ice_servers,
            reply,
        })
        .await?;
        *self.state.channel.write() = Some(request.channel);
        self.state.joined.store(true, Ordering::Relaxed);
        self.state.muted.store(request.muted, Ordering::Relaxed);
        Ok(())
    }

    async fn leave(&self) -> anyhow::Result<()> {
        send_command(&self.commands, |reply| Command::Leave { reply }).await?;
        self.state.joined.store(false, Ordering::Relaxed);
        *self.state.channel.write() = None;
        self.state.participants.write().clear();
        Ok(())
    }

    async fn set_muted(&self, muted: bool) -> anyhow::Result<()> {
        send_command(&self.commands, |reply| Command::Mute { muted, reply }).await?;
        self.state.muted.store(muted, Ordering::Relaxed);
        Ok(())
    }

    async fn set_participant_volume(&self, id: &str, volume: f64) -> anyhow::Result<()> {
        let volume = volume.clamp(0.0, 2.0);
        send_command(&self.commands, |reply| Command::Volume {
            id: id.to_string(),
            volume,
            reply,
        })
        .await?;
        if let Some(participant) = self
            .state
            .participants
            .write()
            .iter_mut()
            .find(|participant| participant.id == id)
        {
            participant.volume = volume;
        }
        Ok(())
    }

    fn is_joined(&self) -> bool {
        self.state.joined.load(Ordering::Relaxed)
    }

    fn channel(&self) -> Option<VoiceChannel> {
        self.state.channel.read().clone()
    }

    fn participants(&self) -> Vec<VoiceParticipant> {
        self.state.participants.read().clone()
    }

    fn subscribe(&self) -> broadcast::Receiver<VoiceEvent> {
        self.state.events.subscribe()
    }
}
