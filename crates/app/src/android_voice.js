// Galène voice channel client for the Android WebView.
//
// Rust owns discovery and the VoicePort state; this script owns the websocket
// signalling and the browser's WebRTC media stack. Messages are exchanged with
// Dioxus' eval bridge: {type:"result"} answers one command, {type:"event"}
// carries participant/connection changes.

const send = (value) => dioxus.send(value);

let ws = null;
let localStream = null;
let selfId = "";
let username = "jabu";
let group = "public";
let iceServers = [];
let muted = false;
let joined = false;
let up = null;
let joinedResolve = null;
let joinedReject = null;

const peers = new Map();
const audios = new Map();
const people = new Map();

// Visible from the WebView devtools while debugging a call.
globalThis.__jabuVoiceDebug = () => ({
  ws: ws ? ws.readyState : null,
  up: up
    ? {
        connection: up.pc.connectionState,
        ice: up.pc.iceConnectionState,
        signaling: up.pc.signalingState,
      }
    : null,
  peers: [...peers.values()].map((peer) => ({
    connection: peer.pc.connectionState,
    ice: peer.pc.iceConnectionState,
  })),
  participants: [...people.values()],
});

function randomId() {
  return Math.random().toString(16).slice(2) + Date.now().toString(16);
}

function sendParticipants() {
  if (selfId && ![...people.values()].some((p) => p.is_self)) {
    people.set(selfId, {
      id: selfId,
      name: username,
      muted,
      speaking: false,
      screen_sharing: false,
      is_self: true,
      volume: 1,
    });
  }
  send({
    type: "event",
    event: "participants",
    participants: [...people.values()],
  });
}

function sendIce(id, candidate) {
  if (!candidate || !ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({
    type: "ice",
    source: selfId,
    id,
    candidate: {
      candidate: candidate.candidate,
      sdpMid: candidate.sdpMid,
      sdpMLineIndex: candidate.sdpMLineIndex,
    },
  }));
}

function updateUser(message) {
  const id = message.id || "";
  if (!id) return;
  if (message.kind === "delete") {
    people.delete(id);
    sendParticipants();
    return;
  }
  if (message.kind !== "add" && message.kind !== "change") return;
  const streams = message.streams || {};
  const hasStream = Object.keys(streams).length > 0;
  const old = people.get(id) || {};
  people.set(id, {
    id,
    name: message.username || old.name || "participant",
    muted: !hasStream,
    speaking: false,
    screen_sharing: false,
    is_self: id === selfId,
    volume: old.volume || 1,
  });
  sendParticipants();
}

async function answerOffer(message) {
  const id = message.id;
  if (!id || !message.sdp) return;
  const pc = new RTCPeerConnection({ iceServers });
  const audio = new Audio();
  audio.autoplay = true;
  audio.playsInline = true;
  audio.volume = 1;
  pc.ontrack = (event) => {
    const stream = event.streams && event.streams[0];
    if (stream) {
      audio.srcObject = stream;
      audio.play().catch(() => {});
    }
  };
  pc.onicecandidate = (event) => sendIce(id, event.candidate);
  pc.onconnectionstatechange = () => {
    send({ type: "event", event: "media", state: pc.connectionState });
    if (pc.connectionState === "failed") {
      send({ type: "event", event: "error", message: "remote voice media failed" });
    }
  };
  await pc.setRemoteDescription({ type: "offer", sdp: message.sdp });
  const answer = await pc.createAnswer();
  await pc.setLocalDescription(answer);
  ws.send(JSON.stringify({ type: "answer", id, sdp: answer.sdp }));
  peers.set(id, { pc, audio, source: message.source || "" });
  if (message.source) {
    audios.set(message.source, audio);
    const participant = people.get(message.source);
    if (participant) {
      participant.muted = false;
      sendParticipants();
    }
  }
}

async function handleMessage(message) {
  switch (message.type) {
    case "ping":
      ws.send(JSON.stringify({ type: "pong" }));
      break;
    case "joined":
      if (message.kind === "join") {
        joined = true;
        const config = message.rtcConfiguration || {};
        const servers = Array.isArray(config.iceServers) ? config.iceServers : [];
        if (joinedResolve) {
          joinedResolve(servers);
          joinedResolve = null;
          joinedReject = null;
        }
      } else if (message.kind === "leave" || message.kind === "fail") {
        joined = false;
        const reason = message.value || "could not join the voice channel";
        if (joinedReject) {
          joinedReject(new Error(reason));
          joinedResolve = null;
          joinedReject = null;
        }
        send({ type: "event", event: "error", message: reason });
      }
      break;
    case "user":
      updateUser(message);
      break;
    case "offer":
      await answerOffer(message);
      break;
    case "answer": {
      const pc = up && message.id === up.id ? up.pc : peers.get(message.id)?.pc;
      if (pc && message.sdp) {
        await pc.setRemoteDescription({ type: "answer", sdp: message.sdp });
      }
      break;
    }
    case "ice": {
      const pc = up && message.id === up.id ? up.pc : peers.get(message.id)?.pc;
      if (pc && message.candidate) {
        await pc.addIceCandidate(message.candidate).catch(() => {});
      }
      break;
    }
    case "close":
    case "abort": {
      const peer = peers.get(message.id);
      if (peer) {
        peer.pc.close();
        if (peer.audio) {
          peer.audio.pause();
          peer.audio.srcObject = null;
        }
        peers.delete(message.id);
      }
      break;
    }
    case "error":
      send({
        type: "event",
        event: "error",
        message: message.value || "Galène error",
      });
      break;
    default:
      break;
  }
}

async function leaveInternal() {
  if (ws) {
    try {
      if (joined) {
        ws.send(JSON.stringify({ type: "join", kind: "leave", group }));
      }
      ws.close();
    } catch (_) {}
    ws = null;
  }
  joined = false;
  for (const peer of peers.values()) {
    peer.pc.close();
    if (peer.audio) {
      peer.audio.pause();
      peer.audio.srcObject = null;
    }
  }
  peers.clear();
  audios.clear();
  people.clear();
  if (localStream) {
    for (const track of localStream.getTracks()) track.stop();
    localStream = null;
  }
  up = null;
}

async function joinVoice(command) {
  await leaveInternal();
  selfId = command.self_id || randomId();
  username = command.username || "jabu";
  group = command.group || "public";
  muted = Boolean(command.muted);
  iceServers = Array.isArray(command.ice_servers) ? command.ice_servers : [];

  ws = new WebSocket(command.endpoint);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("voice websocket failed"));
  });
  ws.onmessage = (event) => {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch (error) {
      send({ type: "event", event: "error", message: String(error) });
      return;
    }
    handleMessage(message).catch((error) => {
      send({
        type: "event",
        event: "error",
        message: String(error && error.message ? error.message : error),
      });
    });
  };
  ws.onclose = () => {
    if (joined) {
      joined = false;
      send({ type: "event", event: "left" });
    }
  };

  const joinedPromise = new Promise((resolve, reject) => {
    joinedResolve = resolve;
    joinedReject = reject;
    setTimeout(() => {
      if (joinedReject === reject) {
        joinedReject = null;
        joinedResolve = null;
        reject(new Error("timed out joining the voice channel"));
      }
    }, 15000);
  });

  ws.send(JSON.stringify({
    type: "handshake",
    version: ["2", "1"],
    id: selfId,
  }));
  ws.send(JSON.stringify({
    type: "join",
    kind: "join",
    group,
    username,
    password: command.password || "",
  }));

  const servers = await joinedPromise;
  if (Array.isArray(servers) && servers.length > 0) {
    iceServers = servers;
  }

  localStream = await navigator.mediaDevices.getUserMedia({
    audio: {
      echoCancellation: true,
      noiseSuppression: true,
      autoGainControl: true,
    },
    video: false,
  });
  for (const track of localStream.getAudioTracks()) {
    track.enabled = !muted;
  }

  const streamId = selfId + "-up-" + randomId();
  const pc = new RTCPeerConnection({ iceServers });
  up = { pc, id: streamId };
  for (const track of localStream.getTracks()) pc.addTrack(track, localStream);
  // Galène streams are unidirectional: the offerer only sends.
  for (const transceiver of pc.getTransceivers()) transceiver.direction = "sendonly";
  pc.onicecandidate = (event) => sendIce(streamId, event.candidate);
  pc.onconnectionstatechange = () => {
    send({ type: "event", event: "media", state: pc.connectionState });
    if (pc.connectionState === "failed") {
      send({ type: "event", event: "error", message: "local voice media failed" });
    }
  };
  ws.send(JSON.stringify({ type: "request", request: { "": ["audio"] } }));
  const offer = await pc.createOffer({ offerToReceiveAudio: false, offerToReceiveVideo: false });
  await pc.setLocalDescription(offer);
  ws.send(JSON.stringify({
    type: "offer",
    source: selfId,
    username,
    kind: "",
    id: streamId,
    label: "camera",
    sdp: offer.sdp,
  }));

  people.set(selfId, {
    id: selfId,
    name: username,
    muted,
    speaking: false,
    screen_sharing: false,
    is_self: true,
    volume: 1,
  });
  sendParticipants();
  return {};
}

async function setMuted(command) {
  muted = Boolean(command.muted);
  if (localStream) {
    for (const track of localStream.getAudioTracks()) track.enabled = !muted;
  }
  const self = people.get(selfId);
  if (self) self.muted = muted;
  sendParticipants();
  return {};
}

async function setVolume(command) {
  const volume = Math.max(0, Math.min(2, Number(command.volume) || 0));
  const audio = audios.get(command.id);
  if (audio) audio.volume = volume;
  const participant = people.get(command.id);
  if (participant) participant.volume = volume;
  sendParticipants();
  return {};
}

async function leaveVoice() {
  await leaveInternal();
  send({ type: "event", event: "left" });
  return {};
}

// Keep this loop at the top level. Dioxus only keeps the eval channel alive
// while the evaluated script is still awaiting; an un-awaited async IIFE would
// return immediately and `dioxus` would be dropped.
while (true) {
  let command;
  try {
    command = await dioxus.recv();
  } catch (_) {
    break;
  }
  if (!command || !command.cmd) continue;
  try {
    let result = {};
    if (command.cmd === "join") result = await joinVoice(command);
    else if (command.cmd === "leave") result = await leaveVoice();
    else if (command.cmd === "mute") result = await setMuted(command);
    else if (command.cmd === "volume") result = await setVolume(command);
    send({ type: "result", req: command.req, ok: true, ...result });
  } catch (error) {
    send({
      type: "result",
      req: command.req,
      ok: false,
      error: String(error && error.message ? error.message : error),
    });
  }
}
