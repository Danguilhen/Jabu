//! Screen-cast stub for platforms without xdg-desktop-portal.

use anyhow::Result;

pub struct ScreenCastSession {
    pub node: u32,
}

pub async fn request_screen_cast() -> Result<ScreenCastSession> {
    anyhow::bail!("screen capture is not supported on this platform")
}
