//! Deterministic identity colours (XEP-0392 hue derivation).

use sha1::{Digest, Sha1};

/// A stable colour for an identity such as a JID or nickname.
///
/// The hue is derived from the first two bytes of the SHA-1 digest, so the
/// same identifier always maps to the same colour on every platform.
#[must_use]
pub fn identity_color(identifier: &str) -> (u8, u8, u8) {
    let digest = Sha1::digest(identifier.as_bytes());
    let hue = f32::from(u16::from_be_bytes([digest[0], digest[1]])) / 65_535.0 * 360.0;
    hsl_to_rgb(hue, 0.65, 0.55)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops
)]
fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (u8, u8, u8) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = (hue / 60.0) % 6.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    let channel = |value: f32| ((value + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (channel(red), channel(green), channel(blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_deterministic() {
        assert_eq!(
            identity_color("alice@localhost"),
            identity_color("alice@localhost")
        );
    }

    #[test]
    fn differs_between_identities() {
        assert_ne!(
            identity_color("alice@localhost"),
            identity_color("bob@localhost")
        );
    }

    #[test]
    fn produces_valid_channels() {
        for seed in ["alice@localhost", "room@conference.localhost", "日本語"] {
            assert_ne!(identity_color(seed), (0, 0, 0));
        }
    }
}
