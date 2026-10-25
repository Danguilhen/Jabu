//! xdg-desktop-portal ScreenCast client used for Wayland screen capture.

use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use gio::prelude::*;
use glib::translate::{from_glib_full, ToGlibPtr};
use glib::variant::ObjectPath;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SCREEN_CAST: &str = "org.freedesktop.portal.ScreenCast";
const REQUEST: &str = "org.freedesktop.portal.Request";

/// A live portal screen-cast session. Keep it alive while the stream is used.
pub struct ScreenCastSession {
    connection: gio::DBusConnection,
    session: ObjectPath,
    pub node: u32,
    /// The PipeWire remote the portal opened for us. The stream belongs to the
    /// portal's PipeWire client, so the source has to connect through this
    /// descriptor; the daemon's default socket cannot see it.
    pub fd: OwnedFd,
}

/// Ask the desktop portal for a screen-cast stream and return its PipeWire node.
pub async fn request_screen_cast() -> Result<ScreenCastSession> {
    tokio::task::spawn_blocking(request_blocking)
        .await
        .context("screen-cast task failed")?
}

fn request_blocking() -> Result<ScreenCastSession> {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| request_with_context(&context))
        .map_err(|error| anyhow!("cannot enter the GLib main context: {error}"))?
}

fn request_with_context(context: &glib::MainContext) -> Result<ScreenCastSession> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .context("cannot open the session bus")?;
    let unique = connection
        .unique_name()
        .ok_or_else(|| anyhow!("session bus has no unique name"))?
        .to_string();
    let sender = unique.trim_start_matches(':').replace('.', "_");

    let session_token = format!("jabu_session_{}", std::process::id());
    let create_token = format!("jabu_create_{}", std::process::id());
    let create_path = format!("/org/freedesktop/portal/desktop/request/{sender}/{create_token}");
    let create_options = glib::VariantDict::new(None);
    create_options.insert("handle_token", &create_token);
    create_options.insert("session_handle_token", &session_token);
    let create_values = create_options.end();
    let create_args = glib::Variant::tuple_from_iter([&create_values]);
    let created = portal_request(
        &connection,
        context,
        "CreateSession",
        &create_path,
        &create_args,
    )?;
    let dict = glib::VariantDict::new(Some(&created));
    let session = dict
        .lookup::<ObjectPath>("session_handle")
        .ok()
        .flatten()
        .or_else(|| {
            dict.lookup::<String>("session_handle")
                .ok()
                .flatten()
                .and_then(|path| ObjectPath::try_from(path).ok())
        })
        .ok_or_else(|| anyhow!("portal did not return a session handle"))?;

    let select_token = format!("jabu_select_{}", std::process::id());
    let select_path = format!("/org/freedesktop/portal/desktop/request/{sender}/{select_token}");
    let select_options = glib::VariantDict::new(None);
    select_options.insert("handle_token", &select_token);
    select_options.insert("types", &3u32); // monitor | window
    select_options.insert("multiple", &false);
    select_options.insert("cursor_mode", &2u32); // embedded
    let select_values = select_options.end();
    let select_args = glib::Variant::tuple_from_iter([&session.to_variant(), &select_values]);
    portal_request(
        &connection,
        context,
        "SelectSources",
        &select_path,
        &select_args,
    )?;

    let start_token = format!("jabu_start_{}", std::process::id());
    let start_path = format!("/org/freedesktop/portal/desktop/request/{sender}/{start_token}");
    let start_options = glib::VariantDict::new(None);
    start_options.insert("handle_token", &start_token);
    let start_values = start_options.end();
    let start_args =
        glib::Variant::tuple_from_iter([&session.to_variant(), &"".to_variant(), &start_values]);
    let started = portal_request(&connection, context, "Start", &start_path, &start_args)?;

    let dict = glib::VariantDict::new(Some(&started));
    let streams = dict
        .lookup_value("streams", None)
        .ok_or_else(|| anyhow!("portal did not return any stream"))?;
    if streams.n_children() == 0 {
        bail!("no screen was selected");
    }
    let stream = streams.child_value(0);
    let node = stream
        .child_value(0)
        .get::<u32>()
        .ok_or_else(|| anyhow!("portal stream has no PipeWire node"))?;
    let fd = open_pipewire_remote(&connection, &session)?;

    Ok(ScreenCastSession {
        connection,
        session,
        node,
        fd,
    })
}

/// Ask the portal for the PipeWire remote behind this screen-cast session.
///
/// `OpenPipeWireRemote` is the only way to reach the stream: it returns a
/// connected descriptor, not a socket path, which is why this goes through the
/// fd-list variant of the D-Bus call.
fn open_pipewire_remote(connection: &gio::DBusConnection, session: &ObjectPath) -> Result<OwnedFd> {
    let options = glib::VariantDict::new(None).end();
    let arguments = glib::Variant::tuple_from_iter([&session.to_variant(), &options]);
    let mut out_fd_list: *mut gio::ffi::GUnixFDList = std::ptr::null_mut();
    let mut error: *mut glib::ffi::GError = std::ptr::null_mut();
    let reply = unsafe {
        gio::ffi::g_dbus_connection_call_with_unix_fd_list_sync(
            connection.to_glib_none().0,
            PORTAL.to_glib_none().0,
            PORTAL_PATH.to_glib_none().0,
            SCREEN_CAST.to_glib_none().0,
            "OpenPipeWireRemote".to_glib_none().0,
            arguments.to_glib_none().0,
            std::ptr::null(),
            gio::DBusCallFlags::NONE.bits(),
            -1,
            std::ptr::null_mut(),
            &mut out_fd_list,
            std::ptr::null_mut(),
            &mut error,
        )
    };
    if reply.is_null() {
        let error: glib::Error = unsafe { from_glib_full(error) };
        return Err(anyhow!("portal OpenPipeWireRemote failed: {error}"));
    }
    unsafe { glib::ffi::g_variant_unref(reply) };
    if out_fd_list.is_null() {
        bail!("portal OpenPipeWireRemote returned no descriptor");
    }
    let list: gio::UnixFDList = unsafe { from_glib_full(out_fd_list) };
    let fd: RawFd = list
        .get(0)
        .context("portal returned no PipeWire descriptor")?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn portal_request(
    connection: &gio::DBusConnection,
    context: &glib::MainContext,
    method: &str,
    request_path: &str,
    arguments: &glib::Variant,
) -> Result<glib::Variant> {
    let (sender, receiver) = mpsc::channel();
    let subscription = connection.signal_subscribe(
        Some(PORTAL),
        Some(REQUEST),
        Some("Response"),
        Some(request_path),
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, value| {
            let _ = sender.send(value.clone());
        },
    );

    let reply = connection
        .call_sync(
            Some(PORTAL),
            PORTAL_PATH,
            SCREEN_CAST,
            method,
            Some(arguments),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .with_context(|| format!("portal {method} call failed"))?;
    let actual_path = reply.get::<ObjectPath>().map(|path| String::from(path));
    if actual_path
        .as_deref()
        .is_some_and(|path| path != request_path)
    {
        // The portal may choose a different request object; resubscribe before
        // waiting for the response. The response is generally sent after the
        // method returns, so this is still safe.
        connection.signal_unsubscribe(subscription);
        return portal_request_with_path(
            connection,
            context,
            method,
            &actual_path.unwrap_or_else(|| request_path.to_string()),
        );
    }

    let value = wait_for_response(context, &receiver)?;
    connection.signal_unsubscribe(subscription);
    let code = value
        .child_value(0)
        .get::<u32>()
        .ok_or_else(|| anyhow!("portal {method} returned an invalid response"))?;
    if code != 0 {
        bail!("portal {method} was cancelled ({code})");
    }
    Ok(value.child_value(1))
}

fn portal_request_with_path(
    connection: &gio::DBusConnection,
    context: &glib::MainContext,
    method: &str,
    request_path: &str,
) -> Result<glib::Variant> {
    let (sender, receiver) = mpsc::channel();
    let subscription = connection.signal_subscribe(
        Some(PORTAL),
        Some(REQUEST),
        Some("Response"),
        Some(request_path),
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, value| {
            let _ = sender.send(value.clone());
        },
    );
    let value = wait_for_response(context, &receiver)?;
    connection.signal_unsubscribe(subscription);
    let code = value
        .child_value(0)
        .get::<u32>()
        .ok_or_else(|| anyhow!("portal {method} returned an invalid response"))?;
    if code != 0 {
        bail!("portal {method} was cancelled ({code})");
    }
    Ok(value.child_value(1))
}

fn wait_for_response(
    context: &glib::MainContext,
    receiver: &mpsc::Receiver<glib::Variant>,
) -> Result<glib::Variant> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(value) = receiver.try_recv() {
            return Ok(value);
        }
        while context.iteration(false) {}
        if Instant::now() >= deadline {
            bail!("no screen was selected within 60 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl Drop for ScreenCastSession {
    fn drop(&mut self) {
        let _ = self.connection.call_sync(
            Some(PORTAL),
            &self.session.to_string(),
            "org.freedesktop.portal.Session",
            "Close",
            None,
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        );
    }
}
