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
    for media in message.medias() {
        jingle.contents.push(sdp_media_content(media));
    }
    Ok(element_to_string(Element::from(jingle)))
}

/// Build a Jingle content-add/content-accept carrying only one media type.
pub fn sdp_to_jingle_media(
    sdp: &str,
    action: Action,
    sid: &str,
    media_type: &str,
) -> Result<String> {
    let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
        .map_err(|error| anyhow!("invalid SDP: {error}"))?;
    let media = message
        .medias()
        .find(|media| media.media() == Some(media_type))
        .ok_or_else(|| anyhow!("SDP has no {media_type} media"))?;
    let mut jingle = Jingle::new(action, SessionId(sid.to_string()));
    jingle.contents.push(sdp_media_content(media));
    Ok(element_to_string(Element::from(jingle)))
}

/// Build a Jingle content-add/content-accept for one SDP media index.
pub fn sdp_to_jingle_media_at(sdp: &str, action: Action, sid: &str, index: u32) -> Result<String> {
    let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
        .map_err(|error| anyhow!("invalid SDP: {error}"))?;
    let media = message
        .medias()
        .nth(index as usize)
        .ok_or_else(|| anyhow!("SDP has no media at index {index}"))?;
    let mut jingle = Jingle::new(action, SessionId(sid.to_string()));
    jingle.contents.push(sdp_media_content(media));
    Ok(element_to_string(Element::from(jingle)))
}

fn sdp_media_content(media: &gstreamer_sdp::SDPMediaRef) -> Content {
    let media_type = media.media().unwrap_or("audio").to_string();
    let mut rtp = RtpDescription::new(media_type.clone());
    let mut transport = IceTransport::new();
    let mut setup = Setup::Actpass;
    let mut fingerprint: Option<(Algo, String)> = None;
    let mut content_name = media_type;
    let mut ssrc = None;
    let mut senders = Senders::Both;

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
            "rtcp-mux" | "rtcp-mux-only" => rtp.rtcp_mux = Some(RtcpMux),
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
            "sendonly" => senders = Senders::Initiator,
            "recvonly" => senders = Senders::Responder,
            "inactive" => senders = Senders::None,
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
        if let Ok(fp) = Fingerprint::from_colon_separated_hex(setup, &String::from(algo), &hash) {
            transport.fingerprint = Some(fp);
        }
    }

    let mut content = Content::new(Creator::Initiator, ContentId(content_name));
    content.senders = senders;
    content.description = Some(Description::Rtp(rtp));
    content.transport = Some(xmpp_parsers::jingle::Transport::IceUdp(transport));
    content
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
    if !mids.is_empty() {
        lines.push(format!("a=group:BUNDLE {}", mids.join(" ")));
    }

    for content in &jingle.contents {
        if let Some(media_lines) = content_media_lines(content) {
            lines.extend(media_lines);
        }
    }

    Ok(lines.join("\r\n") + "\r\n")
}

/// Convert the first RTP content of a Jingle stanza into an SDP media block.
pub fn first_content_media(jingle_xml: &str) -> Result<String> {
    let element: Element = jingle_xml.parse().context("invalid Jingle XML")?;
    let jingle = Jingle::try_from(element).map_err(|error| anyhow!("invalid Jingle: {error}"))?;
    let content = jingle
        .contents
        .first()
        .ok_or_else(|| anyhow!("Jingle stanza has no content"))?;
    let lines = content_media_lines(content)
        .ok_or_else(|| anyhow!("Jingle content has no RTP/ICE description"))?;
    Ok(lines.join("\r\n") + "\r\n")
}

fn content_media_lines(content: &Content) -> Option<Vec<String>> {
    let Some(Description::Rtp(rtp)) = &content.description else {
        return None;
    };
    let Some(xmpp_parsers::jingle::Transport::IceUdp(ice)) = &content.transport else {
        return None;
    };

    let formats = rtp
        .payload_types
        .iter()
        .map(|payload| payload.id.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let mut lines = vec![
        format!("m={} 9 UDP/TLS/RTP/SAVPF {}", rtp.media, formats),
        "c=IN IP4 0.0.0.0".to_string(),
        "a=rtcp:9 IN IP4 0.0.0.0".to_string(),
    ];
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
    Some(lines)
}

/// Replace or append SDP media blocks in a description.
///
/// Blocks are matched by media type (audio/video), so a renegotiated video
/// section replaces the one already known to the peer. The BUNDLE grouping is
/// rebuilt from the resulting mids.
pub fn merge_sdp(base: &str, blocks: &[String], bump_version: bool) -> Result<String> {
    let (session, mut media) = split_sdp(base)?;

    for block in blocks {
        let new_lines = sdp_block_lines(block)?;
        let media_type = media_type_of(&new_lines);
        if let Some(existing) = media
            .iter_mut()
            .find(|section| media_type_of(section) == media_type)
        {
            *existing = new_lines;
        } else {
            media.push(new_lines);
        }
    }

    Ok(rebuild_sdp(session, media, bump_version))
}

/// Append a media section to an SDP description.
pub fn append_sdp_media(base: &str, block: &str, bump_version: bool) -> Result<String> {
    let (session, mut media) = split_sdp(base)?;
    media.push(sdp_block_lines(block)?);
    Ok(rebuild_sdp(session, media, bump_version))
}

/// Replace a media section identified by its mid, or append it when missing.
pub fn replace_sdp_media_by_mid(
    base: &str,
    mid: &str,
    block: &str,
    bump_version: bool,
) -> Result<String> {
    let (session, mut media) = split_sdp(base)?;
    let lines = sdp_block_lines(block)?;
    let needle = format!("a=mid:{mid}");
    if let Some(existing) = media
        .iter_mut()
        .find(|section| section.iter().any(|line| line == &needle))
    {
        *existing = lines;
    } else {
        media.push(lines);
    }
    Ok(rebuild_sdp(session, media, bump_version))
}

fn split_sdp(base: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut session: Vec<String> = Vec::new();
    let mut media: Vec<Vec<String>> = Vec::new();
    for line in base.lines().filter(|line| !line.is_empty()) {
        if line.starts_with("m=") {
            media.push(vec![line.to_string()]);
        } else if let Some(current) = media.last_mut() {
            current.push(line.to_string());
        } else {
            session.push(line.to_string());
        }
    }
    if media.is_empty() {
        return Err(anyhow!("base SDP has no media sections"));
    }
    Ok((session, media))
}

fn sdp_block_lines(block: &str) -> Result<Vec<String>> {
    let mut lines = block.lines().filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return Err(anyhow!("SDP block is empty"));
    };
    if !first.starts_with("m=") {
        return Err(anyhow!("SDP block does not start with m="));
    }
    Ok(std::iter::once(first.to_string())
        .chain(lines.map(str::to_string))
        .collect())
}

fn media_type_of(section: &[String]) -> &str {
    section[0]
        .strip_prefix("m=")
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_default()
}

fn rebuild_sdp(mut session: Vec<String>, media: Vec<Vec<String>>, bump_version: bool) -> String {
    if bump_version {
        if let Some(origin) = session.iter_mut().find(|line| line.starts_with("o=")) {
            let parts: Vec<&str> = origin.split(' ').collect();
            if parts.len() > 2 {
                let version = parts[2].parse::<u64>().unwrap_or(0) + 1;
                let bumped = version.to_string();
                let owned: Vec<String> = parts
                    .iter()
                    .enumerate()
                    .map(|(index, part)| {
                        if index == 2 {
                            bumped.clone()
                        } else {
                            (*part).to_string()
                        }
                    })
                    .collect();
                *origin = owned.join(" ");
            }
        }
    }

    let mids: Vec<String> = media
        .iter()
        .filter_map(|section| {
            section
                .iter()
                .find(|line| line.starts_with("a=mid:"))
                .map(|line| line["a=mid:".len()..].to_string())
        })
        .collect();
    if !mids.is_empty() {
        let bundle = format!("a=group:BUNDLE {}", mids.join(" "));
        if let Some(line) = session
            .iter_mut()
            .find(|line| line.starts_with("a=group:BUNDLE"))
        {
            *line = bundle;
        } else {
            session.push(bundle);
        }
    }

    let mut lines = session;
    for section in media {
        lines.extend(section);
    }
    lines.join("\r\n") + "\r\n"
}

/// Media block for one mid in an SDP description.
pub fn sdp_media_by_mid(sdp: &str, mid: &str) -> Option<String> {
    let mut sections: Vec<Vec<&str>> = Vec::new();
    for line in sdp.lines().filter(|line| !line.is_empty()) {
        if line.starts_with("m=") {
            sections.push(vec![line]);
        } else if let Some(section) = sections.last_mut() {
            section.push(line);
        }
    }
    sections
        .into_iter()
        .find(|section| section.contains(&format!("a=mid:{mid}").as_str()))
        .map(|section| section.join("\r\n") + "\r\n")
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
    sdp_mids(sdp)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("SDP has no media mid"))
}

/// Media `a=mid` values in SDP order.
pub fn sdp_mids(sdp: &str) -> Vec<String> {
    let Ok(message) = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes()) else {
        return Vec::new();
    };
    message
        .medias()
        .filter_map(|media| {
            media
                .attributes()
                .find(|attribute| attribute.key() == "mid")
                .and_then(|attribute| attribute.value())
                .map(str::to_string)
        })
        .collect()
}

/// Index of the first media section of a given type in an SDP description.
#[must_use]
pub fn sdp_media_index(sdp: &str, media_type: &str) -> Option<u32> {
    let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes()).ok()?;
    message
        .medias()
        .position(|media| media.media() == Some(media_type))
        .and_then(|index| u32::try_from(index).ok())
}

/// Index of the last media section of a given type in an SDP description.
#[must_use]
pub fn sdp_last_media_index(sdp: &str, media_type: &str) -> Option<u32> {
    let message = gstreamer_sdp::SDPMessage::parse_buffer(sdp.as_bytes()).ok()?;
    message
        .medias()
        .enumerate()
        .filter(|(_, media)| media.media() == Some(media_type))
        .last()
        .and_then(|(index, _)| u32::try_from(index).ok())
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

    const AUDIO_SDP: &str = "v=0\r\no=- 1 0 IN IP4 0.0.0.0\r\ns=-\r\nt=0 0\r\na=group:BUNDLE audio0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 96\r\nc=IN IP4 0.0.0.0\r\na=ice-ufrag:aaa\r\na=ice-pwd:bbb\r\na=fingerprint:sha-256 11:22\r\na=setup:actpass\r\na=sendrecv\r\na=rtpmap:96 OPUS/48000/2\r\na=mid:audio0\r\n";

    const VIDEO_MEDIA: &str = "m=video 0 UDP/TLS/RTP/SAVPF 97\r\nc=IN IP4 0.0.0.0\r\na=ice-ufrag:aaa\r\na=ice-pwd:bbb\r\na=fingerprint:sha-256 11:22\r\na=setup:actpass\r\na=sendrecv\r\na=rtpmap:97 VP8/90000\r\na=mid:video1\r\n";

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

    #[test]
    fn merge_appends_video_and_bumps_version() {
        let merged = merge_sdp(AUDIO_SDP, &[VIDEO_MEDIA.to_string()], true).unwrap();
        assert!(merged.contains("o=- 1 1 IN IP4 0.0.0.0"));
        assert!(merged.contains("a=group:BUNDLE audio0 video1"));
        assert_eq!(merged.matches("m=video").count(), 1);

        let again = merge_sdp(&merged, &[VIDEO_MEDIA.to_string()], false).unwrap();
        assert_eq!(again.matches("m=video").count(), 1);
        assert_eq!(again.matches("m=audio").count(), 1);
    }

    #[test]
    fn content_add_carries_only_video() {
        let _ = gstreamer::init();
        let sdp = format!("{AUDIO_SDP}{VIDEO_MEDIA}");
        let xml = sdp_to_jingle_media(&sdp, Action::ContentAdd, "sid1", "video").unwrap();
        let jingle = Jingle::try_from(xml.parse::<Element>().unwrap()).unwrap();
        assert_eq!(jingle.action, Action::ContentAdd);
        assert_eq!(jingle.contents.len(), 1);
        assert_eq!(jingle.contents[0].name.0, "video1");

        let media = first_content_media(&xml).unwrap();
        assert!(media.starts_with("m=video"));
        assert!(media.contains("a=mid:video1"));
        let merged = merge_sdp(AUDIO_SDP, &[media], true).unwrap();
        assert_eq!(merged.matches("m=video").count(), 1);
        assert!(merged.contains("a=group:BUNDLE audio0 video1"));
    }

    #[test]
    fn append_keeps_existing_video() {
        let _ = gstreamer::init();
        let base = format!("{AUDIO_SDP}{VIDEO_MEDIA}");
        let second = VIDEO_MEDIA.replace("97", "98").replace("video1", "video2");
        let appended = append_sdp_media(&base, &second, true).unwrap();
        assert_eq!(appended.matches("m=video").count(), 2);
        assert!(appended.contains("a=group:BUNDLE audio0 video1 video2"));
        assert_eq!(sdp_last_media_index(&appended, "video"), Some(2));
    }

    #[test]
    fn replace_media_by_mid_changes_only_that_section() {
        let _ = gstreamer::init();
        let base = format!("{AUDIO_SDP}{VIDEO_MEDIA}");
        let replacement = VIDEO_MEDIA.replace("97", "98");
        let replaced = replace_sdp_media_by_mid(&base, "video1", &replacement, false).unwrap();
        assert_eq!(replaced.matches("m=video").count(), 1);
        assert!(replaced.contains("a=rtpmap:98 VP8/90000"));
        assert!(replaced.contains("a=rtpmap:96 OPUS/48000/2"));
    }

    #[test]
    fn direction_maps_to_senders() {
        let _ = gstreamer::init();
        let recvonly = AUDIO_SDP.replace("a=sendrecv", "a=recvonly");
        let jingle = sdp_to_jingle_media(&recvonly, Action::ContentAccept, "sid", "audio").unwrap();
        let element: Element = jingle.parse().unwrap();
        let content = element
            .children()
            .find(|child| child.name() == "content")
            .unwrap();
        assert_eq!(content.attr("senders"), Some("responder"));

        let sendonly = AUDIO_SDP.replace("a=sendrecv", "a=sendonly");
        let jingle =
            sdp_to_jingle_media(&sendonly, Action::SessionInitiate, "sid", "audio").unwrap();
        let element: Element = jingle.parse().unwrap();
        let content = element
            .children()
            .find(|child| child.name() == "content")
            .unwrap();
        assert_eq!(content.attr("senders"), Some("initiator"));
    }

    #[test]
    fn media_by_mid_selects_audio() {
        let _ = gstreamer::init();
        let sdp = format!("{AUDIO_SDP}{VIDEO_MEDIA}");
        let audio = sdp_media_by_mid(&sdp, "audio0").unwrap();
        assert!(audio.starts_with("m=audio"));
        assert!(!audio.contains("m=video"));
        assert_eq!(sdp_mids(&sdp), vec!["audio0", "video1"]);
        assert_eq!(sdp_media_index(&sdp, "audio"), Some(0));
        assert_eq!(sdp_media_index(&sdp, "video"), Some(1));
        assert_eq!(sdp_media_index(&sdp, "application"), None);
    }
}
