//! Rows and preview of the fzf menu (also called by fzf itself through claude-menu).

use crate::index::{Session, all_sessions, find_session, fold, one_line};
use crate::{BOLD, GREY, NEW_ID, Paths, RESET, Window, fg, frame_rgb, local_tm, mix, now, open_windows, short_dir, WHITE};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Light version of the window color, for titles and highlights.
fn accent() -> String {
    fg(mix(frame_rgb(), WHITE, 0.45))
}

pub fn pins(paths: &Paths) -> Vec<String> {
    fs::read(&paths.pins).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn toggle_pin(paths: &Paths, id: &str) {
    if id == NEW_ID {
        return;
    }
    let mut list = pins(paths);
    if let Some(i) = list.iter().position(|p| p == id) {
        list.remove(i);
    } else {
        list.insert(0, id.to_string());
    }
    if let Some(dir) = paths.pins.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(&paths.pins, serde_json::to_vec(&list).unwrap_or_default());
}

fn when(t: i64) -> String {
    let (d, today, yesterday) = (local_tm(t), local_tm(now()), local_tm(now() - 86400));
    let same = |a: &libc::tm, b: &libc::tm| a.tm_year == b.tm_year && a.tm_yday == b.tm_yday;
    let day = if same(&d, &today) {
        "aujourd'hui".to_string()
    } else if same(&d, &yesterday) {
        "hier".to_string()
    } else {
        format!("{:02}/{:02}", d.tm_mday, d.tm_mon + 1)
    };
    format!("{day} {:02}:{:02}", d.tm_hour, d.tm_min)
}

fn dir_of(paths: &Paths, s: &Session) -> String {
    short_dir(&paths.home, s.meta.cwd.as_deref().unwrap_or(""))
}

/// Marker column, always 3 columns wide: colored dot of the window + its state.
fn marker(w: Option<&Window>) -> String {
    let Some(w) = w else { return "   ".into() };
    let state = match w.state.as_str() {
        "travail" => "⏳",
        "attente" => "❗",
        "fini" => "✓ ",
        _ => "  ",
    };
    format!("{}●{RESET}{state}", fg(w.rgb))
}

/// `id \t marker \t date \t title  folder…` — fzf shows fields 2.. and searches field 3 of those.
fn row(paths: &Paths, s: &Session, open: &HashMap<String, Window>, pinned: &[String], extra: &str) -> String {
    let dir = dir_of(paths, s);
    let place = if dir.is_empty() || dir == "~" { String::new() } else { format!("  {GREY}{dir}{RESET}") };
    let pin = pinned.contains(&s.id);
    let old = now() - s.mtime > 7 * 86400 && !pin;
    let title = if old { format!("{GREY}{}{RESET}", s.title) } else { s.title.clone() };
    format!(
        "{}\t{}\t{GREY}{:>17}{RESET}\t{}{title}{place}{extra}\n",
        s.id,
        marker(open.get(&s.id)),
        when(s.mtime),
        if pin { "📌 " } else { "" },
    )
}

pub fn list(paths: &Paths) -> String {
    let sessions = all_sessions(paths);
    let (open, pinned) = (open_windows(paths), pins(paths));
    let mut out = format!("{NEW_ID}\t   \t{:17}\t{BOLD}{}＋ Nouvelle conversation{RESET}\n", "", accent());
    let (first, rest): (Vec<_>, Vec<_>) = sessions.iter().partition(|s| pinned.contains(&s.id));
    for s in first.into_iter().chain(rest) {
        out.push_str(&row(paths, s, &open, &pinned, ""));
    }
    out
}

fn words_of(query: &str) -> Vec<String> {
    fold(query).split_whitespace().map(str::to_string).collect()
}

/// Message lines containing every word, with the char position of the first word.
fn find_hits(norm: &str, words: &[String]) -> Vec<(usize, usize)> {
    norm.lines()
        .enumerate()
        .filter(|(_, l)| words.iter().all(|w| l.contains(w.as_str())))
        .map(|(i, l)| (i, l[..l.find(words[0].as_str()).unwrap_or(0)].chars().count()))
        .collect()
}

fn index_file(paths: &Paths, id: &str, ext: &str) -> String {
    fs::read_to_string(paths.index.join(format!("{id}.{ext}"))).unwrap_or_default()
}

fn excerpt(line: &str, pos: usize, before: usize, after: usize) -> String {
    line.chars().skip(pos.saturating_sub(before)).take(before.min(pos) + after).collect()
}

pub fn search(paths: &Paths, query: &str) -> String {
    let words = words_of(query);
    if words.is_empty() {
        return list(paths);
    }
    let sessions = all_sessions(paths);
    let (open, pinned) = (open_windows(paths), pins(paths));
    let mut out = String::new();
    for s in &sessions {
        let hits = find_hits(&index_file(paths, &s.id, "norm"), &words);
        let Some(&(line, pos)) = hits.last() else { continue };
        let raw = index_file(paths, &s.id, "txt");
        let text = excerpt(raw.lines().nth(line).unwrap_or(""), pos, 40, 70).replace('\t', " ");
        let extra = format!("  {GREY}({}) « {} »{RESET}", hits.len(), text.trim());
        out.push_str(&row(paths, s, &open, &pinned, &extra));
    }
    out
}

fn wrap(text: &str, width: usize) -> String {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines.join("\n")
}

pub fn preview(paths: &Paths, id: &str, query: &str) -> String {
    let width = env::var("FZF_PREVIEW_COLUMNS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(60).max(32) - 2;
    let v = accent();
    if id == NEW_ID {
        let mut out = format!("{BOLD}{v}Nouvelle conversation{RESET}\n\n");
        for para in [
            "Entrée → tu donnes un titre, puis le dossier (Entrée = ~/claude, un numéro = dossier récent), et Claude Code s'ouvre.",
            "Astuce : tape directement le titre dans la recherche. S'il ne correspond à aucune conversation, Entrée la crée avec ce titre.",
        ] {
            out.push_str(&format!("{}\n\n", wrap(para, width)));
        }
        return out;
    }
    let Some(s) = find_session(paths, id) else { return String::new() };
    let mut out = format!("{BOLD}{v}{}{RESET}\n\n", s.title);
    if pins(paths).contains(&s.id) {
        out.push_str(&format!("📌 Épinglée {GREY}(Ctrl-E pour la détacher){RESET}\n\n"));
    }
    if let Some(w) = open_windows(paths).get(&s.id) {
        let state = match w.state.as_str() {
            "travail" => "Claude travaille",
            "fini" => "Claude a fini",
            "attente" => "attend ton accord",
            _ => "ouverte",
        };
        out.push_str(&format!("{}● Ouverte dans la fenêtre {}{RESET} ({state})\n", fg(w.rgb), w.color));
        out.push_str(&format!("{GREY}Entrée = afficher cette fenêtre{RESET}\n\n"));
    }
    out.push_str(&format!("{GREY}Dossier  :{RESET} {}\n", dir_of(paths, &s)));
    out.push_str(&format!("{GREY}Activité :{RESET} {}\n", when(s.mtime)));
    out.push_str(&format!("{GREY}Messages :{RESET} {}\n", s.meta.msgs));
    if let (Some(c), Some(a)) = (&s.meta.custom, &s.meta.ai)
        && c != a
    {
        out.push_str(&format!("{GREY}Résumé   :{RESET} {a}\n"));
    }

    let words = words_of(query);
    if !words.is_empty() {
        let hits = find_hits(&index_file(paths, &s.id, "norm"), &words);
        if !hits.is_empty() {
            let txt = index_file(paths, &s.id, "txt");
            let raw: Vec<&str> = txt.lines().collect();
            out.push_str(&format!("\n{v}Passages avec « {} » ({}) :{RESET}\n", query.trim(), hits.len()));
            let wlen = words[0].chars().count();
            for &(n, pos) in hits.iter().rev().take(4) {
                let line: Vec<char> = raw.get(n).copied().unwrap_or("").chars().collect();
                let start = pos.saturating_sub(60);
                let cut = |a: usize, b: usize| line[a.min(line.len())..b.min(line.len())].iter().collect::<String>();
                let before = format!("{}{}", if start > 0 { "…" } else { "" }, cut(start, pos));
                let found = cut(pos, pos + wlen);
                let after = format!("{}…", cut(pos + wlen, pos + 140));
                let plain = wrap(&format!("{before}\u{1}{after}"), width);
                out.push_str(&plain.replacen('\u{1}', &format!("{v}{found}{RESET}"), 1));
                out.push_str("\n\n");
            }
            return out;
        }
    }
    for (label, text) in [("Premier message", &s.meta.first), ("Dernier message", &s.meta.last)] {
        if let Some(t) = text {
            let short: String = one_line(t).chars().take(500).collect();
            out.push_str(&format!("\n{GREY}{label} :{RESET}\n{}\n", wrap(&short, width)));
        }
    }
    out
}

/// Folders of recent conversations (not ~ nor ~/claude), most recent first, at most 6.
pub fn recent_dirs(paths: &Paths) -> Vec<PathBuf> {
    let skip = [paths.home.clone(), paths.home.join("claude")];
    let mut dirs: Vec<PathBuf> = Vec::new();
    for s in all_sessions(paths) {
        let Some(cwd) = s.meta.cwd else { continue };
        let p = PathBuf::from(cwd);
        if !skip.contains(&p) && p.is_dir() && !dirs.contains(&p) {
            dirs.push(p);
        }
        if dirs.len() == 6 {
            break;
        }
    }
    dirs
}
