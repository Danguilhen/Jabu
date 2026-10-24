//!Receive-side conference media: every Galener participant gets its own
//! transport, even though Galener reuses `name="0"` in every `content-add`.

use std::str::FromStr;

use media::gstreamer::GStreamerMediaPort;
use xmpp_core::jingle::{CallKind, MediaPort};
use xmpp_parsers::minidom::Element;

const ADD_AUDIO: &str = include_str!("../../xmpp-core/tests/golden/galener/content_add_audio.xml");

fn jingle_of(iq: &str) -> String {
    let element = Element::from_str(iq).expect("fixture parses");
    let jingle = element
        .get_child("jingle", "urn:xmpp:jingle:1")
        .expect("jingle");
    let mut buffer = Vec::new();
    jingle.write_to(&mut buffer).expect("serialise");
    String::from_utf8(buffer).expect("utf-8")
}

#[tokio::test]
async fn isolated_conference_contents_have_separate_answers() {
    std::env::set_var("JABU_AUDIO_TEST_TONE", "1");
    let media = GStreamerMediaPort::new(Vec::new()).expect("media port");
    let sid = "conference-receive-test";
    media
        .create_offer(sid, CallKind::Audio)
        .await
        .expect("offer");

    let add = jingle_of(ADD_AUDIO);
    let bob = media
        .accept_remote_content(sid, &add, "bob@localhost|0", true)
        .await
        .expect("bob content");
    let carol = media
        .accept_remote_content(sid, &add, "carol@localhost|0", true)
        .await
        .expect("carol content");

    for answer in [bob, carol] {
        let element = Element::from_str(&answer).expect("answer parses");
        let content = element
            .children()
            .find(|child| child.name() == "content")
            .expect("content");
        assert_eq!(content.attr("name"), Some("0"));
    }

    media.stop(sid).await.expect("stop");
}
