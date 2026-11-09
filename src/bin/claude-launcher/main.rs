//! Conversation picker shown in every Claude window, then the supervisor of Claude.
//!
//! Enter: resume the conversation · Ctrl-N: new one (asks a title) · a title matching
//! nothing + Enter: new conversation with that title · Esc: quit

mod ask;
mod pty;

use ask::{Prompter, expand};
use claude_launcher::index::{self, Session};
use claude_launcher::{
    BLACK, BOLD, GREY, NEW_ID, Paths, RESET, Rgb, WHITE, color_taken, detach, fg, focus, frame_rgb, hex, log_line,
    menu, mix, open_windows, short_dir, which,
};
use serde_json::{Map, Value, json};
use std::env;
use std::ffi::CString;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, sleep};
use std::time::Duration;

struct Ctx {
    paths: Paths,
    color: String,
    accent: Rgb,
    light: Rgb,
    selbg: Rgb,
}

static CTX: OnceLock<Ctx> = OnceLock::new();

fn ctx() -> &'static Ctx {
    CTX.get_or_init(|| {
        let c = frame_rgb();
        Ctx {
            paths: Paths::new(),
            color: env::var("CLAUDE_FRAME_COLOR").unwrap_or_default(),
            accent: c,
            light: mix(c, WHITE, 0.45),
            selbg: mix(c, BLACK, 0.75),
        }
    })
}

fn log(msg: &str) {
    log_line(&ctx().paths, msg);
}

/// Title bar: state icon + conversation title.
struct Bar {
    base: String,
    state: String,
}

static BAR: Mutex<Bar> = Mutex::new(Bar { base: String::new(), state: String::new() });

fn update_bar(base: Option<&str>, state: Option<&str>) {
    let c = ctx();
    let mut bar = BAR.lock().unwrap();
    if let Some(b) = base {
        bar.base = b.to_string();
        if !c.color.is_empty() {
            let _ = fs::create_dir_all(&c.paths.run);
            let _ = fs::write(c.paths.run_file(&c.color, ".titre"), b);
        }
    }
    if let Some(s) = state {
        bar.state = s.to_string();
    }
    let icon = match bar.state.as_str() {
        "travail" => "⏳ ",
        "fini" => "✓ ",
        "attente" => "❗ ",
        _ => "",
    };
    let title: String = format!("{icon}{}", bar.base)
        .chars()
        .map(|ch| if ch < ' ' || ch == '\x7f' { ' ' } else { ch })
        .take(80)
        .collect();
    let mut out = io::stdout().lock();
    let _ = write!(out, "\x1b]2;{title}\x07");
    let _ = out.flush();
}

fn clear_screen() {
    print!("\x1b[0m\x1b[H\x1b[2J");
    let _ = io::stdout().flush();
}

fn color_of(sid: &str) -> Option<String> {
    let data: Value = serde_json::from_slice(&fs::read(&ctx().paths.color_map).ok()?).ok()?;
    data.get(sid)?.as_str().map(str::to_string)
}

/// The first color a conversation got is kept, even when it later opens elsewhere
/// because that color was taken.
fn remember_color(sid: &str) {
    let c = ctx();
    if c.color.is_empty() {
        return;
    }
    let mut data: Map<String, Value> = match fs::read(&c.paths.color_map) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(m) => m,
            Err(_) => return,
        },
        Err(_) => Map::new(),
    };
    if data.contains_key(sid) {
        return;
    }
    data.insert(sid.to_string(), Value::String(c.color.clone()));
    if let Some(dir) = c.paths.color_map.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(&c.paths.color_map, serde_json::to_string_pretty(&data).unwrap_or_default());
}

/// Tells the other windows' menus which conversation this window shows.
fn mark_session(sid: &str) {
    let c = ctx();
    if !c.color.is_empty() {
        let _ = fs::write(c.paths.run_file(&c.color, ".session"), sid);
    }
}

/// Matching theme, reduced motion (less CPU), and the hooks that feed the title bar.
fn claude_cmd(extra: &[&str]) -> Vec<String> {
    let c = ctx();
    let claude = which("claude").unwrap_or_else(|| c.paths.home.join(".local/bin/claude"));
    let mut settings = json!({ "prefersReducedMotion": true });
    if !c.color.is_empty() && c.paths.home.join(format!(".claude/themes/claude-{}.json", c.color)).exists() {
        settings["theme"] = json!(format!("custom:claude-{}", c.color));
    }
    let hook = c.paths.bin.join("claude-hook");
    if !c.color.is_empty() && hook.exists() {
        let h = |state: &str, matcher: Option<&str>| {
            let mut e = json!({ "hooks": [{ "type": "command", "command": format!("{} {state}", hook.display()) }] });
            if let Some(m) = matcher {
                e["matcher"] = json!(m);
            }
            json!([e])
        };
        settings["hooks"] = json!({
            "UserPromptSubmit": h("travail", None),
            "PostToolUse": h("travail", Some("*")),
            "Stop": h("fini", None),
            "Notification": h("attente", None),
        });
    }
    let mut cmd = vec![claude.to_string_lossy().into_owned()];
    cmd.extend(extra.iter().map(|s| s.to_string()));
    cmd.extend(["--settings".to_string(), settings.to_string()]);
    cmd
}

struct Watch {
    file: Box<dyn Fn() -> Option<PathBuf> + Send>,
    title: String,
    custom: bool,
}

/// Keeps the bar up to date: Claude's state (written by claude-hook) every second, and
/// the conversation title (automatic title, /rename) every 3 s, reading only what was
/// appended to the conversation file.
fn watch_title(w: Watch) {
    let c = ctx();
    let state_file = c.paths.run_file(&c.color, ".etat");
    let (mut pos, mut tick) = (None::<u64>, 0u64);
    let (mut current, mut has_custom) = (w.title, w.custom);
    loop {
        sleep(Duration::from_secs(1));
        tick += 1;
        let state = fs::read_to_string(&state_file).map(|s| s.trim().to_string()).unwrap_or_default();
        if state != BAR.lock().unwrap().state {
            update_bar(None, Some(&state));
        }
        if tick % 3 != 0 {
            continue;
        }
        let Some(path) = (w.file)() else { continue };
        let Ok(size) = fs::metadata(&path).map(|m| m.len()) else { continue };
        let start = match pos {
            Some(p) if p <= size => p,
            _ => size.saturating_sub(512_000),
        };
        pos = Some(start);
        if size == start {
            continue;
        }
        let mut chunk = Vec::new();
        let Ok(mut f) = fs::File::open(&path) else { continue };
        if f.seek(SeekFrom::Start(start)).is_err() || f.take(size - start).read_to_end(&mut chunk).is_err() {
            continue;
        }
        let Some(nl) = chunk.iter().rposition(|&b| b == b'\n') else { continue };
        pos = Some(start + nl as u64 + 1);
        let (custom, ai) = index::titles_in(&chunk[..=nl]);
        let new = custom.clone().or(if has_custom { None } else { ai });
        has_custom |= custom.is_some();
        if let Some(t) = new
            && t != current
        {
            current = t;
            update_bar(Some(&current), None);
            log(&format!("titre mis à jour : {current:?}"));
        }
    }
}

/// Runs Claude and stays behind it, like a shell would: Claude gets its own process
/// group in the foreground of the terminal, otherwise Linux drops its Ctrl+Z and it
/// stays stuck "suspended". If it does stop, it is resumed at once, since there is
/// no shell here to type `fg`.
fn launch(cwd: &Path, args: Vec<String>, watch: Watch) -> ! {
    let _ = env::set_current_dir(cwd);
    log(&format!("lancement dans {} : {:?}", cwd.display(), &args[1..]));
    // SAFETY: no other thread is running at this point
    unsafe {
        env::set_var("CLAUDE_CODE_DISABLE_TERMINAL_TITLE", "1");
        env::set_var("CLAUDE_KONSOLE_PID", libc::getppid().to_string()); // tells claude-hook if the window has focus
    }
    let _ = io::stdout().flush();
    let cargs: Vec<CString> = args.iter().filter_map(|a| CString::new(a.as_str()).ok()).collect();
    let mut argv: Vec<*const libc::c_char> = cargs.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        log("fork impossible");
        exit(1);
    }
    if pid == 0 {
        unsafe {
            libc::setpgid(0, 0);
            libc::signal(libc::SIGTTOU, libc::SIG_IGN);
            libc::tcsetpgrp(0, libc::getpgrp());
            libc::signal(libc::SIGTTOU, libc::SIG_DFL);
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::execv(argv[0], argv.as_ptr());
            libc::_exit(127);
        }
    }
    unsafe {
        libc::setpgid(pid, pid);
        for s in [libc::SIGINT, libc::SIGQUIT, libc::SIGTSTP, libc::SIGTTOU] {
            libc::signal(s, libc::SIG_IGN);
        }
    }
    thread::spawn(move || watch_title(watch));
    loop {
        let mut status = 0;
        if unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) } < 0 {
            if io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            exit(1);
        }
        if libc::WIFSTOPPED(status) {
            log("Claude suspendu (Ctrl+Z) : reprise automatique");
            sleep(Duration::from_millis(200));
            unsafe {
                libc::tcsetpgrp(0, pid);
                libc::killpg(pid, libc::SIGCONT);
            }
            continue;
        }
        let code = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { -libc::WTERMSIG(status) };
        log(&format!("Claude terminé (code {code})"));
        exit(if code >= 0 { code } else { 1 });
    }
}

fn ask(p: &mut Prompter, styled: &str, dirs: bool) -> String {
    let plain = ask::strip_ansi(styled);
    log(&format!("question : {}", plain.split('(').next().unwrap_or("").trim()));
    match p.ask(styled, dirs) {
        Some(r) => {
            log(&format!("réponse : {r:?}"));
            r
        }
        None => {
            log("annulé");
            exit(0);
        }
    }
}

fn ask_dir(p: &mut Prompter, recent: &[PathBuf]) -> PathBuf {
    let c = ctx();
    let v = fg(c.light);
    println!("\n{GREY}Dossier :{RESET}");
    println!("  {v}Entrée{RESET} ~/claude {GREY}(dossier des conversations){RESET}");
    for (i, d) in recent.iter().enumerate() {
        println!("  {v}{:>6}{RESET} {}", i + 1, short_dir(&c.paths.home, &d.to_string_lossy()));
    }
    println!("  {v}{:>6}{RESET} ton dossier perso\n", "~");
    loop {
        let raw = ask(p, &format!("Dossier {GREY}(Entrée, numéro ou chemin · Tab pour compléter){RESET} : "), true);
        if raw.is_empty() {
            let d = c.paths.home.join("claude");
            let _ = fs::create_dir(&d);
            return d;
        }
        if raw.bytes().all(|b| b.is_ascii_digit())
            && let Ok(n) = raw.parse::<usize>()
            && (1..=recent.len()).contains(&n)
        {
            return recent[n - 1].clone();
        }
        let d = expand(&c.paths.home, &raw);
        if d.is_dir() {
            return fs::canonicalize(&d).unwrap_or(d);
        }
        if d.exists() {
            println!("{} n'est pas un dossier.", d.display());
            continue;
        }
        let answer = ask(p, &format!("{} n'existe pas. Le créer ? (o/N) ", d.display()), false).to_lowercase();
        if ["o", "oui", "y", "yes"].contains(&answer.as_str()) && fs::create_dir_all(&d).is_ok() {
            return d;
        }
    }
}

fn uuid4() -> String {
    let mut b = [0u8; 16];
    unsafe { libc::getrandom(b.as_mut_ptr().cast(), b.len(), 0) };
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

fn new_session(title: Option<String>) -> ! {
    let c = ctx();
    log("nouvelle conversation");
    println!("{BOLD}{}Nouvelle conversation{RESET}\n", fg(c.light));
    let Some(mut p) = Prompter::new(&c.paths.home) else {
        log("pas de terminal pour les questions");
        exit(1);
    };
    let title = match title {
        Some(t) => {
            println!("Titre   : {t}");
            t
        }
        None => ask(&mut p, &format!("Titre   {GREY}(Entrée = titre automatique){RESET} : "), false),
    };
    let cwd = ask_dir(&mut p, &menu::recent_dirs(&c.paths));
    let sid = uuid4(); // known in advance, so the title of the conversation can be followed
    let shown = if title.is_empty() { "Nouvelle conversation".to_string() } else { title.clone() };
    update_bar(Some(&shown), None);
    remember_color(&sid);
    mark_session(&sid);
    let mut args = vec!["--session-id", sid.as_str()];
    if !title.is_empty() {
        args.extend(["-n", title.as_str()]);
    }
    let id = sid.clone();
    let watch = Watch { file: Box::new(move || index::session_file(&ctx().paths, &id)), title: shown, custom: !title.is_empty() };
    launch(&cwd, claude_cmd(&args), watch)
}

fn resume(s: Session, move_to_own_color: bool) {
    let c = ctx();
    if move_to_own_color
        && !c.color.is_empty()
        && let Some(pref) = color_of(&s.id)
        && pref != c.color
        && !color_taken(&c.paths, &pref)
    {
        // its usual color is free: reopen it in a window of that color, this one closes
        log(&format!("reprise de {} dans une fenêtre {pref} (couleur habituelle)", s.id));
        let _ = detach(Command::new(c.paths.bin.join("claude-window")).args(["--couleur", &pref, "--reprendre", &s.id]))
            .spawn();
        return;
    }
    let cwd = s.meta.cwd.as_deref().map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(|| c.paths.home.clone());
    update_bar(Some(&s.title), None);
    remember_color(&s.id);
    mark_session(&s.id);
    let file = s.file.clone();
    let watch = Watch { file: Box::new(move || Some(file.clone())), title: s.title.clone(), custom: s.meta.custom.is_some() };
    launch(&cwd, claude_cmd(&["--resume", &s.id]), watch)
}

/// claude-window adds a Konsole settings folder to XDG_CONFIG_DIRS; everything started
/// from here (fzf, Claude) gets the original value back.
fn restore_config_dirs() {
    let Some(orig) = env::var_os("CLAUDE_ORIG_XDG_CONFIG_DIRS") else { return };
    // SAFETY: first thing in main, before any thread exists
    unsafe {
        env::remove_var("CLAUDE_ORIG_XDG_CONFIG_DIRS");
        if orig.is_empty() {
            env::remove_var("XDG_CONFIG_DIRS");
        } else {
            env::set_var("XDG_CONFIG_DIRS", orig);
        }
    }
}

fn fzf_args(menu_bin: &str) -> Vec<String> {
    let c = ctx();
    let (accent, light, selbg) = (hex(c.accent), hex(c.light), hex(c.selbg));
    let m = menu_bin;
    [
        "--ansi",
        "--delimiter=\t",
        "--with-nth=2..",
        "--nth=3",
        "--tabstop=1",
        "--layout=reverse",
        "--height=100%",
        "--border=none",
        "--prompt=Rechercher › ",
        "--header=Entrée : ouvrir · Ctrl-N : nouvelle · Échap : quitter   ● = déjà ouverte\n\
         Ctrl-F : chercher dans les messages · Ctrl-T : revenir aux titres · Ctrl-E : épingler",
        "--print-query",
        "--expect=ctrl-n",
        // in "messages" mode, Enter with no result doesn't create a conversation
        "--bind=enter:transform:case \"$FZF_PROMPT\" in Dans*) echo accept;; *) echo accept-or-print-query;; esac",
        "--bind=start:unbind(change)",
    ]
    .into_iter()
    .map(String::from)
    .chain([
        format!("--bind=ctrl-f:change-prompt(Dans les messages › )+disable-search+reload({m} search {{q}})+first+rebind(change)"),
        format!("--bind=change:reload({m} search {{q}})+first"),
        format!("--bind=ctrl-e:execute-silent({m} pin {{1}})+change-prompt(Rechercher › )+enable-search+unbind(change)+reload({m} list)"),
        format!("--bind=ctrl-t:change-prompt(Rechercher › )+enable-search+unbind(change)+reload({m} list)+first"),
        "--with-shell=sh -c".to_string(),
        format!("--preview={m} preview {{1}} {{q}}"),
        "--preview-window=right,45%,wrap,border-left".to_string(),
        format!(
            "--color=fg+:#ffffff,bg+:{selbg},hl:{light},hl+:{light},pointer:{accent},prompt:{light},border:{accent},\
             label:{light},header:#8a8a8a,info:{accent},preview-border:{accent}"
        ),
    ])
    .collect()
}

fn main() {
    restore_config_dirs();
    let c = ctx();
    log(&format!("démarrage (cadre {})", if c.color.is_empty() { "?" } else { &c.color }));
    if !c.color.is_empty() {
        let _ = fs::remove_file(c.paths.run_file(&c.color, ".etat"));
        let _ = fs::remove_file(c.paths.run_file(&c.color, ".session"));
    }
    update_bar(Some("Claude Code"), Some(""));
    let direct = env::var("CLAUDE_RESUME_SID").unwrap_or_default();
    // SAFETY: still single-threaded
    unsafe { env::remove_var("CLAUDE_RESUME_SID") };
    if !direct.is_empty()
        && let Some(s) = index::find_session(&c.paths, &direct)
    {
        log(&format!("ouverture directe de {direct}"));
        clear_screen();
        return resume(s, false);
    }

    let rows = menu::list(&c.paths);
    log(&format!("{} conversations lues", rows.matches('\n').count().saturating_sub(1)));
    let menu_bin = c.paths.bin.join("claude-menu");
    log("menu affiché");
    let (code, out) = pty::run_fzf(&fzf_args(&menu_bin.to_string_lossy()), rows);
    let lines: Vec<&str> = out.split('\n').collect();
    let query = lines.first().map(|l| l.trim()).unwrap_or("");
    let key = lines.get(1).copied().unwrap_or("");
    let choice = lines.get(2).copied().unwrap_or("");
    let sid = choice.split('\t').next().unwrap_or("");
    log(&format!(
        "menu fermé (code {code}, touche {}, choix {})",
        if key.is_empty() { "Entrée" } else { key },
        if sid.is_empty() { "-" } else { sid }
    ));
    if code != 0 && code != 1 {
        return; // Esc / Ctrl-C
    }
    clear_screen();

    if key == "ctrl-n" || choice.is_empty() {
        new_session((!query.is_empty()).then(|| query.to_string()));
    }
    if sid == NEW_ID {
        new_session(None);
    }
    if let Some(w) = open_windows(&c.paths).get(sid) {
        // already open: show its window instead of opening a second one
        log(&format!("{sid} déjà ouverte (fenêtre {}) : mise au premier plan", w.color));
        focus(&c.paths, &w.pid);
        return;
    }
    if let Some(s) = index::find_session(&c.paths, sid) {
        resume(s, true);
    }
}
