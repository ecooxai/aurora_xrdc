use std::{
    path::Path,
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::warn;
use x11rb::{
    CURRENT_TIME, NONE,
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, ConnectionExt as XProtoConnectionExt, CreateWindowAux, EventMask,
            GetPropertyReply, PropMode, SelectionNotifyEvent, SelectionRequestEvent, Window,
            WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as WrapperConnectionExt,
};

pub const CLIPBOARD_HISTORY_LIMIT: usize = 100;
const CLIPBOARD_HISTORY_PATH: &str = "/tmp/vibe_rdesk_clipboard_history.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClipboardPayload {
    pub text: Option<String>,
    pub image_png_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClipboardHistoryEntry {
    pub side: String,
    pub payload: ClipboardPayload,
}

pub async fn read_remote_clipboard(display: &str) -> Result<ClipboardPayload> {
    let display = display.to_owned();
    tokio::task::spawn_blocking(move || read_remote_clipboard_blocking(&display))
        .await
        .context("clipboard read task failed")?
}

pub async fn write_remote_clipboard(display: &str, payload: &ClipboardPayload) -> Result<()> {
    let display = display.to_owned();
    let payload = payload.clone();
    tokio::task::spawn_blocking(move || write_remote_clipboard_blocking(&display, payload))
        .await
        .context("clipboard write task failed")?
}

pub async fn ensure_upload_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .await
        .with_context(|| format!("failed to create upload dir {}", path.display()))
}

pub async fn read_clipboard_history() -> Result<Vec<ClipboardHistoryEntry>> {
    let bytes = match fs::read(CLIPBOARD_HISTORY_PATH).await {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to read clipboard history {}",
                    CLIPBOARD_HISTORY_PATH
                )
            });
        }
    };
    serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "failed to parse clipboard history {}",
            CLIPBOARD_HISTORY_PATH
        )
    })
}

pub async fn write_clipboard_history(entries: &[ClipboardHistoryEntry]) -> Result<()> {
    let trimmed: Vec<ClipboardHistoryEntry> = entries
        .iter()
        .take(CLIPBOARD_HISTORY_LIMIT)
        .cloned()
        .collect();
    let bytes = serde_json::to_vec(&trimmed).context("failed to serialize clipboard history")?;
    fs::write(CLIPBOARD_HISTORY_PATH, bytes)
        .await
        .with_context(|| {
            format!(
                "failed to write clipboard history {}",
                CLIPBOARD_HISTORY_PATH
            )
        })
}

struct ClipboardOwner {
    stop: mpsc::Sender<()>,
    handle: thread::JoinHandle<()>,
}

static CLIPBOARD_OWNER: OnceLock<Mutex<Option<ClipboardOwner>>> = OnceLock::new();

fn read_remote_clipboard_blocking(display: &str) -> Result<ClipboardPayload> {
    let client = X11ClipboardClient::connect(display)?;
    let targets = client.read_targets().unwrap_or_default();
    let image_png_b64 = if targets.contains(&client.atoms.image_png) {
        client
            .read_target(client.atoms.image_png)
            .ok()
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| STANDARD.encode(bytes))
    } else {
        None
    };
    let text = client
        .read_first_text(&targets)
        .ok()
        .filter(|text| !text.trim().is_empty());
    Ok(ClipboardPayload {
        text,
        image_png_b64,
    })
}

fn write_remote_clipboard_blocking(display: &str, payload: ClipboardPayload) -> Result<()> {
    let owner = ClipboardServer::start(display, payload)?;
    let mut current = CLIPBOARD_OWNER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow!("clipboard owner lock was poisoned"))?;
    if let Some(previous) = current.take() {
        let _ = previous.stop.send(());
        if let Err(err) = previous.handle.join() {
            warn!("failed to join previous clipboard owner: {err:?}");
        }
    }
    *current = Some(owner);
    Ok(())
}

#[derive(Clone, Copy)]
struct ClipboardAtoms {
    clipboard: Atom,
    targets: Atom,
    utf8_string: Atom,
    string: Atom,
    text_plain: Atom,
    text_plain_utf8: Atom,
    image_png: Atom,
    timestamp: Atom,
    vibe_property: Atom,
}

impl ClipboardAtoms {
    fn intern(connection: &RustConnection) -> Result<Self> {
        Ok(Self {
            clipboard: intern_atom(connection, b"CLIPBOARD")?,
            targets: intern_atom(connection, b"TARGETS")?,
            utf8_string: intern_atom(connection, b"UTF8_STRING")?,
            string: AtomEnum::STRING.into(),
            text_plain: intern_atom(connection, b"text/plain")?,
            text_plain_utf8: intern_atom(connection, b"text/plain;charset=utf-8")?,
            image_png: intern_atom(connection, b"image/png")?,
            timestamp: intern_atom(connection, b"TIMESTAMP")?,
            vibe_property: intern_atom(connection, b"VIBE_RDESK_CLIPBOARD")?,
        })
    }
}

fn intern_atom(connection: &RustConnection, name: &[u8]) -> Result<Atom> {
    Ok(connection
        .intern_atom(false, name)
        .context("failed to intern X11 atom")?
        .reply()
        .context("failed to read X11 atom reply")?
        .atom)
}

struct X11ClipboardClient {
    connection: RustConnection,
    window: Window,
    atoms: ClipboardAtoms,
}

impl X11ClipboardClient {
    fn connect(display: &str) -> Result<Self> {
        let (connection, screen_num) = x11rb::connect(Some(display))
            .with_context(|| format!("failed to connect to X11 display {display}"))?;
        let screen = connection
            .setup()
            .roots
            .get(screen_num)
            .context("X11 setup missing default screen")?;
        let window = connection
            .generate_id()
            .context("failed to allocate X11 clipboard window")?;
        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &Default::default(),
            )
            .context("failed to create X11 clipboard window")?;
        let atoms = ClipboardAtoms::intern(&connection)?;
        connection.flush().context("failed to flush X11 requests")?;
        Ok(Self {
            connection,
            window,
            atoms,
        })
    }

    fn read_targets(&self) -> Result<Vec<Atom>> {
        let reply = self.read_target_reply(self.atoms.targets)?;
        Ok(reply.value32().map(Iterator::collect).unwrap_or_default())
    }

    fn read_first_text(&self, targets: &[Atom]) -> Result<String> {
        let target = [
            self.atoms.utf8_string,
            self.atoms.text_plain_utf8,
            self.atoms.text_plain,
            self.atoms.string,
        ]
        .into_iter()
        .find(|target| targets.is_empty() || targets.contains(target))
        .unwrap_or(self.atoms.utf8_string);
        let bytes = self.read_target(target)?;
        String::from_utf8(bytes).context("clipboard text was not utf-8")
    }

    fn read_target(&self, target: Atom) -> Result<Vec<u8>> {
        Ok(self.read_target_reply(target)?.value)
    }

    fn read_target_reply(&self, target: Atom) -> Result<GetPropertyReply> {
        self.connection
            .delete_property(self.window, self.atoms.vibe_property)
            .context("failed to clear X11 clipboard property")?;
        self.connection
            .convert_selection(
                self.window,
                self.atoms.clipboard,
                target,
                self.atoms.vibe_property,
                CURRENT_TIME,
            )
            .context("failed to request X11 clipboard conversion")?;
        self.connection.flush().context("failed to flush X11 requests")?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if Instant::now() >= deadline {
                return Err(anyhow!("timed out waiting for X11 clipboard"));
            }
            match self
                .connection
                .poll_for_event()
                .context("failed to poll X11 event")?
            {
                Some(Event::SelectionNotify(event)) if event.selection == self.atoms.clipboard => {
                    if event.property == NONE {
                        return Err(anyhow!("clipboard target is unavailable"));
                    }
                    let reply = self
                        .connection
                        .get_property(
                            false,
                            self.window,
                            event.property,
                            AtomEnum::ANY,
                            0,
                            u32::MAX,
                        )
                        .context("failed to request X11 clipboard property")?
                        .reply()
                        .context("failed to read X11 clipboard property")?;
                    return Ok(reply);
                }
                Some(_) => {}
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

struct ClipboardServer {
    connection: RustConnection,
    window: Window,
    atoms: ClipboardAtoms,
    payload: ClipboardPayload,
}

impl ClipboardServer {
    fn start(display: &str, payload: ClipboardPayload) -> Result<ClipboardOwner> {
        let mut server = Self::connect(display, payload)?;
        server.claim_selection()?;
        let (stop, stop_rx) = mpsc::channel();
        let handle = thread::spawn(move || server.run(stop_rx));
        Ok(ClipboardOwner { stop, handle })
    }

    fn connect(display: &str, payload: ClipboardPayload) -> Result<Self> {
        let (connection, screen_num) = x11rb::connect(Some(display))
            .with_context(|| format!("failed to connect to X11 display {display}"))?;
        let screen = connection
            .setup()
            .roots
            .get(screen_num)
            .context("X11 setup missing default screen")?;
        let window = connection
            .generate_id()
            .context("failed to allocate X11 clipboard owner window")?;
        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::default().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .context("failed to create X11 clipboard owner window")?;
        let atoms = ClipboardAtoms::intern(&connection)?;
        Ok(Self {
            connection,
            window,
            atoms,
            payload,
        })
    }

    fn claim_selection(&self) -> Result<()> {
        self.connection
            .set_selection_owner(self.window, self.atoms.clipboard, CURRENT_TIME)
            .context("failed to set X11 clipboard owner")?;
        self.connection.flush().context("failed to flush X11 requests")?;
        let owner = self
            .connection
            .get_selection_owner(self.atoms.clipboard)
            .context("failed to query X11 clipboard owner")?
            .reply()
            .context("failed to read X11 clipboard owner")?
            .owner;
        if owner != self.window {
            return Err(anyhow!("failed to own X11 clipboard selection"));
        }
        Ok(())
    }

    fn run(&mut self, stop_rx: mpsc::Receiver<()>) {
        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }
            match self.connection.poll_for_event() {
                Ok(Some(Event::SelectionRequest(event))) => {
                    if let Err(err) = self.handle_selection_request(event) {
                        warn!("failed to handle X11 clipboard request: {err}");
                    }
                }
                Ok(Some(Event::SelectionClear(_))) => break,
                Ok(Some(_)) => {}
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(err) => {
                    warn!("failed to poll X11 clipboard owner event: {err}");
                    break;
                }
            }
        }
        let _ = self.connection.destroy_window(self.window);
        let _ = self.connection.flush();
    }

    fn handle_selection_request(&self, event: SelectionRequestEvent) -> Result<()> {
        let property = if event.property == NONE {
            event.target
        } else {
            event.property
        };
        let success = self.write_requested_property(event.requestor, property, event.target)?;
        let notify = SelectionNotifyEvent {
            response_type: x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: event.time,
            requestor: event.requestor,
            selection: event.selection,
            target: event.target,
            property: if success { property } else { NONE },
        };
        self.connection
            .send_event(false, event.requestor, EventMask::NO_EVENT, notify)
            .context("failed to send X11 clipboard selection notification")?;
        self.connection.flush().context("failed to flush X11 requests")?;
        Ok(())
    }

    fn write_requested_property(
        &self,
        requestor: Window,
        property: Atom,
        target: Atom,
    ) -> Result<bool> {
        if target == self.atoms.targets {
            let targets = self.available_targets();
            self.connection
                .change_property32(
                    PropMode::REPLACE,
                    requestor,
                    property,
                    AtomEnum::ATOM,
                    &targets,
                )
                .context("failed to write X11 clipboard TARGETS")?;
            return Ok(true);
        }
        if target == self.atoms.timestamp {
            self.connection
                .change_property32(
                    PropMode::REPLACE,
                    requestor,
                    property,
                    AtomEnum::INTEGER,
                    &[CURRENT_TIME],
                )
                .context("failed to write X11 clipboard TIMESTAMP")?;
            return Ok(true);
        }
        if target == self.atoms.image_png {
            let Some(image_png_b64) = &self.payload.image_png_b64 else {
                return Ok(false);
            };
            let bytes = STANDARD
                .decode(image_png_b64)
                .context("clipboard image was not valid base64")?;
            self.connection
                .change_property8(PropMode::REPLACE, requestor, property, target, &bytes)
                .context("failed to write X11 clipboard image")?;
            return Ok(true);
        }
        if target == self.atoms.utf8_string
            || target == self.atoms.text_plain_utf8
            || target == self.atoms.text_plain
            || target == self.atoms.string
        {
            let text = self.payload.text.as_deref().unwrap_or_default();
            self.connection
                .change_property8(
                    PropMode::REPLACE,
                    requestor,
                    property,
                    target,
                    text.as_bytes(),
                )
                .context("failed to write X11 clipboard text")?;
            return Ok(true);
        }
        Ok(false)
    }

    fn available_targets(&self) -> Vec<Atom> {
        let mut targets = vec![self.atoms.targets, self.atoms.timestamp];
        if self.payload.image_png_b64.is_some() {
            targets.push(self.atoms.image_png);
        }
        if self.payload.text.is_some() || self.payload.image_png_b64.is_none() {
            targets.extend([
                self.atoms.utf8_string,
                self.atoms.text_plain_utf8,
                self.atoms.text_plain,
                self.atoms.string,
            ]);
        }
        targets
    }
}
