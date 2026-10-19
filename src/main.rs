use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::NaiveDateTime;
use clap::{Parser, Subcommand};

use ak820::device::{list_interfaces, Connection};
use ak820::lighting::{Direction, Lighting, Mode};
use ak820::macros::{MacroDef, MacroMode};
use ak820::profile::{self, FnMode, KeyColors, Keymap, Profile, Screen, SleepTime, System};
use ak820::protocol;
use ak820::screen::{self, Fit};
use ak820::{actions, keys, Keyboard, Rgb};

#[derive(Parser)]
#[command(name = "ak820", version, about = "Configure the AJAZZ AK820 Pro keyboard (Linux & Windows)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the keyboard's HID interfaces
    List,
    /// Built-in lighting effect
    Light {
        /// off, static, breath, rolling (wave), spectrum, ripples, ... (see `ak820 keys`)
        mode: String,
        /// RRGGBB or a color name
        #[arg(short, long)]
        color: Option<Rgb>,
        /// Cycle colors instead of using --color
        #[arg(short, long)]
        rainbow: bool,
        #[arg(short, long, default_value_t = 5, value_parser = clap::value_parser!(u8).range(0..=5))]
        brightness: u8,
        #[arg(short, long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=5))]
        speed: u8,
        /// right/left (rolling, rotating, flowing, tilt) or down/up (scrolling)
        #[arg(short, long, value_parser = parse_direction)]
        direction: Option<Direction>,
    },
    /// Static per-key colors, e.g. `ak820 colors --base 000020 w=red a=red s=red d=red`
    Colors {
        /// KEY=COLOR (COLOR may be `default` to use the base color again)
        assign: Vec<String>,
        /// Color of every other key
        #[arg(long)]
        base: Option<Rgb>,
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(1..=5))]
        brightness: Option<u8>,
        /// Forget every per-key color first
        #[arg(long)]
        reset: bool,
    },
    /// Set the screen clock (to now, or --at "YYYY-MM-DD HH:MM[:SS]")
    Clock {
        #[arg(long)]
        at: Option<String>,
    },
    /// Upload an image or animated GIF to the 128×128 screen
    Screen {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = Fit::Crop)]
        fit: Fit,
    },
    /// Sleep timer, Fn mode, debounce, key blocking (unset options keep their last value)
    System {
        #[arg(long, value_enum)]
        sleep: Option<SleepTime>,
        #[arg(long, value_enum)]
        fn_mode: Option<FnMode>,
        /// Debounce 1 (fast) .. 5 (slow)
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=5))]
        response: Option<u8>,
        #[arg(long, value_name = "on|off", value_parser = parse_onoff)]
        block_win: Option<bool>,
        #[arg(long, value_name = "on|off", value_parser = parse_onoff)]
        block_alt_f4: Option<bool>,
        #[arg(long, value_name = "on|off", value_parser = parse_onoff)]
        block_alt_tab: Option<bool>,
    },
    /// Remap keys inside the keyboard, e.g. `ak820 remap pgdn=end capslock=ctrl`
    Remap {
        /// KEY=TARGET; with no arguments, show the current remapping
        assign: Vec<String>,
        /// Edit the Fn layer
        #[arg(long = "fn")]
        fn_layer: bool,
        /// Restore the factory layout of the selected layer first
        #[arg(long)]
        reset: bool,
    },
    /// Manage named macros (used with `remap KEY=macro:NAME`)
    Macro {
        #[command(subcommand)]
        action: MacroAction,
    },
    /// Apply a TOML profile (see examples/profile.toml)
    Apply {
        file: PathBuf,
        /// Only validate the profile
        #[arg(long)]
        check: bool,
    },
    /// Write the remembered configuration as a profile
    Export { file: Option<PathBuf> },
    /// Send the remembered configuration again
    Resend,
    /// Factory settings: no remaps or macros, rainbow wave, default system settings
    Reset {
        #[arg(long)]
        yes: bool,
    },
    /// Battery level (2.4 GHz receiver only)
    Battery,
    /// List key names, remap targets and effects
    Keys,
    /// Live rainbow animation (not saved), to test per-key LEDs
    Demo {
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
}

#[derive(Subcommand)]
enum MacroAction {
    /// Create or replace a macro, e.g. `ak820 macro set sig "text:Hi," "tap:enter"`
    Set {
        name: String,
        /// Steps: text:..., tap:ctrl+c, down:KEY, up:KEY, delay:MS, click:left, mouse-down:left, mouse-up:left
        #[arg(required = true)]
        steps: Vec<String>,
        #[arg(long, default_value = "once", value_parser = parse_macro_mode)]
        mode: MacroMode,
        /// Repetitions for --mode repeat
        #[arg(long, default_value_t = 1)]
        count: u8,
    },
    /// Delete a macro
    Rm { name: String },
    /// List macros
    List,
}

fn parse_direction(s: &str) -> Result<Direction, String> {
    Direction::from_name(s).ok_or_else(|| "use left, right, up or down".into())
}

fn parse_onoff(s: &str) -> Result<bool, String> {
    match s {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => Err("use on or off".into()),
    }
}

fn parse_macro_mode(s: &str) -> Result<MacroMode, String> {
    MacroMode::from_name(s).ok_or_else(|| "use once, repeat, toggle, hold or step".into())
}

fn split_assign(a: &str) -> Result<(&str, &str)> {
    a.split_once('=')
        .filter(|(k, _)| !k.is_empty())
        .with_context(|| format!("'{a}' must look like KEY=VALUE"))
}

fn open() -> Result<Keyboard> {
    let kb = Keyboard::open()?;
    let how = match kb.connection() {
        Connection::Wired => "USB cable",
        Connection::Receiver => "2.4 GHz receiver",
    };
    eprintln!("AK820 Pro (PID {:04x}) via {how}", kb.pid());
    Ok(kb)
}

fn save_state(state: &Profile) -> Result<()> {
    state.save(&profile::state_path()).context("cannot save the state file")
}

fn send_keymap(kb: &Keyboard, state: &Profile) -> Result<()> {
    Ok(actions::send_keymap(kb, state)?)
}

fn send_colors(kb: &Keyboard, c: &KeyColors) -> Result<()> {
    Ok(actions::send_key_colors(kb, c)?)
}

fn upload_screen(kb: &Keyboard, frames: &[screen::Frame]) -> Result<()> {
    eprintln!("Uploading {} frame(s)...", frames.len());
    actions::upload_frames(kb, frames, progress)?;
    eprintln!();
    Ok(())
}

fn progress(i: usize, n: usize) {
    eprint!("\r  {:3}%", i * 100 / n);
    let _ = std::io::stderr().flush();
}

fn apply(kb: &Keyboard, p: &Profile) -> Result<()> {
    actions::apply(kb, p, &mut |m| eprintln!("- {m}"), &mut progress)?;
    eprintln!();
    Ok(())
}

fn print_keymap(state: &Profile) {
    let km = state.keymap.clone().unwrap_or_default();
    for (title, map) in [("Normal layer", &km.normal), ("Fn layer", &km.fn_layer)] {
        println!("{title}:");
        if map.is_empty() {
            println!("  (factory)");
        }
        for (k, t) in map {
            println!("  {k} = {t}");
        }
    }
    if !state.macros.is_empty() {
        println!("Macros: {}", state.macros.keys().cloned().collect::<Vec<_>>().join(", "));
    }
}

fn print_keys() {
    let names: Vec<String> = keys::all_lights()
        .map(|(n, _)| n.to_string())
        .collect();
    println!("Physical keys (remap sources, per-key colors):\n  {}\n", names.join(" "));
    println!("Remap targets:");
    println!("  key            a..z 0..9 f1..f24 enter esc backspace tab space minus equal lbracket");
    println!("                 rbracket backslash semicolon quote grave comma period slash capslock");
    println!("                 printscreen scrolllock pause insert home end pgup pgdn delete");
    println!("                 up down left right numlock menu");
    println!("  modifiers      ctrl shift alt win rctrl rshift ralt rwin  (alone or as ctrl+shift+t)");
    let media: Vec<&str> = keys::CONSUMER.iter().map(|(n, _)| *n).collect();
    println!("  media          {}", media.join(" "));
    println!("  mouse          mouse:left mouse:right mouse:middle wheel:up wheel:down");
    println!("  multi-key      multi:a+s+d   (up to 3 keys at once)");
    println!("  macro          text:Hello   macro:NAME   (see `ak820 macro`)");
    println!("  factory        default");
    let modes: Vec<&str> = Mode::ALL.iter().map(|m| m.name()).collect();
    println!("\nLighting effects:\n  {}", modes.join(" "));
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let mut state = profile::load_state();
    match cli.command {
        Command::List => {
            let list = list_interfaces()?;
            if list.is_empty() {
                bail!("no AK820 device (VID 0c45) found");
            }
            for i in list {
                let role = match (i.pid, i.usage_page) {
                    (_, protocol::CONFIG_USAGE_PAGE) => "  <- configuration",
                    (_, protocol::DATA_USAGE_PAGE) => "  <- screen data",
                    (protocol::RECEIVER_PID, protocol::RECEIVER_USAGE_PAGE) => "  <- 2.4G receiver",
                    _ => "",
                };
                println!(
                    "{:04x}  if {}  usage page {:04x}:{:04x}  {}  {}{role}",
                    i.pid, i.interface, i.usage_page, i.usage, i.product, i.path
                );
            }
        }
        Command::Light { mode, color, rainbow, brightness, speed, direction } => {
            let mode = Mode::from_name(&mode).with_context(|| format!("unknown effect '{mode}' (see `ak820 keys`)"))?;
            let direction = direction.unwrap_or_default();
            if let Some(axis) = mode.direction_axis() {
                if direction.on_axis(axis) != direction {
                    let [a, b] = axis.directions();
                    bail!("{} moves {a:?} or {b:?}", mode.name());
                }
            }
            let l = Lighting {
                mode,
                color: color.unwrap_or(Rgb(255, 255, 255)),
                rainbow: rainbow || (color.is_none() && mode != Mode::Static),
                brightness,
                speed,
                direction,
            };
            let kb = open()?;
            if mode == Mode::Custom {
                send_colors(&kb, &state.key_colors.clone().unwrap_or_default())?;
            } else {
                kb.set_lighting(&l)?;
            }
            state.lighting = Some(l);
            save_state(&state)?;
            println!("Lighting set: {}", mode.name());
        }
        Command::Colors { assign, base, brightness, reset } => {
            let mut c = if reset { KeyColors::default() } else { state.key_colors.clone().unwrap_or_default() };
            if let Some(b) = base {
                c.base = b;
            }
            if let Some(b) = brightness {
                c.brightness = b;
            }
            for a in &assign {
                let (k, v) = split_assign(a)?;
                let k = keys::position(k)
                    .and_then(keys::canonical_name)
                    .with_context(|| format!("unknown key '{k}' (see `ak820 keys`)"))?;
                if v.eq_ignore_ascii_case("default") {
                    c.keys.remove(k);
                } else {
                    c.keys.insert(k.to_string(), v.parse()?);
                }
            }
            let kb = open()?;
            send_colors(&kb, &c)?;
            state.key_colors = Some(c);
            state.lighting = Some(Lighting::new(Mode::Custom));
            save_state(&state)?;
            println!("Per-key colors set.");
        }
        Command::Clock { at } => {
            let t = match at {
                Some(s) => actions::to_clock(
                    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M"))
                        .context("use --at \"YYYY-MM-DD HH:MM[:SS]\"")?,
                ),
                None => actions::now(),
            };
            open()?.set_clock(&t)?;
            println!(
                "Clock set to {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                t.year, t.month, t.day, t.hour, t.minute, t.second
            );
        }
        Command::Screen { file, fit } => {
            let frames = actions::load_frames(&file, fit)?;
            let kb = open()?;
            upload_screen(&kb, &frames)?;
            let image = std::fs::canonicalize(&file).unwrap_or(file);
            state.screen = Some(Screen { image, fit });
            save_state(&state)?;
            println!("Screen updated.");
        }
        Command::System { sleep, fn_mode, response, block_win, block_alt_f4, block_alt_tab } => {
            let mut s = state.system.unwrap_or_default();
            s.sleep = sleep.unwrap_or(s.sleep);
            s.fn_mode = fn_mode.unwrap_or(s.fn_mode);
            s.response = response.unwrap_or(s.response);
            s.block_win = block_win.unwrap_or(s.block_win);
            s.block_alt_f4 = block_alt_f4.unwrap_or(s.block_alt_f4);
            s.block_alt_tab = block_alt_tab.unwrap_or(s.block_alt_tab);
            open()?.set_system(&s.settings()?)?;
            state.system = Some(s);
            save_state(&state)?;
            println!("{}", toml::to_string_pretty(&s)?);
        }
        Command::Remap { assign, fn_layer, reset } => {
            if assign.is_empty() && !reset {
                print_keymap(&state);
                return Ok(());
            }
            let mut km = state.keymap.clone().unwrap_or_default();
            let layer = if fn_layer { &mut km.fn_layer } else { &mut km.normal };
            if reset {
                layer.clear();
            }
            for a in &assign {
                let (k, t) = split_assign(a)?;
                keys::check_remappable(k, fn_layer)?;
                let k = keys::canonical_name(keys::position(k).unwrap()).unwrap().to_string();
                if t.eq_ignore_ascii_case("default") {
                    layer.remove(&k);
                } else {
                    layer.insert(k, t.to_string());
                }
            }
            let mut next = state.clone();
            next.keymap = Some(km);
            let kb = open()?;
            send_keymap(&kb, &next)?;
            state = next;
            save_state(&state)?;
            print_keymap(&state);
        }
        Command::Macro { action } => match action {
            MacroAction::Set { name, steps, mode, count } => {
                let def = MacroDef { steps, mode, count };
                ak820::macros::events(&def.steps)?;
                state.macros.insert(name.clone(), def);
                let used = state
                    .keymap
                    .iter()
                    .flat_map(|k| k.normal.values().chain(k.fn_layer.values()))
                    .any(|t| t == &format!("macro:{name}"));
                if used {
                    send_keymap(&open()?, &state)?;
                }
                save_state(&state)?;
                println!("Macro '{name}' saved{}.", if used { " and sent" } else { "; assign it with `ak820 remap KEY=macro:NAME`" });
            }
            MacroAction::Rm { name } => {
                let target = format!("macro:{name}");
                if let Some(k) = state
                    .keymap
                    .iter()
                    .flat_map(|k| k.normal.iter().chain(k.fn_layer.iter()))
                    .find(|(_, t)| **t == target)
                {
                    bail!("macro '{name}' is used by key '{}': remap it first", k.0);
                }
                if state.macros.remove(&name).is_none() {
                    bail!("no macro named '{name}'");
                }
                save_state(&state)?;
            }
            MacroAction::List => {
                for (name, m) in &state.macros {
                    println!("{name} ({:?} x{}): {}", m.mode, m.count, m.steps.join("  "));
                }
            }
        },
        Command::Apply { file, check } => {
            let p = Profile::load(&file)?;
            p.validate()?;
            if check {
                println!("{} is valid.", file.display());
                return Ok(());
            }
            let kb = open()?;
            apply(&kb, &p)?;
            state.merge(&p);
            save_state(&state)?;
            println!("Profile applied.");
        }
        Command::Export { file } => {
            let text = toml::to_string_pretty(&state)?;
            match file {
                Some(f) => std::fs::write(&f, text).with_context(|| format!("cannot write {}", f.display()))?,
                None => print!("{text}"),
            }
        }
        Command::Resend => {
            let kb = open()?;
            let mut p = state.clone();
            p.sync_clock = Some(true);
            if p.keymap.is_none() && !p.macros.is_empty() {
                p.keymap = Some(Keymap::default());
            }
            apply(&kb, &p)?;
            println!("Configuration sent again.");
        }
        Command::Reset { yes } => {
            if !yes {
                println!(
                    "This removes every remap and macro, restores the rainbow wave and the factory \
                     system settings.\nRun `ak820 reset --yes` to do it. (On the keyboard: hold Fn+Space 3-5 s.)"
                );
                return Ok(());
            }
            let p = Profile {
                sync_clock: Some(true),
                lighting: Some(Lighting::factory()),
                system: Some(System::default()),
                keymap: Some(Keymap::default()),
                ..Profile::default()
            };
            let kb = open()?;
            apply(&kb, &p)?;
            let backup = profile::state_path().with_extension("before-reset.toml");
            let _ = state.save(&backup);
            save_state(&p)?;
            println!("Factory settings restored (previous state saved in {}).", backup.display());
        }
        Command::Battery => match open()?.battery()? {
            Some(v) => println!("Battery: {v}%"),
            None => println!("Battery level unknown (keyboard asleep or not reporting it)."),
        },
        Command::Keys => print_keys(),
        Command::Demo { seconds } => {
            let kb = open()?;
            let grid = actions::key_grid();
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(seconds) {
                let t = start.elapsed().as_secs_f32();
                kb.live_frame(&actions::rainbow_frame(&grid, t))?;
            }
            println!("Done: the keyboard returns to its saved effect by itself.");
        }
    }
    Ok(())
}
