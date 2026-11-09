//! Generates everything the Claude window colors need, from ~/.config/claude-window/couleurs
//! (« name r,g,b » per line, in the order they are handed out; created if missing):
//! Konsole style sheets, KWin rules, Claude themes, hidden .desktop files and the
//! window icons. Run it again after changing the colors.
//!
//! The windows have no KDE decoration: Konsole draws the black bar (colored title) and the
//! thin frame through one style sheet per color.

use claude_launcher::{BLACK, Paths, Rgb, WHITE, hex, mix, palette, var_dir};
use serde_json::json;
use std::fs;
use std::process::{Command, Stdio};

const DEFAULT_COLORS: [(&str, Rgb); 8] = [
    ("bleu", (59, 130, 246)),
    ("violet", (139, 92, 246)),
    ("orange", (217, 119, 87)),
    ("vert", (34, 197, 94)),
    ("rose", (236, 72, 153)),
    ("cyan", (6, 182, 212)),
    ("jaune", (234, 179, 8)),
    ("rouge", (239, 68, 68)),
];

/// Frame width in logical pixels (×1.6 on this screen ≈ 3 px).
const FRAME_PX: u32 = 2;

const STYLE: &str = "/* Fenêtre Claude {name} : barre noire + titre en couleur + cadre fin (généré) */
QMainWindow, QTabWidget, QStackedWidget, QTabBar { background: #000000; }
QTabBar::tab { background: #000000; color: {a}; font-weight: bold; padding: 0px 10px;
               border: {px}px solid {a}; border-bottom: none; margin: 0px; }
QTabBar::tab:!active { color: {i}; border-color: {i}; }
QTabBar::close-button { image: none; width: 0px; height: 0px; }
QTabBar { qproperty-iconSize: 0px 0px; }
QTabWidget::right-corner, QTabWidget::left-corner { width: 0px; height: 0px; }
QTabWidget QToolButton { width: 0px; max-width: 0px; min-width: 0px; padding: 0px; margin: 0px; border: none; }
QSplitter { border: {px}px solid {a}; border-top: none; }
QSplitter:!active { border-color: {i}; }
";

/// Claude Code's pixel character, measured on its welcome screen. A cell is a quarter of
/// a terminal character, so taller than wide.
const MASCOT: [&str; 5] = [
    "...############...",
    "...##.######.##...",
    ".################.",
    "...############...",
    "...#.#......#.#...",
];
const CELL_W: f64 = 0.6;
const CELL_H: f64 = 1.0;
const ICON_PX: u32 = 256;

fn kwrite(group: &str, key: &str, value: Option<&str>) {
    let mut cmd = Command::new("kwriteconfig6");
    cmd.args(["--file", "kwinrulesrc", "--group", group, "--key", key]);
    match value {
        Some(v) => cmd.arg(v),
        None => cmd.arg("--delete"),
    };
    if !cmd.status().is_ok_and(|s| s.success()) {
        eprintln!("kwriteconfig6 a échoué pour [{group}] {key}");
    }
}

fn kread(group: &str, key: &str) -> String {
    Command::new("kreadconfig6")
        .args(["--file", "kwinrulesrc", "--group", group, "--key", key])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Keeps the other rules (e.g. helium-sans-bordure) after ours, in the same order.
fn update_rule_order(names: &[String]) {
    let ours: Vec<String> = names.iter().map(|n| format!("claude-code-{n}")).collect();
    let current = kread("General", "Order");
    let others = current.split(',').filter(|r| !r.is_empty() && !r.starts_with("claude-code-")).map(str::to_string);
    let order: Vec<String> = ours.into_iter().chain(others).collect();
    if order.join(",") != current {
        kwrite("General", "Order", Some(&order.join(",")));
    }
}

/// Square view box with a small margin around the character.
fn mascot_box() -> (f64, f64, f64) {
    let (w, h) = (MASCOT[0].len() as f64 * CELL_W, MASCOT.len() as f64 * CELL_H);
    let side = w.max(h) * 1.08;
    (side, (side - w) / 2.0, (side - h) / 2.0)
}

/// Rectangles of the character (x, y, w, h), rounded like the SVG; neighbors overlap a
/// little so no seam shows between them.
fn mascot_rects() -> Vec<(f64, f64, f64, f64)> {
    let (_, ox, oy) = mascot_box();
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    let mut rects = Vec::new();
    for (y, line) in MASCOT.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch == '#' {
                rects.push((r3(ox + x as f64 * CELL_W), r3(oy + y as f64 * CELL_H), r3(CELL_W + 0.01), r3(CELL_H + 0.01)));
            }
        }
    }
    rects
}

fn mascot_svg(c: Rgb) -> String {
    let (side, _, _) = mascot_box();
    let rects: String = mascot_rects()
        .iter()
        .map(|(x, y, w, h)| format!(r#"<rect x="{x:.3}" y="{y:.3}" width="{w:.3}" height="{h:.3}"/>"#))
        .collect();
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {side:.3} {side:.3}" shape-rendering="crispEdges" fill="{}">{rects}</svg>
"#,
        hex(c)
    )
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend(kind);
    out.extend(data);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

/// Palette PNG, 2 bits per pixel (index 0 is transparent): crisp and a few kB.
fn palette_png(colors: &[Rgb], index: impl Fn(f64, f64) -> u8) -> Vec<u8> {
    let row_len = (ICON_PX * 2).div_ceil(8) as usize;
    let mut raw = Vec::with_capacity((row_len + 1) * ICON_PX as usize);
    for y in 0..ICON_PX {
        raw.push(0); // no filter
        let mut row = vec![0u8; row_len];
        for x in 0..ICON_PX {
            let i = index(x as f64 + 0.5, y as f64 + 0.5) & 3;
            row[x as usize / 4] |= i << (6 - 2 * (x % 4));
        }
        raw.extend(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        let len = block.len() as u16;
        zlib.push((i + 1 == blocks.len()) as u8); // stored block, final flag
        zlib.extend(len.to_le_bytes());
        zlib.extend((!len).to_le_bytes());
        zlib.extend(*block);
    }
    zlib.extend(adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend(ICON_PX.to_be_bytes());
    header.extend(ICON_PX.to_be_bytes());
    header.extend([2, 3, 0, 0, 0]); // 2 bits, palette
    png_chunk(&mut out, b"IHDR", &header);
    let plte: Vec<u8> = colors.iter().flat_map(|c| [c.0, c.1, c.2]).collect();
    png_chunk(&mut out, b"PLTE", &plte);
    png_chunk(&mut out, b"tRNS", &[0]);
    png_chunk(&mut out, b"IDAT", &zlib);
    png_chunk(&mut out, b"IEND", &[]);
    out
}

fn inside(rects: &[(f64, f64, f64, f64)], x: f64, y: f64) -> bool {
    rects.iter().any(|&(rx, ry, w, h)| x >= rx && x < rx + w && y >= ry && y < ry + h)
}

/// The character alone, in the window color (Alt+Tab).
fn mascot_png(c: Rgb) -> Vec<u8> {
    let side = (mascot_box().0 * 1000.0).round() / 1000.0;
    let scale = side / ICON_PX as f64;
    let rects = mascot_rects();
    palette_png(&[BLACK, c], |x, y| inside(&rects, x * scale, y * scale) as u8)
}

/// Taskbar icon: rounded tile in the window color, character in the user's violet across
/// 92 % of it.
const TILE_MARGIN: f64 = 8.0;
const TILE_RADIUS: f64 = 48.0;
const TILE_SHARE: f64 = 0.92;
const TILE_INK: Rgb = (97, 0, 138);
/// The taskbar shows the icon at 32 px, so 8 px here is one pixel there: cell edges are
/// snapped to that grid to stay sharp instead of blurring between two pixels.
const TILE_GRID: f64 = 8.0;
/// One taskbar pixel below the exact center, chosen by eye.
const TILE_SHIFT: (f64, f64) = (0.0, 8.0);

/// Character rectangles in pixels of the 256 px tile icon.
fn tile_rects() -> Vec<(f64, f64, f64, f64)> {
    let (w, h) = (MASCOT[0].len() as f64 * CELL_W, MASCOT.len() as f64 * CELL_H);
    let side = w.max(h) / TILE_SHARE;
    let k = ICON_PX as f64 / side;
    let (ox, oy) = ((side - w) / 2.0, (side - h) / 2.0);
    let snap = |v: f64| (v * k / TILE_GRID).round_ties_even() * TILE_GRID;
    let mut rects = Vec::new();
    for (y, line) in MASCOT.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch != '#' {
                continue;
            }
            let (x0, x1) = (snap(ox + x as f64 * CELL_W), snap(ox + (x + 1) as f64 * CELL_W));
            let (y0, y1) = (snap(oy + y as f64 * CELL_H), snap(oy + (y + 1) as f64 * CELL_H));
            if x1 > x0 && y1 > y0 {
                rects.push((x0 + TILE_SHIFT.0, y0 + TILE_SHIFT.1, x1 - x0, y1 - y0));
            }
        }
    }
    rects
}

fn in_tile(x: f64, y: f64) -> bool {
    let (lo, hi) = (TILE_MARGIN, ICON_PX as f64 - TILE_MARGIN);
    if x < lo || x >= hi || y < lo || y >= hi {
        return false;
    }
    let cx = x.clamp(lo + TILE_RADIUS, hi - TILE_RADIUS);
    let cy = y.clamp(lo + TILE_RADIUS, hi - TILE_RADIUS);
    (x - cx).powi(2) + (y - cy).powi(2) <= TILE_RADIUS * TILE_RADIUS
}

fn tile_png(c: Rgb) -> Vec<u8> {
    let rects = tile_rects();
    palette_png(&[BLACK, c, TILE_INK], |x, y| {
        if !in_tile(x, y) {
            0
        } else if inside(&rects, x, y) {
            2
        } else {
            1
        }
    })
}

fn tile_svg(c: Rgb) -> String {
    let size = ICON_PX as f64 - 2.0 * TILE_MARGIN;
    let rects: String = tile_rects()
        .iter()
        .map(|(x, y, w, h)| format!(r#"<rect x="{x:.3}" y="{y:.3}" width="{w:.3}" height="{h:.3}"/>"#))
        .collect();
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {ICON_PX} {ICON_PX}"><rect x="{TILE_MARGIN}" y="{TILE_MARGIN}" width="{size}" height="{size}" rx="{TILE_RADIUS}" fill="{}"/><g fill="{}" shape-rendering="crispEdges">{rects}</g></svg>
"##,
        hex(c),
        hex(TILE_INK)
    )
}

fn css_rgb(c: Rgb) -> String {
    format!("rgb({},{},{})", c.0, c.1, c.2)
}

fn capitalize(s: &str) -> String {
    let mut it = s.chars();
    it.next().map(|f| f.to_uppercase().chain(it).collect()).unwrap_or_default()
}

fn write(path: &std::path::Path, data: impl AsRef<[u8]>) {
    if let Err(e) = fs::write(path, data) {
        eprintln!("{} : {e}", path.display());
    }
}

fn main() {
    let paths = Paths::new();
    let config = var_dir("XDG_CONFIG_HOME").unwrap_or_else(|| paths.home.join(".config"));
    let themes = paths.home.join(".claude/themes");
    let apps = paths.home.join(".local/share/applications");
    for dir in [paths.conf.join("defaults"), themes.clone(), apps.clone(), paths.icons.clone()] {
        let _ = fs::create_dir_all(dir);
    }
    let mut colors = palette(&paths);
    if colors.is_empty() {
        colors = DEFAULT_COLORS.iter().map(|(n, c)| (n.to_string(), *c)).collect();
        let text: String = colors.iter().map(|(n, c)| format!("{n} {},{},{}\n", c.0, c.1, c.2)).collect();
        write(&paths.palette, text);
    }
    let rules_before = fs::read(config.join("kwinrulesrc")).unwrap_or_default();

    for (name, c) in &colors {
        let inactive = mix(*c, BLACK, 0.25); // same hue, darker when the window isn't focused
        let style = STYLE
            .replace("{name}", name)
            .replace("{a}", &hex(*c))
            .replace("{i}", &hex(inactive))
            .replace("{px}", &FRAME_PX.to_string());
        write(&paths.conf.join(format!("style-{name}.qss")), style);

        let group = format!("claude-code-{name}");
        kwrite(&group, "Description", Some(&format!("Claude Code ({name}) : sans décoration")));
        kwrite(&group, "wmclass", Some(&group));
        kwrite(&group, "wmclassmatch", Some("1"));
        kwrite(&group, "decorationpolicy", Some("none"));
        kwrite(&group, "decorationpolicyrule", Some("2"));
        kwrite(&group, "decocolor", None);
        kwrite(&group, "decocolorrule", None);

        let (light, dark) = (mix(*c, WHITE, 0.45), mix(*c, BLACK, 0.75));
        let hover = (dark.0.saturating_add(12), dark.1.saturating_add(12), dark.2.saturating_add(12));
        let theme = json!({
            "name": format!("Claude {name}"),
            "base": "dark",
            "overrides": {
                "claude": css_rgb(*c),
                "claudeShimmer": css_rgb(light),
                "promptBorder": css_rgb(*c),
                "promptBorderShimmer": css_rgb(light),
                "clawd_body": css_rgb(*c),
                "briefLabelClaude": css_rgb(*c),
                "userMessageBackground": css_rgb(dark),
                "userMessageBackgroundHover": css_rgb(hover),
            }
        });
        write(&themes.join(format!("claude-{name}.json")), serde_json::to_string_pretty(&theme).unwrap_or_default() + "\n");

        write(&paths.icons.join(format!("claude-code-{name}.svg")), mascot_svg(*c));
        write(&paths.icons.join(format!("claude-code-{name}.png")), mascot_png(*c));
        let icon = paths.icons.join(format!("claude-code-{name}-tuile.png"));
        write(&paths.icons.join(format!("claude-code-{name}-tuile.svg")), tile_svg(*c));
        write(&icon, tile_png(*c));

        // hidden entry: the taskbar shows its icon for this window
        write(
            &apps.join(format!("claude-code-{name}.desktop")),
            format!(
                "[Desktop Entry]\nType=Application\nName=Claude Code\nComment=Fenêtre Claude Code ({name})\n\
                 Exec={}\nIcon={}\nNoDisplay=true\nTerminal=false\n",
                paths.bin.join("claude-window").display(),
                icon.display()
            ),
        );

        // color scheme of the old KDE-decoration frame, no longer used
        let _ = fs::remove_file(
            paths.home.join(format!(".local/share/color-schemes/ClaudeCadre{}.colors", capitalize(name))),
        );
    }
    let names: Vec<String> = colors.iter().map(|(n, _)| n.clone()).collect();
    update_rule_order(&names);

    // Konsole settings for Claude windows only, through XDG_CONFIG_DIRS: they apply
    // only where your own konsolerc doesn't set the key
    write(
        &paths.conf.join("defaults/konsolerc"),
        "[TabBar]\nExpandTabWidth=true\nNewTabButton=false\nCloseTabButton=None\n",
    );

    if fs::read(config.join("kwinrulesrc")).unwrap_or_default() != rules_before {
        let _ = Command::new("qdbus6").args(["org.kde.KWin", "/KWin", "reconfigure"]).stderr(Stdio::null()).status();
    }
    // the taskbar picks up new .desktop icons when KDE's application cache is rebuilt
    let _ = Command::new("kbuildsycoca6").stdout(Stdio::null()).stderr(Stdio::null()).status();
    println!("ok : {}", names.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }
}
