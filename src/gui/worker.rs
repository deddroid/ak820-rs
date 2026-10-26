//! Background thread that talks to the keyboard, so the UI never blocks.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use ak820::device::Connection;
use ak820::lighting::Lighting;
use ak820::profile::{self, KeyColors, Profile, Screen};
use ak820::protocol::SystemSettings;
use ak820::{actions, Error, Keyboard, Model};

use crate::i18n::t;
use crate::tray::Wake;

pub enum Op {
    Lighting(Lighting),
    KeyColors(KeyColors),
    Keymap(Profile),
    Clock,
    System(SystemSettings),
    Screen(Screen),
    Apply(Profile),
    Read(Profile),
    Demo(u64),
    Battery,
    #[cfg(target_os = "linux")]
    InstallUdev,
}

pub struct Job {
    pub title: String,
    pub op: Op,
    /// Saved as the state file if the job succeeds.
    pub new_state: Option<Profile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Searching,
    Wired(Model, u16),
    Receiver,
    Bluetooth,
    NotFound,
    Permission,
    Error(String),
}

pub enum Msg {
    Status(Status),
    Log { text: String, error: bool },
    Progress(f32),
    Battery(Option<u8>),
    Done { ok: bool, new_state: Option<Profile> },
}

struct Worker {
    tx: Sender<Msg>,
    wake: Wake,
    last: Option<Status>,
    kb: Option<Keyboard>,
}

pub fn spawn(rx: Receiver<Job>, tx: Sender<Msg>, wake: Wake) {
    std::thread::spawn(move || {
        let mut w = Worker { tx, wake, last: None, kb: None };
        w.probe();
        loop {
            match rx.recv_timeout(Duration::from_millis(1500)) {
                Ok(job) => {
                    w.run(job);
                }
                Err(RecvTimeoutError::Timeout) => {
                    if w.kb.is_none() {
                        w.probe();
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

fn status_of(r: &Result<Keyboard, Error>) -> Status {
    match r {
        Ok(kb) => match kb.connection() {
            Connection::Wired => Status::Wired(kb.model(), kb.pid()),
            Connection::Receiver => Status::Receiver,
        },
        Err(Error::Permission(_)) => Status::Permission,
        Err(Error::NotFound(m)) if m.contains("Bluetooth") => Status::Bluetooth,
        Err(Error::NotFound(_)) => Status::NotFound,
        Err(e) => Status::Error(e.to_string()),
    }
}

impl Worker {
    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
        (self.wake)();
    }

    fn log(&self, text: impl Into<String>) {
        self.send(Msg::Log { text: text.into(), error: false });
    }

    fn probe(&mut self) {
        if self.kb.is_none() {
            let res = Keyboard::open();
            let s = status_of(&res);
            if let Ok(kb) = res {
                self.kb = Some(kb);
            }
            if self.last.as_ref() != Some(&s) {
                self.last = Some(s.clone());
                self.send(Msg::Status(s));
            }
        }
    }

    fn run(&mut self, mut job: Job) {
        self.log(format!("{}…", job.title));
        let result = self.execute(&mut job);
        let ok = result.is_ok();
        match result {
            Ok(()) => {
                if let Some(state) = &job.new_state {
                    if let Err(e) = state.save(&profile::state_path()) {
                        self.send(Msg::Log {
                            text: format!("{}: {e}", t("Could not save the state", "Impossibile salvare lo stato")),
                            error: true,
                        });
                    }
                }
                self.log(format!("{}: {}", job.title, t("done", "fatto")));
            }
            Err(e) => self.send(Msg::Log { text: format!("{}: {e}", job.title), error: true }),
        }
        self.send(Msg::Progress(0.0));
        self.send(Msg::Done { ok, new_state: job.new_state.filter(|_| ok) });
    }

    fn execute(&mut self, job: &mut Job) -> Result<(), Error> {
        #[cfg(target_os = "linux")]
        if let Op::InstallUdev = job.op {
            return install_udev();
        }
        if self.kb.is_none() {
            self.kb = Some(Keyboard::open()?);
        }
        let kb = self.kb.as_ref().unwrap();
        let progress = |i: usize, n: usize| self.send(Msg::Progress(i as f32 / n as f32));
        let result = match &job.op {
            Op::Lighting(l) => kb.set_lighting(l),
            Op::KeyColors(c) => actions::send_key_colors(kb, c),
            Op::Keymap(p) => actions::send_keymap(kb, p),
            Op::Clock => kb.set_clock(&actions::now()),
            Op::System(s) => kb.set_system(s),
            Op::Screen(screen) => {
                let frames = actions::load_frames(&screen.image, screen.fit, screen.crop)?;
                actions::upload_frames(kb, &frames, progress)
            }
            Op::Apply(p) => actions::apply(kb, p, &mut |m| self.log(format!("  {m}")), &mut |i, n| progress(i, n)),
            Op::Read(current) => {
                let mut read_prof = kb.read_profile()?;
                read_prof.macros = current.macros.clone();
                let mut merged = current.clone();
                merged.merge(&read_prof);
                job.new_state = Some(merged);
                Ok(())
            }
            Op::Demo(secs) => {
                let secs = *secs;
                let grid = actions::key_grid_for(kb.model());
                let start = Instant::now();
                let total = Duration::from_secs(secs);
                while start.elapsed() < total {
                    let t = start.elapsed().as_secs_f32();
                    kb.live_frame(&actions::rainbow_frame(&grid, t))?;
                    self.send(Msg::Progress(t / secs as f32));
                }
                Ok(())
            }
            Op::Battery => {
                let b = kb.battery()?;
                self.send(Msg::Battery(b));
                Ok(())
            }
            #[cfg(target_os = "linux")]
            Op::InstallUdev => unreachable!(),
        };
        if result.is_err() {
            self.kb = None;
            self.last = None;
            self.send(Msg::Status(Status::NotFound));
        }
        result
    }
}

/// Install the udev rule through polkit (asks for the password graphically).
#[cfg(target_os = "linux")]
fn install_udev() -> Result<(), Error> {
    const RULES: &str = include_str!("../../udev/70-ajazz-ak820.rules");
    let tmp = std::env::temp_dir().join("70-ajazz-ak820.rules");
    std::fs::write(&tmp, RULES)?;
    let status = std::process::Command::new("pkexec")
        .args([
            "sh",
            "-c",
            "install -m 0644 \"$1\" /etc/udev/rules.d/70-ajazz-ak820.rules && udevadm control --reload && udevadm trigger --subsystem-match=hidraw",
            "sh",
        ])
        .arg(&tmp)
        .status()
        .map_err(|e| Error::Unsupported(format!("pkexec not available ({e}): install the rule from a terminal")))?;
    let _ = std::fs::remove_file(&tmp);
    if !status.success() {
        return Err(Error::Unsupported("installation cancelled or failed".into()));
    }
    // Give udev a moment to apply the new permissions.
    std::thread::sleep(Duration::from_millis(800));
    Ok(())
}
