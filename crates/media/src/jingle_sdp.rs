//! Conversion between Jingle (XEP-0167/0176/0320) and SDP.

use std::net::IpAddr;

use anyhow::{anyhow, Context, Result};
use minidom::Element;
use xmpp_parsers::hashes::Algo;
use xmpp_parsers::jingle::{
    Action, Content, ContentId, Creator, Description, Jingle, Reason, ReasonElement, Senders,
    SessionId,
};
use xmpp_parsers::jingle_dtls_srtp::{Fingerprint, Setup};
use xmpp_parsers::jingle_ice_udp::{Candidate, Transport as IceTransport, Type as CandidateType};
use xmpp_parsers::jingle_rtp::{Description as RtpDescription, Parameter, PayloadType, RtcpMux};

/// Build a Jingle session-initiate/session-accept from a local SDP offer/answer.
pub fn sdp_to_jingle(sdp: &str, action: Action, sid: &str) -> Result<String> {
    let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
        .map_err(|error| anyhow!("invalid SDP: {error}"))?;

    let mut jingle = Jingle::new(action, SessionId(sid.to_string()));

    for (index, media) in message.medias().enumerate() {
        let media_type = media.media().unwrap_or("audio").to_string();
        let mut rtp = RtpDescription::new(media_type.clone());
        let mut transport = IceTransport::new();
        let mut setup = Setup::Actpass;
        let mut fingerprint: Option<(Algo, String)> = None;
        let mut content_name = media_type.clone();
        let mut ssrc = None;

        for attribute in media.attributes() {
            let key = attribute.key();
            let value = attribute.value().unwrap_or_default();
            match key {
                "rtpmap" => {
                    if let Some(payload) = parse_rtpmap(value) {
                        rtp.payload_types.push(payload);
                    }
                }
                "fmtp" => {
                    if let Some((id, params)) = parse_fmtp(value) {
                        if let Some(payload) = rtp.payload_types.iter_mut().find(|p| p.id == id) {
                            payload.parameters.extend(params);
                        }
                    }
                }
                "rtcp-mux" => rtp.rtcp_mux = Some(RtcpMux),
                "ice-ufrag" => transport.ufrag = Some(value.to_string()),
                "ice-pwd" => transport.pwd = Some(value.to_string()),
                "fingerprint" => {
                    if let Some((algo, hash)) = value.split_once(' ') {
                        if let Ok(algo) = algo.parse::<Algo>() {
                            fingerprint = Some((algo, hash.to_string()));
                        }
                    }
                }
                "setup" => {
                    if let Ok(parsed) = value.parse::<Setup>() {
                        setup = parsed;
                    }
                }
                "mid" => content_name = value.to_string(),
                "candidate" => {
                    if let Ok(candidate) = parse_candidate(value) {
                        transport.candidates.push(candidate);
                    }
                }
                "ssrc" => {
                    if let Some(id) = value.split_whitespace().next() {
                        ssrc = Some(id.to_string());
                    }
                }
                _ => {}
            }
        }

        rtp.ssrc = ssrc;
        if let Some((algo, hash)) = fingerprint {
            if let Ok(fp) = Fingerprint::from_colon_separated_hex(setup, &String::from(algo), &hash)
            {
                transport.fingerprint = Some(fp);
            }
        }

        let mut content = Content::new(Creator::Initiator, ContentId(content_name));
        content.senders = Senders::Both;
        content.description = Some(Description::Rtp(rtp));
        content.transport = Some(xmpp_parsers::jingle::Transport::IceUdp(transport));
        let _ = index;
        jingle.contents.push(content);
    }

    Ok(element_to_string(Element::from(jingle)))
}

/// Build an SDP description from a remote Jingle session-initiate/accept.
pub fn jingle_to_sdp(jingle_xml: &str) -> Result<String> {
    let element: Element = jingle_xml.parse().context("invalid Jingle XML")?;
    let jingle = Jingle::try_from(element).map_err(|error| anyhow!("invalid Jingle: {error}"))?;

    let mut lines = vec![
        "v=0".to_string(),
        format!("o=- {} 2 IN IP4 127.0.0.1", jingle.sid.0),
        "s=-".to_string(),
        "t=0 0".to_string(),
    ];

    let mids: Vec<String> = jingle
        .contents
        .iter()
        .map(|content| content.name.0.clone())
        .collect();
    if mids.len() > 1 {
        lines.push(format!("a=group:BUNDLE {}", mids.join(" ")));
    }

    for content in &jingle.contents {
        let Some(Description::Rtp(rtp)) = &content.description else {
            continue;
        };
        let Some(xmpp_parsers::jingle::Transport::IceUdp(ice)) = &content.transport else {
            continue;
        };

        let formats = rtp
            .payload_types
            .iter()
            .map(|payload| payload.id.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!("m={} 9 UDP/TLS/RTP/SAVPF {}", rtp.media, formats));
        lines.push("c=IN IP4 0.0.0.0".to_string());
        lines.push("a=rtcp:9 IN IP4 0.0.0.0".to_string());
        if let Some(ufrag) = &ice.ufrag {
            lines.push(format!("a=ice-ufrag:{ufrag}"));
        }
        if let Some(pwd) = &ice.pwd {
            lines.push(format!("a=ice-pwd:{pwd}"));
        }
        if let Some(fingerprint) = &ice.fingerprint {
            lines.push(format!(
                "a=fingerprint:{} {}",
                String::from(fingerprint.hash.clone()),
                colon_hex(&fingerprint.value)
            ));
            lines.push(format!("a=setup:{}", fingerprint.setup));
        }
        lines.push(format!("a=mid:{}", content.name.0));
        lines.push("a=sendrecv".to_string());
        if rtp.rtcp_mux.is_some() {
            lines.push("a=rtcp-mux".to_string());
        }
        for payload in &rtp.payload_types {
            if let Some(name) = &payload.name {
                let clockrate = payload.clockrate.unwrap_or(48000);
                let channels = payload.channels.0;
                lines.push(format!(
                    "a=rtpmap:{} {}/{}/{}",
                    payload.id, name, clockrate, channels
                ));
            }
            if !payload.parameters.is_empty() {
                let params = payload
                    .parameters
                    .iter()
                    .map(|parameter| format!("{}={}", parameter.name, parameter.value))
                    .collect::<Vec<_>>()
                    .join(";");
                lines.push(format!("a=fmtp:{} {params}", payload.id));
            }
        }
        if let Some(ssrc) = &rtp.ssrc {
            lines.push(format!("a=ssrc:{ssrc} cname:{ssrc}"));
        }
        for candidate in &ice.candidates {
            lines.push(format!("a={}", candidate_to_sdp(candidate)));
        }
    }

    Ok(lines.join("\r\n") + "\r\n")
}

/// Build a Jingle `transport-info` for one trickled candidate.
pub fn transport_info(sid: &str, content_name: &str, candidate: &str) -> Result<String> {
    let candidate = parse_candidate(candidate)?;
    let mut jingle = Jingle::new(Action::TransportInfo, SessionId(sid.to_string()));
    let mut transport = IceTransport::new();
    transport.candidates.push(candidate);
    let mut content = Content::new(Creator::Initiator, ContentId(content_name.to_string()));
    content.transport = Some(xmpp_parsers::jingle::Transport::IceUdp(transport));
    jingle.contents.push(content);
    Ok(element_to_string(Element::from(jingle)))
}

/// Extract `(content index, SDP candidate)` pairs from a transport-info.
pub fn candidates_from_jingle(jingle_xml: &str) -> Result<Vec<(u32, String)>> {
    let element: Element = jingle_xml.parse().context("invalid Jingle XML")?;
    let jingle = Jingle::try_from(element).map_err(|error| anyhow!("invalid Jingle: {error}"))?;
    let mut out = Vec::new();
    for (index, content) in jingle.contents.iter().enumerate() {
        let Some(xmpp_parsers::jingle::Transport::IceUdp(ice)) = &content.transport else {
            continue;
        };
        for candidate in &ice.candidates {
            out.push((index as u32, candidate_to_sdp(candidate)));
        }
    }
    Ok(out)
}

/// Build a minimal session-terminate.
pub fn terminate(sid: &str, reason: &str) -> String {
    let mut jingle = Jingle::new(Action::SessionTerminate, SessionId(sid.to_string()));
    let reason = match reason {
        "busy" => Reason::Busy,
        "cancel" => Reason::Cancel,
        "decline" => Reason::Decline,
        "timeout" => Reason::Timeout,
        "connectivity-error" => Reason::ConnectivityError,
        _ => Reason::Success,
    };
    jingle.reason = Some(ReasonElement {
        reason,
        texts: std::collections::BTreeMap::new(),
    });
    element_to_string(Element::from(jingle))
}

fn parse_rtpmap(value: &str) -> Option<PayloadType> {
    let mut parts = value.split_whitespace();
    let id = parts.next()?.parse::<u8>().ok()?;
    let rest = parts.next()?;
    let mut encoding = rest.split('/');
    let name = encoding.next()?.to_string();
    let clockrate = encoding.next().and_then(|v| v.parse::<u32>().ok())?;
    let channels = encoding
        .next()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1);
    Some(PayloadType::new(id, name, clockrate, channels))
}

fn parse_fmtp(value: &str) -> Option<(u8, Vec<Parameter>)> {
    let mut parts = value.splitn(2, ' ');
    let id = parts.next()?.parse::<u8>().ok()?;
    let params = parts
        .next()
        .unwrap_or_default()
        .split(';')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            Some(Parameter {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect();
    Some((id, params))
}

fn parse_candidate(value: &str) -> Result<Candidate> {
    let mut tokens = value.split_whitespace();
    let foundation = tokens
        .next()
        .ok_or_else(|| anyhow!("candidate without foundation"))?
        .trim_start_matches("candidate:")
        .to_string();
    let component = tokens
        .next()
        .ok_or_else(|| anyhow!("candidate without component"))?
        .parse::<u8>()?;
    let protocol = tokens
        .next()
        .ok_or_else(|| anyhow!("candidate without protocol"))?
        .to_string();
    let priority = tokens
        .next()
        .ok_or_else(|| anyhow!("candidate without priority"))?
        .parse::<u32>()?;
    let ip = tokens
        .next()
        .ok_or_else(|| anyhow!("candidate without address"))?
        .parse::<IpAddr>()?;
    let port = tokens
        .next()
        .ok_or_else(|| anyhow!("candidate without port"))?
        .parse::<u16>()?;

    let mut type_str = "host";
    let mut rel_addr = None;
    let mut rel_port = None;
    let mut generation = 0;
    let mut network = None;
    while let Some(token) = tokens.next() {
        match token {
            "typ" => {
                if let Some(value) = tokens.next() {
                    type_str = value;
                }
            }
            "raddr" => {
                if let Some(value) = tokens.next() {
                    rel_addr = value.parse::<IpAddr>().ok();
                }
            }
            "rport" => {
                if let Some(value) = tokens.next() {
                    rel_port = value.parse::<u16>().ok();
                }
            }
            "generation" => {
                if let Some(value) = tokens.next() {
                    generation = value.parse::<u8>().unwrap_or(0);
                }
            }
            "network-id" => {
                if let Some(value) = tokens.next() {
                    network = value.parse::<u8>().ok();
                }
            }
            _ => {}
        }
    }

    Ok(Candidate {
        component,
        foundation: foundation.clone(),
        generation,
        id: format!("{foundation}-{component}"),
        ip,
        port,
        priority,
        protocol,
        rel_addr,
        rel_port,
        network,
        type_: type_str.parse::<CandidateType>()?,
    })
}

fn candidate_to_sdp(candidate: &Candidate) -> String {
    let mut line = format!(
        "candidate:{} {} {} {} {} {} typ {}",
        candidate.foundation,
        candidate.component,
        candidate.protocol,
        candidate.priority,
        candidate.ip,
        candidate.port,
        candidate.type_
    );
    if let Some(address) = candidate.rel_addr {
        line.push_str(&format!(" raddr {address}"));
    }
    if let Some(port) = candidate.rel_port {
        line.push_str(&format!(" rport {port}"));
    }
    line.push_str(&format!(" generation {}", candidate.generation));
    if let Some(network) = candidate.network {
        line.push_str(&format!(" network-id {network}"));
    }
    line
}

/// First content name in a Jingle session.
pub fn first_content_name(jingle_xml: &str) -> Result<String> {
    let element: Element = jingle_xml.parse().context("invalid Jingle XML")?;
    let jingle = Jingle::try_from(element).map_err(|error| anyhow!("invalid Jingle: {error}"))?;
    jingle
        .contents
        .first()
        .map(|content| content.name.0.clone())
        .ok_or_else(|| anyhow!("Jingle session has no content"))
}

/// First media `a=mid` in an SDP description.
pub fn sdp_first_mid(sdp: &str) -> Result<String> {
    let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
        .map_err(|error| anyhow!("invalid SDP: {error}"))?;
    let media = message
        .medias()
        .next()
        .ok_or_else(|| anyhow!("SDP has no media"))?;
    media
        .attributes()
        .find(|attribute| attribute.key() == "mid")
        .and_then(|attribute| attribute.value())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("SDP media has no mid"))
}

fn element_to_string(element: Element) -> String {
    let mut buffer = Vec::new();
    let _ = element.write_to(&mut buffer);
    String::from_utf8_lossy(&buffer).into_owned()
}

fn colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_candidate_round_trip() {
        let sdp = "candidate:1 1 UDP 2130706431 192.168.1.4 54321 typ host generation 0";
        let candidate = parse_candidate(sdp).unwrap();
        assert_eq!(candidate.component, 1);
        assert_eq!(candidate.port, 54321);
        assert_eq!(candidate.type_, CandidateType::Host);
        assert!(candidate_to_sdp(&candidate).contains("typ host"));
    }

    #[test]
    fn parses_rtpmap() {
        let payload = parse_rtpmap("111 opus/48000/2").unwrap();
        assert_eq!(payload.id, 111);
        assert_eq!(payload.name.as_deref(), Some("opus"));
        assert_eq!(payload.clockrate, Some(48000));
    }

    #[test]
    fn terminate_has_reason() {
        let xml = terminate("abc", "busy");
        let jingle = Jingle::try_from(xml.parse::<Element>().unwrap()).unwrap();
        assert_eq!(jingle.action, Action::SessionTerminate);
        assert_eq!(jingle.reason.unwrap().reason, Reason::Busy);
    }
}
