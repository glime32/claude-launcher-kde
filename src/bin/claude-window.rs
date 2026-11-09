//! Opens a Claude Konsole window in a color no other window uses.
//!
//!   claude-window                  first free color, then the menu
//!   claude-window --libre C        exit 0 if color C is free, 1 if taken
//!   claude-window --couleur C      C if it is free (else the first free one)
//!   claude-window --reprendre ID   open conversation ID directly, without the menu
//!
//! The color is reserved in $XDG_RUNTIME_DIR/claude-couleurs/<color> (our pid, which
//! becomes Konsole's through exec).

use claude_launcher::{Paths, color_taken, hex, palette, which};
use std::env;
use std::fs::{self, File};
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Konsole runs from a copy named claude-konsole. KWin names a window after its
/// executable, and the taskbar files every window named "konsole" under Konsole (its
/// .desktop says StartupWMClass=konsole) whatever its app id. Under another name each
/// window gets its own entry, with the icon of claude-code-<color>.desktop.
/// The copy (51 kB) is refreshed whenever Konsole is updated.
fn konsole_binary(paths: &Paths) -> PathBuf {
    let Some(src) = which("konsole").and_then(|p| fs::canonicalize(p).ok()) else {
        return PathBuf::from("konsole");
    };
    let dest = paths.cache.join("claude-konsole");
    let stamp = |p: &Path| fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok()));
    if stamp(&src) != stamp(&dest) && copy_binary(&src, &dest).is_err() {
        return src;
    }
    dest
}

fn copy_binary(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest.parent().unwrap_or(Path::new("/")))?;
    let tmp = dest.with_extension("tmp");
    fs::copy(src, &tmp)?;
    File::options().write(true).open(&tmp)?.set_modified(fs::metadata(src)?.modified()?)?;
    fs::rename(&tmp, dest)
}

fn main() {
    let paths = Paths::new();
    let _ = fs::create_dir_all(&paths.run);
    let args: Vec<String> = env::args().skip(1).collect();
    let (mut wanted, mut resume) = (None, None);
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).cloned();
        match args[i].as_str() {
            "--libre" => exit(if color_taken(&paths, value.as_deref().unwrap_or("")) { 1 } else { 0 }),
            "--couleur" => wanted = value,
            "--reprendre" => resume = value,
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }

    let lock = File::create(paths.run.join(".verrou")).ok();
    if let Some(f) = &lock {
        let deadline = Instant::now() + Duration::from_secs(5);
        while f.try_lock().is_err() && Instant::now() < deadline {
            sleep(Duration::from_millis(50));
        }
    }
    let colors = palette(&paths);
    let free = |name: &String| !color_taken(&paths, name);
    let pick = wanted
        .and_then(|w| colors.iter().find(|(n, _)| *n == w))
        .filter(|(n, _)| free(n))
        .or_else(|| colors.iter().find(|(n, _)| free(n)));
    let Some((name, rgb)) = pick.cloned() else {
        drop(lock);
        let _ = Command::new("notify-send")
            .args(["-a", "Claude Code", "-i"])
            .arg(paths.icons.join("claude-code.png"))
            .arg("Claude Code")
            .arg(format!(
                "Toutes les couleurs sont déjà prises ({} fenêtres Claude ouvertes). Ferme-en une pour en ouvrir une autre.",
                colors.len()
            ))
            .status();
        exit(1);
    };
    let _ = fs::write(paths.run_file(&name, ""), format!("{}\n", std::process::id()));
    let konsole = konsole_binary(&paths); // under the lock: two windows may start at once
    drop(lock);

    // Konsole settings for these windows only (the launcher restores the original for Claude)
    let orig_dirs = env::var("XDG_CONFIG_DIRS").unwrap_or_default();
    let base_dirs = if orig_dirs.is_empty() { "/etc/xdg" } else { &orig_dirs };
    let defaults = paths.conf.join("defaults");
    let mut cmd = Command::new(&konsole);
    cmd.env("CLAUDE_FRAME_COLOR", &name)
        .env("CLAUDE_FRAME_RGB", format!("{},{},{}", rgb.0, rgb.1, rgb.2))
        .env("CLAUDE_ORIG_XDG_CONFIG_DIRS", &orig_dirs)
        .env("XDG_CONFIG_DIRS", format!("{}:{base_dirs}", defaults.display()));
    match &resume {
        Some(id) => cmd.env("CLAUDE_RESUME_SID", id),
        None => cmd.env_remove("CLAUDE_RESUME_SID"),
    };
    let style = paths.conf.join(format!("style-{name}.qss"));
    let icon = paths.icons.join(format!("claude-code-{name}.png"));
    cmd.arg("--separate")
        .arg(format!("-stylesheet={}", style.display()))
        .args(["--desktopfile", &format!("claude-code-{name}")])
        .args(["--hide-toolbars", "--show-tabbar", "--notransparency"])
        .args(["-p", "ScrollBarPosition=2", "-p", "UseCustomCursorColor=true"])
        .args(["-p", &format!("CustomCursorColor={}", hex(rgb))])
        .args(["-p", "LocalTabTitleFormat=%w", "-p", "RemoteTabTitleFormat=%w"])
        .args(["-p", "DimWhenInactive=true", "-p", "DimmValue=40"])
        .args(["-p", &format!("Icon={}", icon.display())])
        .arg("--workdir")
        .arg(&paths.home)
        .arg("-e")
        .arg(paths.bin.join("claude-launcher"));
    let err = cmd.exec();
    eprintln!("claude-window: konsole: {err}");
    exit(1);
}
