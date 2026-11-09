//! Claude Code hook, in the windows opened from the menu:
//!   claude-hook travail   prompt sent, tool used
//!   claude-hook fini      Claude finished its answer
//!   claude-hook attente   Claude asks for a permission
//! Writes the state shown in the title bar and the panel, and notifies when the window
//! isn't the one you are looking at. Must never print anything: Claude would add it
//! to the conversation.

use claude_launcher::{Paths, detach, focus, read_trim, record_window};
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn run(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Screen locked or session idle: you are not at the computer.
fn away() -> bool {
    let session = env::var("XDG_SESSION_ID").unwrap_or_default();
    run("loginctl", &["show-session", &session, "-p", "LockedHint", "-p", "IdleHint"]).contains("=yes")
        || run("qdbus6", &["org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver.GetActive"]) == "true"
}

fn focused(konsole: &str) -> bool {
    !konsole.is_empty()
        && run(
            "qdbus6",
            &[
                &format!("org.kde.konsole-{konsole}"),
                "/konsole/MainWindow_1",
                "org.freedesktop.DBus.Properties.Get",
                "org.qtproject.Qt.QWidget",
                "isActiveWindow",
            ],
        ) == "true"
}

/// Phone notification through ntfy, same channel as the Claude usage widget.
fn push(paths: &Paths, title: &str, body: &str, done: bool) {
    let cfg: Value = fs::read(paths.home.join(".config/claude-usage/config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let topic = cfg["ntfy_topic"].as_str().unwrap_or("");
    if topic.is_empty() {
        return;
    }
    let server = cfg["ntfy_server"].as_str().unwrap_or("https://ntfy.sh");
    let tag = if done { "white_check_mark" } else { "warning" };
    let payload = json!({ "topic": topic, "title": title, "message": body, "tags": [tag] }).to_string();
    let child = detach(Command::new("curl").args(["-s", "-m", "10", "-o", "/dev/null", "-d", "@-", server]))
        .stdin(Stdio::piped())
        .spawn();
    if let Ok(mut child) = child
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = stdin.write_all(payload.as_bytes());
    }
}

/// How long the « Afficher » button keeps working. Plasma never closes a notification
/// that went into the history, so `notify-send -w` would otherwise stay alive with this
/// process (≈ 11 MB together) until it is closed by hand. Past this delay notify-send is
/// killed: the notification stays in the history, its button just does nothing anymore.
const CLICK_WAIT: Duration = Duration::from_secs(120);

/// Desktop notification; its « Afficher » button raises the window. Runs detached so
/// Claude doesn't wait for the click.
fn notifier(paths: &Paths, args: &[String]) {
    let [title, body, konsole] = args else { return };
    let icon = paths.icons.join("claude-code.png");
    let Ok(mut child) = Command::new("notify-send")
        .args(["-a", "Claude Code", "-i"])
        .arg(icon)
        .args(["-A", "afficher=Afficher", "-w", "--", title, body])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let Some(mut stdout) = child.stdout.take() else { return };
    // read in a thread so the wait can give up: no polling while the notification is up
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = tx.send(out);
    });
    let clicked = match rx.recv_timeout(CLICK_WAIT) {
        Ok(out) => out.trim() == "afficher",
        Err(_) => {
            let _ = child.kill();
            false
        }
    };
    let _ = child.wait();
    if clicked && !konsole.is_empty() {
        focus(paths, konsole);
    }
}

/// Same state on disk, for windows opened by a launcher that cleans it up when the window
/// is closed by hand (CLAUDE_REPRISE=1): after a crash or a shutdown, claude-window
/// --reprise reopens them.
fn remember(paths: &Paths, color: &str, state: &str) {
    if env::var("CLAUDE_REPRISE").as_deref() != Ok("1") {
        return;
    }
    if let Some(session) = read_trim(paths.run_file(color, ".session")).filter(|s| !s.is_empty()) {
        record_window(paths, color, &session, state);
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let paths = Paths::new();
    if args.first().is_some_and(|a| a == "--notifier") {
        return notifier(&paths, &args[1..]);
    }
    let state = args.first().map(String::as_str).unwrap_or("");
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let color = env::var("CLAUDE_FRAME_COLOR").unwrap_or_default();
    if color.is_empty() {
        return;
    }
    let state_file = paths.run_file(&color, ".etat");
    if state == "travail" {
        if read_trim(&state_file).as_deref() != Some("travail") {
            let _ = fs::write(&state_file, "travail\n");
            remember(&paths, &color, "travail");
        }
        return;
    }

    let data: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let msg = data["message"].as_str().unwrap_or("");
    // « Claude is waiting for your input » (idle) is already covered by « fini »
    if state == "attente" && !msg.contains("ermission") {
        return;
    }
    let _ = fs::write(&state_file, format!("{state}\n"));
    remember(&paths, &color, state);

    let name = read_trim(paths.run_file(&color, ".titre")).filter(|t| !t.is_empty());
    let name = name.as_deref().unwrap_or("Claude Code");
    let done = state == "fini";
    let (title, body) = if done {
        let last = data["last_assistant_message"].as_str().unwrap_or("");
        let body: String = last.split('\n').filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
        let body: String = body.trim().chars().take(180).collect();
        (format!("✓ {name}"), if body.is_empty() { "Claude a terminé.".into() } else { body })
    } else {
        let body = match msg.strip_prefix("Claude needs your permission to use") {
            Some(rest) => format!("Claude attend ton accord pour utiliser{rest}"),
            None => msg.to_string(),
        };
        (format!("❗ {name}"), body)
    };

    let konsole = env::var("CLAUDE_KONSOLE_PID").unwrap_or_default();
    // checked before focus: while locked, the window still counts as active
    if away() {
        push(&paths, &title, &body, done);
    } else if focused(&konsole) {
        return;
    }
    if let Ok(me) = env::current_exe() {
        let _ = detach(Command::new(me).args(["--notifier", &title, &body, &konsole])).spawn();
    }
}
