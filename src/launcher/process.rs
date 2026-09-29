use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Read,
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub type Environment = BTreeMap<OsString, OsString>;
pub struct ChildProcess {
    pub name: String,
    pub child: Child,
    pub log: PathBuf,
}
pub struct Processes {
    pub children: Vec<ChildProcess>,
    pub directory: PathBuf,
    pub env: Environment,
}
impl Processes {
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut c = Command::new(program);
        c.envs(&self.env);
        c
    }
    pub fn spawn(&mut self, name: &str, mut command: Command) -> Result<u32> {
        let path = self.directory.join(format!("{name}.log"));
        let log = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        command
            .envs(&self.env)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .process_group(0);
        let child = command
            .spawn()
            .with_context(|| format!("starting {name}; see {}", path.display()))?;
        let id = child.id();
        self.children.push(ChildProcess {
            name: name.into(),
            child,
            log: path,
        });
        Ok(id)
    }
    pub fn ensure_alive(&mut self) -> Result<()> {
        for p in &mut self.children {
            if let Some(status) = p.child.try_wait()? {
                let mut tail = String::new();
                if let Ok(mut file) = File::open(&p.log) {
                    let _ = Read::by_ref(&mut file)
                        .take(16_384)
                        .read_to_string(&mut tail);
                }
                bail!("{} exited ({status}); {}\n{tail}", p.name, p.log.display());
            }
        }
        Ok(())
    }
}
impl Drop for Processes {
    fn drop(&mut self) {
        // All groups below were created by this launcher. Never signal host X/audio/D-Bus.
        for p in self.children.iter_mut().rev() {
            unsafe {
                libc::kill(-(p.child.id() as i32), libc::SIGTERM);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut live = false;
            for p in &mut self.children {
                live |= matches!(p.child.try_wait(), Ok(None));
            }
            if !live || Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        for p in self.children.iter_mut().rev() {
            // The child PID cannot be reused until wait() reaps it. Descendant groups are owned.
            if matches!(p.child.try_wait(), Ok(None)) {
                unsafe {
                    libc::kill(-(p.child.id() as i32), libc::SIGKILL);
                }
                let _ = p.child.wait();
            }
        }
    }
}

pub fn probe(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut result = String::new();
                child
                    .stdout
                    .take()?
                    .take(32_768)
                    .read_to_string(&mut result)
                    .ok()?;
                return Some(result);
            }
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}
