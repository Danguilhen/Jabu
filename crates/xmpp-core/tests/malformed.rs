//! Malformed input must be rejected without panicking.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::str::FromStr;

use xmpp_parsers::minidom::Element;

const MALFORMED: &[&str] = &[
    "",
    "<",
    "</message>",
    "<message>",
    "<message xmlns='jabber:client'><body></message>",
    "<message xmlns='jabber:client'><body>&bogus;</body></message>",
    "<iq xmlns='jabber:client' type='result'><fin xmlns='urn:xmpp:mam:2'",
    "<presence type='subscribe' to='not a jid'/>",
    "<message xmlns='jabber:client'><result xmlns='urn:xmpp:mam:2'><forwarded/></result></message>",
    "<iq xmlns='jabber:client' type='set'><query xmlns='jabber:iq:roster'><item/></query></iq>",
    "<enabled xmlns='urn:xmpp:sm:3' max='not-a-number'/>",
    "<message xmlns='jabber:client'><reactions xmlns='urn:xmpp:reactions:0'/></message>",
    "<iq xmlns='jabber:client' type='result'><services xmlns='urn:xmpp:extdisco:2'><service/></services></iq>",
];

#[test]
fn malformed_stanzas_never_panic() {
    for xml in MALFORMED {
        let outcome = catch_unwind(AssertUnwindSafe(|| Element::from_str(xml)));
        assert!(outcome.is_ok(), "parser panicked on: {xml:?}");
    }
}

#[test]
fn deterministic_fuzz_never_panics() {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    for case in 0..5000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let len = usize::try_from(state % 256).unwrap_or(0);
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            bytes.push(u8::try_from((state >> 33) & 0xff).unwrap_or(0));
        }
        let input = String::from_utf8_lossy(&bytes).into_owned();
        let outcome = catch_unwind(AssertUnwindSafe(|| Element::from_str(&input)));
        assert!(outcome.is_ok(), "parser panicked on fuzz case {case}");
    }
}
