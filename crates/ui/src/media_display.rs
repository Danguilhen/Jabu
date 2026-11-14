// SPDX-License-Identifier: MIT

//! Choosing between an original animated image and its static thumbnail.

use xmpp_core::MediaAttachment;

/// True when a MIME type is known to carry animation.
#[must_use]
pub fn is_animated_mime(mime: &str) -> bool {
    matches!(
        mime.split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "image/gif" | "image/webp" | "image/apng"
    )
}

/// True when this attachment should render the original bytes.
#[must_use]
pub fn is_animated(attachment: &MediaAttachment) -> bool {
    attachment.mime.as_deref().is_some_and(is_animated_mime)
}

/// The URL to show in a chat image. Animated media prefers the original; other
/// images keep using a thumbnail when one is available.
#[must_use]
pub fn display_url(attachment: &MediaAttachment) -> &str {
    if is_animated(attachment) {
        return &attachment.url;
    }
    attachment
        .thumbnail_url
        .as_deref()
        .unwrap_or(&attachment.url)
}

/// A thumbnail to paint behind the original while it loads.
#[must_use]
pub fn placeholder_url(attachment: &MediaAttachment) -> Option<&str> {
    if is_animated(attachment) {
        attachment.thumbnail_url.as_deref()
    } else {
        None
    }
}

/// A short badge for the attachment, if any.
#[must_use]
pub fn badge(attachment: &MediaAttachment) -> Option<&'static str> {
    let mime = attachment.mime.as_deref()?;
    if is_animated_mime(mime) {
        if mime.to_ascii_lowercase().starts_with("image/webp") {
            Some("WEBP")
        } else {
            Some("GIF")
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{badge, display_url, is_animated, placeholder_url};
    use xmpp_core::{MediaAttachment, MediaType};

    fn attachment(mime: Option<&str>, thumbnail: Option<&str>) -> MediaAttachment {
        MediaAttachment {
            id: "1".to_string(),
            url: "https://example.org/original".to_string(),
            media_type: MediaType::Image,
            mime: mime.map(str::to_string),
            name: Some("image".to_string()),
            size: None,
            width: None,
            height: None,
            duration: None,
            thumbnail_url: thumbnail.map(str::to_string),
            blur_hash: None,
        }
    }

    #[test]
    fn animated_images_use_the_original() {
        let gif = attachment(Some("image/gif"), Some("https://example.org/thumb.png"));
        assert!(is_animated(&gif));
        assert_eq!(display_url(&gif), "https://example.org/original");
        assert_eq!(placeholder_url(&gif), Some("https://example.org/thumb.png"));
        assert_eq!(badge(&gif), Some("GIF"));

        let webp = attachment(Some("image/webp"), Some("https://example.org/thumb.png"));
        assert_eq!(display_url(&webp), "https://example.org/original");
        assert_eq!(badge(&webp), Some("WEBP"));

        let apng = attachment(Some("image/apng"), None);
        assert_eq!(display_url(&apng), "https://example.org/original");
    }

    #[test]
    fn static_images_keep_the_thumbnail() {
        let png = attachment(Some("image/png"), Some("https://example.org/thumb.png"));
        assert!(!is_animated(&png));
        assert_eq!(display_url(&png), "https://example.org/thumb.png");
        assert_eq!(placeholder_url(&png), None);
        assert_eq!(badge(&png), None);
    }

    #[test]
    fn missing_mime_and_thumbnail_fall_back_to_the_original() {
        let unknown = attachment(None, None);
        assert_eq!(display_url(&unknown), "https://example.org/original");
        assert_eq!(badge(&unknown), None);
    }

    #[test]
    fn mime_parameters_do_not_hide_animation() {
        assert!(super::is_animated_mime("image/gif; charset=binary"));
    }
}
