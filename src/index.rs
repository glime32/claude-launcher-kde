//! Per-conversation index in ~/.cache/claude-launcher/index: titles and folder (.json),
//! messages as plain text (.txt) and folded for search (.norm). Only appended bytes are read.

use crate::Paths;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const INDEX_VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Default)]
pub struct Meta {
    pub v: u32,
    pub size: u64,
    pub custom: Option<String>,
    pub ai: Option<String>,
    pub summary: Option<String>,
    pub last: Option<String>,
    pub first: Option<String>,
    pub cwd: Option<String>,
    pub msgs: u64,
}

fn text_only<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) if !s.is_empty() => Some(s),
        _ => None,
    })
}

#[derive(Deserialize)]
struct Record<'a> {
    #[serde(rename = "type", default, deserialize_with = "text_only")]
    kind: Option<String>,
    #[serde(rename = "customTitle", default, deserialize_with = "text_only")]
    custom_title: Option<String>,
    #[serde(rename = "aiTitle", default, deserialize_with = "text_only")]
    ai_title: Option<String>,
    #[serde(default, deserialize_with = "text_only")]
    summary: Option<String>,
    #[serde(rename = "lastPrompt", default, deserialize_with = "text_only")]
    last_prompt: Option<String>,
    #[serde(rename = "isMeta", default)]
    is_meta: Option<bool>,
    #[serde(default, deserialize_with = "text_only")]
    cwd: Option<String>,
    #[serde(borrow, default)]
    message: Option<Message<'a>>,
}

#[derive(Deserialize)]
struct Message<'a> {
    #[serde(borrow, default)]
    content: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

fn block_text(raw: &RawValue) -> Option<String> {
    let s = raw.get();
    if s.starts_with('"') {
        return serde_json::from_str(s).ok();
    }
    let blocks: Vec<Block> = serde_json::from_str(s).ok()?;
    let parts: Vec<String> = blocks
        .into_iter()
        .filter(|b| b.kind.as_deref() == Some("text"))
        .filter_map(|b| b.text)
        .collect();
    Some(parts.join(" "))
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        match rest[i + 1..].find('>') {
            Some(j) if j > 0 => {
                out.push(' ');
                rest = &rest[i + j + 2..];
            }
            _ => {
                out.push('<');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn user_text(raw: &RawValue) -> Option<String> {
    let text = block_text(raw)?;
    let head = text.trim_start();
    if head.starts_with("<command") || head.starts_with("<local-command") || head.starts_with("<system") {
        return None;
    }
    let text = strip_tags(&text).trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lowercase without accents, so « geometrie » finds « Géométrie ».
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => out.push('a'),
            'ç' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' => out.push('i'),
            'ñ' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            '\u{300}'..='\u{36f}' => {}
            _ => out.push(c),
        }
    }
    out
}

fn update_index(paths: &Paths, file: &Path, id: &str) -> Option<Meta> {
    let size = fs::metadata(file).ok()?.len();
    fs::create_dir_all(&paths.index).ok()?;
    let lock = File::create(paths.index.join(format!("{id}.lock"))).ok()?;
    lock.lock().ok()?;

    let meta_path = paths.index.join(format!("{id}.json"));
    let txt_path = paths.index.join(format!("{id}.txt"));
    let norm_path = paths.index.join(format!("{id}.norm"));
    let mut meta: Meta = fs::read(&meta_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if meta.v != INDEX_VERSION || meta.size > size {
        meta = Meta { v: INDEX_VERSION, ..Meta::default() };
        fs::write(&txt_path, "").ok()?;
        fs::write(&norm_path, "").ok()?;
    }
    if meta.size == size {
        return Some(meta);
    }

    let mut f = File::open(file).ok()?;
    f.seek(SeekFrom::Start(meta.size)).ok()?;
    let mut chunk = Vec::with_capacity((size - meta.size) as usize);
    f.take(size - meta.size).read_to_end(&mut chunk).ok()?;
    let Some(end) = chunk.iter().rposition(|&b| b == b'\n') else {
        return Some(meta);
    };

    let mut raw = String::new();
    for line in chunk[..=end].split(|&b| b == b'\n') {
        let Ok(rec) = serde_json::from_slice::<Record>(line) else { continue };
        match rec.kind.as_deref() {
            Some("custom-title") if rec.custom_title.is_some() => meta.custom = rec.custom_title,
            Some("ai-title") if rec.ai_title.is_some() => meta.ai = rec.ai_title,
            Some("summary") if rec.summary.is_some() => meta.summary = rec.summary,
            Some("last-prompt") if rec.last_prompt.is_some() => meta.last = rec.last_prompt,
            Some(kind @ ("user" | "assistant")) if rec.is_meta != Some(true) => {
                meta.msgs += 1;
                if meta.cwd.is_none() {
                    meta.cwd = rec.cwd;
                }
                let content = rec.message.and_then(|m| m.content);
                let text = if kind == "user" {
                    let t = content.and_then(user_text);
                    if meta.first.is_none() {
                        meta.first = t.clone();
                    }
                    t
                } else {
                    content.and_then(block_text).filter(|t| !t.trim().is_empty())
                };
                if let Some(t) = text {
                    raw.push_str(if kind == "user" { "Toi : " } else { "Claude : " });
                    raw.push_str(&one_line(&t));
                    raw.push('\n');
                }
            }
            _ => {}
        }
    }
    if !raw.is_empty() {
        append(&txt_path, &raw)?;
        append(&norm_path, &fold(&raw))?;
    }
    meta.size += end as u64 + 1;
    let tmp = meta_path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(&meta).ok()?).ok()?;
    fs::rename(&tmp, &meta_path).ok()?;
    Some(meta)
}

fn append(path: &Path, text: &str) -> Option<()> {
    OpenOptions::new().create(true).append(true).open(path).ok()?.write_all(text.as_bytes()).ok()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Latest custom (/rename) and automatic titles found in a piece of a conversation file.
pub fn titles_in(chunk: &[u8]) -> (Option<String>, Option<String>) {
    let (mut custom, mut ai) = (None, None);
    for line in chunk.split(|&b| b == b'\n') {
        if !contains(line, b"\"custom-title\"") && !contains(line, b"\"ai-title\"") {
            continue;
        }
        let Ok(rec) = serde_json::from_slice::<Record>(line) else { continue };
        match rec.kind.as_deref() {
            Some("custom-title") if rec.custom_title.is_some() => custom = rec.custom_title,
            Some("ai-title") if rec.ai_title.is_some() => ai = rec.ai_title,
            _ => {}
        }
    }
    (custom, ai)
}

pub struct Session {
    pub id: String,
    pub file: PathBuf,
    pub mtime: i64,
    pub title: String,
    pub meta: Meta,
}

fn first_line(s: &Option<String>) -> Option<String> {
    s.as_deref().and_then(|t| t.trim().lines().next()).filter(|l| !l.is_empty()).map(str::to_string)
}

fn load_session(paths: &Paths, file: &Path) -> Option<Session> {
    let id = file.file_stem()?.to_str()?.to_string();
    let meta = update_index(paths, file, &id)?;
    let titled = meta.custom.is_some() || meta.ai.is_some() || meta.summary.is_some() || meta.first.is_some();
    if meta.msgs == 0 || !titled {
        return None;
    }
    let mtime = fs::metadata(file).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let title = meta
        .custom
        .clone()
        .or_else(|| meta.ai.clone())
        .or_else(|| meta.summary.clone())
        .or_else(|| first_line(&meta.first))
        .or_else(|| first_line(&meta.last))
        .unwrap_or_else(|| "(sans titre)".into());
    Some(Session { id, file: file.to_path_buf(), mtime, title, meta })
}

/// Every titled conversation, most recent first.
pub fn all_sessions(paths: &Paths) -> Vec<Session> {
    let mut out = Vec::new();
    for project in fs::read_dir(&paths.projects).into_iter().flatten().flatten() {
        for entry in fs::read_dir(project.path()).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "jsonl") {
                out.extend(load_session(paths, &p));
            }
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    out
}

/// Conversation file of `id`, whichever project folder it is in.
pub fn session_file(paths: &Paths, id: &str) -> Option<PathBuf> {
    fs::read_dir(&paths.projects)
        .ok()?
        .flatten()
        .map(|project| project.path().join(format!("{id}.jsonl")))
        .find(|p| p.exists())
}

pub fn find_session(paths: &Paths, id: &str) -> Option<Session> {
    load_session(paths, &session_file(paths, id)?)
}
