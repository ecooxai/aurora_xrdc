mod process;
use anyhow::{Context, Result, bail};
use process::{Environment, Processes, private_write, probe};
use std::{
    env,
    ffi::OsString,
    fs,
    io::Read,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use x11rb::{connection::Connection, protocol::xtest::ConnectionExt};

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn on_signal(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Auto,
    Yes,
    No,
}
impl Mode {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(Self::Auto),
            "yes" => Ok(Self::Yes),
            "no" => Ok(Self::No),
            _ => bail!("expected auto, yes or no; got {s}"),
        }
    }
}
#[derive(Debug)]
struct Options {
    port: u16,
    https: bool,
    headless: Mode,
    audio: Mode,
    dbus: Mode,
    display: Option<String>,
    launcher: Option<String>,
    password: Option<String>,
    password_file: Option<PathBuf>,
    state_dir: Option<PathBuf>,
    local: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            port: 18443,
            https: true,
            headless: Mode::Auto,
            audio: Mode::Auto,
            dbus: Mode::Auto,
            display: None,
            launcher: None,
            password: None,
            password_file: None,
            state_dir: None,
            local: false,
        }
    }
}
fn parse(args: impl IntoIterator<Item = String>) -> Result<Options> {
    let mut o = Options::default();
    let mut args = args.into_iter().peekable();
    while let Some(raw) = args.next() {
        let (a, inline_value) = match raw.split_once('=') {
            Some((key, value)) if key.starts_with('-') => (key.to_owned(), Some(value.to_owned())),
            _ => (raw, None),
        };
        if a == "--headless"
            && inline_value.is_none()
            && args.peek().is_none_or(|v| v.starts_with('-'))
        {
            o.headless = Mode::Yes;
            continue;
        }
        let v = match inline_value {
            Some(value) => value,
            None => args
                .next()
                .with_context(|| format!("missing value for {a}"))?,
        };
        match a.as_str() {
            "--port" | "-p" => {
                o.port = v.parse()?;
                if o.port == 0 {
                    bail!("port must be 1..65535");
                }
            }
            "--https" => o.https = parse_bool(&v)?,
            "--headless" => o.headless = Mode::parse(&v)?,
            "--audio" => o.audio = Mode::parse(&v)?,
            "--dbus" => o.dbus = Mode::parse(&v)?,
            "--localhost" => o.local = parse_bool(&v)?,
            "--display" => o.display = Some(v),
            "--launcher" => o.launcher = Some(v),
            "--passwd" => o.password = Some(v),
            "--passwd-file" => o.password_file = Some(v.into()),
            "--state-dir" => o.state_dir = Some(v.into()),
            _ => bail!("unknown option {a}; run ./aurora --help"),
        }
    }
    if o.password.is_some() && o.password_file.is_some() {
        bail!("choose --passwd or --passwd-file, not both");
    }
    Ok(o)
}
fn parse_bool(v: &str) -> Result<bool> {
    match v {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => bail!("expected yes or no"),
    }
}
fn random_bytes(size: usize) -> Result<Vec<u8>> {
    let mut v = vec![0; size];
    fs::File::open("/dev/urandom")?.read_exact(&mut v)?;
    Ok(v)
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn auth_record(display: &str, cookie: &[u8]) -> Result<Vec<u8>> {
    let number = display
        .strip_prefix(':')
        .context("private display must be local")?
        .split('.')
        .next()
        .unwrap();
    number.parse::<u16>()?;
    let mut b = 65535u16.to_be_bytes().to_vec(); // FamilyWild, only the random bearer cookie grants access.
    for field in [&b""[..], number.as_bytes(), b"MIT-MAGIC-COOKIE-1", cookie] {
        b.extend_from_slice(&(u16::try_from(field.len())?).to_be_bytes());
        b.extend_from_slice(field);
    }
    let mut hostname = [0u8; 256];
    if unsafe { libc::gethostname(hostname.as_mut_ptr().cast(), hostname.len()) } == 0 {
        let end = hostname
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(hostname.len());
        let mut local = 256u16.to_be_bytes().to_vec();
        for field in [
            &hostname[..end],
            number.as_bytes(),
            b"MIT-MAGIC-COOKIE-1",
            cookie,
        ] {
            local.extend_from_slice(&(u16::try_from(field.len())?).to_be_bytes());
            local.extend_from_slice(field);
        }
        b.extend(local);
    }
    Ok(b)
}
fn check_x(display: &str) -> Result<()> {
    let (c, _) = x11rb::connect(Some(display))?;
    c.xtest_get_version(2, 2)?.reply()?;
    c.flush()?;
    Ok(())
}
fn set(e: &mut Environment, k: &str, v: impl Into<OsString>) {
    e.insert(k.into(), v.into());
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn bus_address_escape(path: &str) -> String {
    path.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"_-/.*".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn display_number(display: &str) -> Result<u16> {
    let number = display
        .strip_prefix(':')
        .context("private display must be local, for example :220")?
        .split('.')
        .next()
        .context("private display number is missing")?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        bail!("invalid private display {display}; expected :N or :N.0");
    }
    number
        .parse::<u16>()
        .with_context(|| format!("invalid private display number in {display}"))
}
fn private_display_number(options: &Options) -> Result<u16> {
    match options.display.as_deref() {
        Some(display) => display_number(display),
        None => Ok(options.port % 1000),
    }
}

fn display_free(n: u16) -> bool {
    !Path::new(&format!("/tmp/.X{n}-lock")).exists()
        && !Path::new(&format!("/tmp/.X11-unix/X{n}")).exists()
}
fn wait_ready(
    p: &mut Processes,
    label: &str,
    mut ready: impl FnMut(&Processes) -> bool,
) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(8) && !STOP.load(Ordering::Relaxed) {
        p.ensure_alive()?;
        if ready(p) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(40));
    }
    bail!(
        "{label} did not become ready; logs: {}",
        p.directory.display()
    )
}
fn x_probe(p: &Processes, exe: &Path, display: &str) -> bool {
    let mut c = p.command(exe);
    c.arg("--probe-display").arg(display);
    probe(c, Duration::from_secs(2)).is_some()
}
fn pulse_probe(p: &Processes, bin: &Path) -> bool {
    let mut c = p.command(bin.join("pactl"));
    c.arg("info");
    probe(c, Duration::from_millis(700)).is_some()
}
fn bus_probe(p: &Processes, bin: &Path) -> bool {
    let mut c = p.command(bin.join("dbus-send"));
    c.args([
        "--session",
        "--print-reply",
        "--reply-timeout=600",
        "--dest=org.freedesktop.DBus",
        "/",
        "org.freedesktop.DBus.ListNames",
    ]);
    probe(c, Duration::from_millis(800)).is_some()
}

pub fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--help" || s == "-h") {
        println!(
            "Aurora portable X11 remote desktop\n\n./aurora --passwd-file /private/password --port 18443\n\n--headless auto|yes|no    Prefer DISPLAY/:0; TinyX fallback (default auto)\n--audio auto|yes|no       Prefer host PulseAudio/PipeWire; private fallback\n--dbus auto|yes|no        Prefer host session bus; private fallback\n--https yes|no           Native TLS (default yes); use no behind HTTPS proxy\n--localhost yes|no       Restrict TCP bind (default no)\n--display :N             Existing display, or exact private display in headless mode\n--launcher none|COMMAND  Override private desktop launcher\n--state-dir PATH         Private per-run logs/runtime parent (mode 700)\n--passwd TEXT            Alternative to --passwd-file (visible in initial argv)\n--probe-display :N       Check X11 and XTEST without starting services\n--version               Print version\n\nNo root, service manager, shell, or runtime package installation is required.\nFallbacks never replace or stop host services. Audio yes / D-Bus yes force private services.\nTinyX provides a software X11 desktop, not XKB/XInput/GLX. Private TinyX defaults to :PORT_LAST_3 (11220 -> :220); --display overrides it. Camera loopback/GPU require host drivers."
        );
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "--version") {
        println!("aurora 0.2.1 portable");
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "--probe-display") {
        return check_x(args.get(1).context("missing display")?);
    }
    let options = parse(args)?;
    let executable = env::current_exe().or_else(|original_error| {
        // /proc is optional in a minimal container. Resolve only our actual argv[0].
        let arg0 = env::args_os().next().ok_or(original_error)?;
        let path = PathBuf::from(&arg0);
        if path.is_absolute() || path.components().count() > 1 {
            return Ok(path);
        }
        for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
            let candidate = directory.join(&arg0);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot locate launcher from argv[0] or PATH",
        ))
    })?;
    let exe = executable.canonicalize()?;
    let root = exe.parent().context("executable directory missing")?;
    let arch = env::consts::ARCH;
    let bin = root.join("vendor").join(arch).join("bin");
    for name in [
        "Xtiny",
        "pulseaudio",
        "pactl",
        "dbus-daemon",
        "dbus-send",
        "ffmpeg",
    ] {
        if !bin.join(name).is_file() {
            bail!("missing vendor/{arch}/bin/{name}; use the complete release archive");
        }
    }
    let server = root.join("bin/vibe_rdesk");
    if !server.is_file() {
        bail!("missing bin/vibe_rdesk; run build-dist.sh first");
    }
    let uid = unsafe { libc::geteuid() };
    if uid == 0 {
        bail!("run Aurora as an unprivileged user, not root");
    }
    let parent = options
        .state_dir
        .clone()
        .unwrap_or_else(|| env::temp_dir().join(format!("aurora-{uid}")));
    if !parent.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&parent)?;
    }
    let metadata = fs::symlink_metadata(&parent)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o077 != 0
    {
        bail!(
            "state directory must be owned by your user, mode 700, and not a symlink: {}",
            parent.display()
        );
    }
    let parent = parent.canonicalize()?;
    // sudo/chroot and some service managers preserve an unusable HOME (for example
    // /root while running as an unprivileged UID). Keep a real user home when it
    // belongs to us; otherwise use the private state parent as a writable fallback.
    let child_home = env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| {
            fs::metadata(home).is_ok_and(|metadata| metadata.is_dir() && metadata.uid() == uid)
        })
        .unwrap_or_else(|| parent.clone());
    let directory = parent.join(format!("session-{}", hex(&random_bytes(8)?)));
    fs::DirBuilder::new().mode(0o700).create(&directory)?;
    let mut p = Processes {
        children: Vec::new(),
        directory,
        env: Environment::new(),
    };
    let mut paths = vec![bin.clone()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    set(&mut p.env, "PATH", env::join_paths(paths)?);
    set(&mut p.env, "HOME", child_home.as_os_str());
    set(&mut p.env, "VIBE_RDESK_MANAGED_RUNTIME", "1");
    set(
        &mut p.env,
        "PULSE_CLIENTCONFIG",
        p.directory.join("client.conf").as_os_str(),
    );
    private_write(
        &p.directory.join("client.conf"),
        b"autospawn = no\nenable-shm = no\n",
    )?;
    let password = match (&options.password, &options.password_file) {
        (Some(s), _) => s.clone(),
        (_, Some(path)) => fs::read_to_string(path)
            .context("reading password file")?
            .trim_end_matches(['\n', '\r'])
            .into(),
        _ => bail!("authentication required: specify --passwd-file or --passwd"),
    };
    if password.trim().is_empty() || password.len() > 4096 {
        bail!("password must be nonempty and at most 4096 bytes");
    }
    let password_file = p.directory.join("server-password");
    private_write(&password_file, password.as_bytes())?;
    unsafe {
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
    }

    let preferred = options
        .display
        .clone()
        .or_else(|| env::var("DISPLAY").ok())
        .unwrap_or_else(|| ":0".into());
    let host_x = options.headless != Mode::Yes && x_probe(&p, &exe, &preferred);
    let display = if host_x {
        println!("[aurora] X11: reuse {preferred}");
        preferred
    } else {
        if options.headless == Mode::No {
            bail!("cannot access host DISPLAY={preferred}; check DISPLAY and XAUTHORITY");
        }
        let n = private_display_number(&options)?;
        if !display_free(n) {
            let source = if options.display.is_some() {
                "explicit --display"
            } else {
                "last three digits of --port"
            };
            bail!(
                "private X11 display :{n} ({source}) is already in use; choose another port or pass --display :N"
            );
        }
        let display = format!(":{n}");
        let auth = p.directory.join("Xauthority");
        private_write(&auth, &auth_record(&display, &random_bytes(16)?)?)?;
        set(&mut p.env, "XAUTHORITY", auth.as_os_str());
        set(&mut p.env, "VIBE_RDESK_HEADLESS_DISPLAY_ACTIVE", "1");
        let mut c = p.command(bin.join("Xtiny"));
        c.args([
            &display,
            "-screen",
            "1280x720x24",
            "-nolisten",
            "tcp",
            "-fp",
            "built-ins",
            "-s",
            "0",
            "-noreset",
            "-auth",
        ])
        .arg(&auth);
        p.spawn("tinyx", c)?;
        wait_ready(&mut p, "TinyX", |p| x_probe(p, &exe, &display))?;
        println!("[aurora] X11: private TinyX {display}");
        display
    };
    set(&mut p.env, "DISPLAY", display.as_str());
    if !host_x {
        set(&mut p.env, "SHELL", bin.join("sh").as_os_str());
    }
    // Target the requested X display, never an unrelated kernel input seat.
    set(
        &mut p.env,
        "VIBE_RDESK_INPUT_BACKEND",
        env::var_os("VIBE_RDESK_INPUT_BACKEND").unwrap_or_else(|| "x11".into()),
    );

    if options.dbus != Mode::No {
        let mut host_bus = options.dbus == Mode::Auto
            && env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
            && bus_probe(&p, &bin);
        if !host_bus
            && options.dbus == Mode::Auto
            && Path::new(&format!("/run/user/{uid}/bus")).exists()
        {
            set(
                &mut p.env,
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path=/run/user/{uid}/bus"),
            );
            host_bus = bus_probe(&p, &bin);
        }
        if host_bus {
            println!("[aurora] D-Bus: reuse host session bus");
        } else {
            let machine_path = p.directory.join("machine-id");
            let machine_id = ["/etc/machine-id", "/var/lib/dbus/machine-id"]
                .iter()
                .filter_map(|path| fs::read_to_string(path).ok())
                .map(|s| s.trim().to_owned())
                .find(|s| s.len() == 32 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                .unwrap_or(hex(&random_bytes(16)?));
            private_write(&machine_path, format!("{machine_id}\n").as_bytes())?;
            set(
                &mut p.env,
                "AURORA_DBUS_MACHINE_ID_FILE",
                machine_path.as_os_str(),
            );
            let address = format!(
                "unix:path={}",
                bus_address_escape(&p.directory.join("bus").to_string_lossy())
            );
            let config = p.directory.join("bus.conf");
            private_write(&config,format!("<busconfig><type>session</type><listen>{}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow user=\"{}\"/><allow send_destination=\"*\" eavesdrop=\"true\"/><allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>",xml(&address),uid).as_bytes())?;
            set(&mut p.env, "DBUS_SESSION_BUS_ADDRESS", address);
            let mut c = p.command(bin.join("dbus-daemon"));
            c.arg("--nofork")
                .arg("--nopidfile")
                .arg(format!("--config-file={}", config.display()));
            p.spawn("dbus", c)?;
            wait_ready(&mut p, "D-Bus", |p| bus_probe(p, &bin))?;
            println!("[aurora] D-Bus: private session bus");
        }
    }

    if options.audio != Mode::No {
        let mut host_audio = options.audio == Mode::Auto && pulse_probe(&p, &bin);
        if !host_audio && options.audio == Mode::Auto {
            for socket in [
                env::var_os("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(format!("/run/user/{uid}")))
                    .join("pulse/native"),
                PathBuf::from(format!("/run/user/{uid}/pulse/native")),
            ] {
                if socket.exists() {
                    set(
                        &mut p.env,
                        "PULSE_SERVER",
                        format!("unix:{}", socket.display()),
                    );
                    host_audio = pulse_probe(&p, &bin);
                    if host_audio {
                        break;
                    }
                }
            }
        }
        if host_audio {
            println!("[aurora] audio: reuse host PulseAudio/PipeWire (routing unchanged)");
        } else {
            let socket = p.directory.join("pulse-native");
            let cookie = p.directory.join("pulse-cookie");
            private_write(&cookie, &random_bytes(256)?)?;
            set(
                &mut p.env,
                "PULSE_SERVER",
                format!("unix:{}", socket.display()),
            );
            set(&mut p.env, "PULSE_COOKIE", cookie.as_os_str());
            set(&mut p.env, "PULSE_RUNTIME_PATH", p.directory.as_os_str());
            set(&mut p.env, "PULSE_STATE_PATH", p.directory.as_os_str());
            let config = p.directory.join("default.pa");
            // PA's script tokenizer accepts quoted paths; state paths containing quotes are rejected below.
            let sock = socket.to_string_lossy();
            let cook = cookie.to_string_lossy();
            if sock.contains(['"', '\n', '\r']) || cook.contains(['"', '\n', '\r']) {
                bail!("state path contains unsupported quotation/control characters");
            }
            private_write(&config,format!("load-module module-native-protocol-unix socket=\"{sock}\" auth-cookie=\"{cook}\" auth-anonymous=0\nload-module module-null-sink sink_name=aurora_output rate=48000 channels=2 sink_properties=device.description=Aurora\nset-default-sink aurora_output\n").as_bytes())?;
            let mut c = p.command(bin.join("pulseaudio"));
            c.args([
                "-n",
                "--daemonize=no",
                "--exit-idle-time=-1",
                "--use-pid-file=no",
                "--realtime=no",
                "--high-priority=no",
                "--log-target=stderr",
                "--file",
            ])
            .arg(&config);
            p.spawn("pulseaudio", c)?;
            wait_ready(&mut p, "PulseAudio", |p| pulse_probe(p, &bin))?;
            println!("[aurora] audio: private static PulseAudio");
        }
    }
    // Desktop applications launched here share the private audio/bus environment.
    if !host_x && options.launcher.as_deref() != Some("none") {
        let c = if let Some(command) = &options.launcher {
            // A custom shell command is an explicit opt-in; default startup never needs a shell.
            let mut c = p.command("/bin/sh");
            c.arg("-c").arg(command);
            c
        } else {
            p.command(bin.join("aurora-wm"))
        };
        p.spawn("desktop", c)?;
    }
    if options.https {
        let cert = env::var_os("VIBE_RDESK_TLS_CERT");
        let key = env::var_os("VIBE_RDESK_TLS_KEY");
        if cert.is_some() != key.is_some() {
            bail!("set both VIBE_RDESK_TLS_CERT and VIBE_RDESK_TLS_KEY");
        }
        if cert.is_none() {
            let identity = wtransport::Identity::self_signed(["localhost", "127.0.0.1", "::1"])?;
            let certpath = p.directory.join("cert.pem");
            let keypath = p.directory.join("key.pem");
            let pem: String = identity
                .certificate_chain()
                .as_slice()
                .iter()
                .map(|c| c.to_pem())
                .collect();
            private_write(&certpath, pem.as_bytes())?;
            private_write(&keypath, identity.private_key().to_secret_pem().as_bytes())?;
            set(&mut p.env, "VIBE_RDESK_TLS_CERT", certpath.as_os_str());
            set(&mut p.env, "VIBE_RDESK_TLS_KEY", keypath.as_os_str());
        }
    }
    let mut runtime = serde_json::Map::new();
    for name in [
        "DISPLAY",
        "XAUTHORITY",
        "DBUS_SESSION_BUS_ADDRESS",
        "AURORA_DBUS_MACHINE_ID_FILE",
        "PULSE_SERVER",
        "PULSE_COOKIE",
    ] {
        if let Some(v) = p
            .env
            .get(std::ffi::OsStr::new(name))
            .cloned()
            .or_else(|| env::var_os(name))
        {
            runtime.insert(
                name.into(),
                serde_json::Value::String(v.to_string_lossy().into()),
            );
        }
    }
    runtime.insert("private_x11".into(), (!host_x).into());
    private_write(
        &p.directory.join("session.json"),
        serde_json::to_string_pretty(&runtime)?.as_bytes(),
    )?;
    let mut c = p.command(server);
    c.arg("--passwd-file")
        .arg(&password_file)
        .arg("--port")
        .arg(options.port.to_string());
    if options.local {
        c.args(["--localhost", "yes"]);
    }
    if !options.https {
        c.env_remove("VIBE_RDESK_TLS_CERT")
            .env_remove("VIBE_RDESK_TLS_KEY");
    }
    p.spawn("server", c)?;
    println!("[aurora] logs/session: {}", p.directory.display());
    println!(
        "[aurora] listening: {}://{}:{}",
        if options.https { "https" } else { "http" },
        if options.local {
            "127.0.0.1"
        } else {
            "0.0.0.0"
        },
        options.port
    );
    while !STOP.load(Ordering::Relaxed) {
        p.ensure_alive()?;
        thread::sleep(Duration::from_millis(100));
    }
    println!("[aurora] stopping owned processes; host services are untouched");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_prefers_host_services() {
        let o = parse(Vec::<String>::new()).unwrap();
        assert_eq!(o.headless, Mode::Auto);
        assert_eq!(o.audio, Mode::Auto);
        assert_eq!(o.dbus, Mode::Auto);
        assert!(o.https);
    }
    #[test]
    fn bare_headless_and_password_file() {
        let o = parse(["--headless", "--passwd-file", "/secret"].map(String::from)).unwrap();
        assert_eq!(o.headless, Mode::Yes);
        assert_eq!(o.password_file, Some(PathBuf::from("/secret")));
    }
    #[test]
    fn invalid_modes_ports_options_rejected() {
        for a in [
            vec!["--port", "0"],
            vec!["--audio", "invalid"],
            vec!["--unknown", "yes"],
            vec!["--passwd"],
        ] {
            assert!(parse(a.into_iter().map(String::from)).is_err());
        }
    }
    #[test]
    fn private_display_defaults_to_last_three_port_digits() {
        for (port, expected) in [
            (11220, 220),
            (18443, 443),
            (9990, 990),
            (10000, 0),
            (65535, 535),
        ] {
            let mut o = Options::default();
            o.port = port;
            assert_eq!(private_display_number(&o).unwrap(), expected);
        }
    }
    #[test]
    fn explicit_private_display_overrides_port_mapping() {
        let mut o = Options::default();
        o.port = 11220;
        o.display = Some(":321.0".into());
        assert_eq!(private_display_number(&o).unwrap(), 321);
        o.display = Some("remote:9".into());
        assert!(private_display_number(&o).is_err());
    }
    #[test]
    fn equals_style_arguments_are_supported() {
        let o =
            parse(["--port=11220", "--headless=yes", "--display=:220"].map(String::from)).unwrap();
        assert_eq!(o.port, 11220);
        assert_eq!(o.headless, Mode::Yes);
        assert_eq!(o.display.as_deref(), Some(":220"));
    }
    #[test]
    fn xauth_cookie_has_correct_wire_format() {
        let b = auth_record(":71", &[7; 16]).unwrap();
        assert_eq!(&b[..4], &[255, 255, 0, 0]);
        assert_eq!(&b[4..8], &[0, 2, b'7', b'1']);
        assert_eq!(&b[b.len() - 16..], &[7; 16]);
        assert!(auth_record("remote:1", &[0; 16]).is_err());
    }
    #[test]
    fn private_bus_paths_are_encoded() {
        assert_eq!(bus_address_escape("/tmp/a b,%"), "/tmp/a%20b%2C%25");
    }
    #[test]
    fn xml_config_values_are_escaped() {
        assert_eq!(xml("a&<\"'"), "a&amp;&lt;&quot;&apos;");
    }
}
