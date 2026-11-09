pub mod index;
pub mod menu;

use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const NEW_ID: &str = "__new__";
pub const GREY: &str = "\x1b[38;5;245m";
pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

pub struct Paths {
    pub home: PathBuf,
    pub projects: PathBuf,
    pub cache: PathBuf,
    pub index: PathBuf,
    pub pins: PathBuf,
    pub color_map: PathBuf,
    pub log: PathBuf,
    pub run: PathBuf,
    pub conf: PathBuf,
    pub palette: PathBuf,
    pub bin: PathBuf,
    pub icons: PathBuf,
}

pub fn var_dir(name: &str) -> Option<PathBuf> {
    env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

impl Paths {
    pub fn new() -> Paths {
        let home = var_dir("HOME").unwrap_or_else(|| PathBuf::from("/"));
        let cache = var_dir("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        let state = var_dir("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state"));
        let run = var_dir("XDG_RUNTIME_DIR").unwrap_or_else(|| PathBuf::from("/tmp"));
        let conf = home.join(".config/claude-window");
        Paths {
            projects: home.join(".claude/projects"),
            index: cache.join("claude-launcher/index"),
            cache: cache.join("claude-launcher"),
            pins: state.join("claude-window/epingles.json"),
            color_map: state.join("claude-window/couleurs-conversations.json"),
            log: state.join("claude-launcher.log"),
            run: run.join("claude-couleurs"),
            palette: conf.join("couleurs"),
            bin: home.join(".local/bin"),
            icons: home.join(".local/share/icons"),
            conf,
            home,
        }
    }

    /// `$XDG_RUNTIME_DIR/claude-couleurs/<color><ext>`: pid, .session, .titre, .etat
    pub fn run_file(&self, color: &str, ext: &str) -> PathBuf {
        self.run.join(format!("{color}{ext}"))
    }
}

impl Default for Paths {
    fn default() -> Self {
        Paths::new()
    }
}

pub type Rgb = (u8, u8, u8);
pub const WHITE: Rgb = (255, 255, 255);
pub const BLACK: Rgb = (0, 0, 0);

pub fn fg(c: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
}

pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

pub fn mix(c: Rgb, other: Rgb, t: f64) -> Rgb {
    let m = |a: u8, b: u8| (a as f64 * (1.0 - t) + b as f64 * t).round_ties_even() as u8;
    (m(c.0, other.0), m(c.1, other.1), m(c.2, other.2))
}

pub fn parse_rgb(s: &str) -> Option<Rgb> {
    let mut it = s.split(',').map(|v| v.trim().parse::<u8>());
    Some((it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?))
}

/// Color of the current window, set by claude-window (violet outside of one).
pub fn frame_rgb() -> Rgb {
    env::var("CLAUDE_FRAME_RGB").ok().and_then(|s| parse_rgb(&s)).unwrap_or((139, 92, 246))
}

pub fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn palette(paths: &Paths) -> Vec<(String, Rgb)> {
    let text = fs::read_to_string(&paths.palette).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), parse_rgb(it.next()?)?))
        })
        .collect()
}

fn cmdline(pid: &str) -> Option<String> {
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(String::from_utf8_lossy(&raw).replace('\0', " "))
}

pub struct Window {
    pub color: String,
    pub rgb: Rgb,
    pub state: String,
    pub pid: String,
}

/// Conversations shown in an open Claude window, by session id.
pub fn open_windows(paths: &Paths) -> HashMap<String, Window> {
    let mut out = HashMap::new();
    for (color, rgb) in palette(paths) {
        let Some(pid) = read_trim(paths.run_file(&color, "")) else { continue };
        if !cmdline(&pid).is_some_and(|c| c.contains(&format!("claude-code-{color}"))) {
            continue;
        }
        let Some(session) = read_trim(paths.run_file(&color, ".session")) else { continue };
        let state = read_trim(paths.run_file(&color, ".etat")).unwrap_or_default();
        out.insert(session, Window { color, rgb, state, pid });
    }
    out
}

/// Is this color used by a window (or by a claude-window still starting one)?
pub fn color_taken(paths: &Paths, color: &str) -> bool {
    if let Some(cmd) = read_trim(paths.run_file(color, "")).and_then(|pid| cmdline(&pid))
        && (cmd.contains("claude-window") || cmd.contains(&format!("claude-code-{color} ")))
    {
        return true;
    }
    // windows opened before reservations existed
    let wanted = format!("--desktopfile claude-code-{color} ");
    let me = std::process::id().to_string();
    fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        name != me && cmdline(&name).is_some_and(|c| c.contains(&wanted))
    })
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn local_tm(t: i64) -> libc::tm {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

/// Timestamped line in ~/.local/state/claude-launcher.log, trimmed to its last 500 lines past 200 kB.
pub fn log_line(paths: &Paths, msg: &str) {
    if fs::metadata(&paths.log).is_ok_and(|m| m.len() > 200_000)
        && let Ok(text) = fs::read_to_string(&paths.log)
    {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let _ = fs::write(&paths.log, lines[lines.len().saturating_sub(500)..].concat());
    }
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&paths.log) else { return };
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let tm = local_tm(t.as_secs() as i64);
    let _ = writeln!(
        f,
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} [{}] {msg}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        t.subsec_millis(),
        std::process::id()
    );
}

pub fn short_dir(home: &Path, cwd: &str) -> String {
    if cwd.is_empty() {
        return String::new();
    }
    match Path::new(cwd).strip_prefix(home) {
        Ok(rel) if rel.as_os_str().is_empty() => "~".into(),
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => cwd.to_string(),
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// Own session, no terminal: survives the window closing and never writes into it.
pub fn detach(cmd: &mut Command) -> &mut Command {
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
}

/// Raises and unminimizes the window of process `pid`, through a short KWin script.
pub fn focus(paths: &Paths, pid: &str) -> bool {
    let Ok(pid) = pid.trim().parse::<u32>() else { return false };
    let _ = fs::create_dir_all(&paths.run);
    let js = paths.run.join(format!("activer-{pid}.js"));
    let name = format!("claude-activer-{pid}");
    let script = format!(
        "for (const w of workspace.windowList()) if (w.pid === {pid}) {{ w.minimized = false; workspace.activeWindow = w; }}\n"
    );
    if fs::write(&js, script).is_err() {
        return false;
    }
    let qdbus = |args: &[&str]| {
        let out = Command::new("qdbus6").args(args).stderr(Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let unload = || qdbus(&["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", &name]);
    unload();
    let js = js.to_string_lossy();
    let Some(id) = qdbus(&["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", &js, &name]) else {
        return false;
    };
    qdbus(&["org.kde.KWin", &format!("/Scripting/Script{id}"), "org.kde.kwin.Script.run"]);
    sleep(Duration::from_millis(300));
    unload();
    true
}
