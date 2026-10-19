use crate::models::{
    default_palettes, BookCover, ColourPalette, CoverShape, CurveFunction, InstalledFont,
    ShapeType, TextAlign, TextBlock, TextCase,
};
use eframe::egui;
use std::path::Path;

pub const PALETTES_FILE: &str = "palettes.txt";
pub const PROJECT_FILE: &str = "project.cbproj";

// ============================================================================
// ZERO-DEPENDENCY PNG ENCODER (FOR HIGH-RES RASTER EXPORT)
// ============================================================================

fn crc32_slice(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn adler32_slice(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(data);
    let crc = crc32_slice(&crc_input);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Encodes an RGBA byte buffer into a valid `.png` file without any external crates
pub fn encode_rgba_to_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth: 8
    ihdr.push(6); // colour type: 6 (RGBA)
    ihdr.push(0); // compression: 0
    ihdr.push(0); // filter: 0
    ihdr.push(0); // interlace: 0
    write_png_chunk(&mut out, b"IHDR", &ihdr);

    let row_bytes = (width as usize) * 4;
    let mut raw = Vec::with_capacity((row_bytes + 1) * (height as usize));
    for y in 0..(height as usize) {
        raw.push(0);
        let start = y * row_bytes;
        let end = start + row_bytes;
        if end <= rgba.len() {
            raw.extend_from_slice(&rgba[start..end]);
        }
    }

    let mut zlib = Vec::with_capacity(raw.len() + 64);
    zlib.push(0x78);
    zlib.push(0x01);

    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i + 1 == chunks.len();
        zlib.push(if is_last { 0x01 } else { 0x00 });
        let len = chunk.len() as u16;
        let nlen = !len;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&nlen.to_le_bytes());
        zlib.extend_from_slice(chunk);
    }
    let adler = adler32_slice(&raw);
    zlib.extend_from_slice(&adler.to_be_bytes());

    write_png_chunk(&mut out, b"IDAT", &zlib);
    write_png_chunk(&mut out, b"IEND", &[]);
    out
}

// ============================================================================
// BUILT-IN SVG FILE & PATH PARSER
// ============================================================================

fn tokenize_svg_path(d: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            tokens.push(c.to_string());
        } else if c == ',' || c.is_ascii_whitespace() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        } else if (c == '-' || c == '+')
            && !current.is_empty()
            && !current.ends_with('e')
            && !current.ends_with('E')
        {
            tokens.push(current.clone());
            current.clear();
            current.push(c);
        } else if c == '.' && current.contains('.') {
            tokens.push(current.clone());
            current.clear();
            current.push('.');
        } else {
            current.push(c);
        }
        i += 1;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub fn parse_svg_path_data(d: &str) -> Vec<Vec<egui::Vec2>> {
    let tokens = tokenize_svg_path(d);
    let mut contours: Vec<Vec<egui::Vec2>> = Vec::new();
    let mut current_contour: Vec<egui::Vec2> = Vec::new();
    let mut curr = egui::Vec2::ZERO;
    let mut start_pt = egui::Vec2::ZERO;
    let mut last_ctrl: Option<egui::Vec2> = None;

    let mut idx = 0;
    let mut cmd = 'M';

    let next_f32 = |idx: &mut usize| -> Option<f32> {
        if *idx < tokens.len() {
            if let Ok(v) = tokens[*idx].parse::<f32>() {
                *idx += 1;
                return Some(v);
            }
        }
        None
    };

    while idx < tokens.len() {
        let t = &tokens[idx];
        if t.len() == 1 && t.chars().next().unwrap().is_ascii_alphabetic() {
            cmd = t.chars().next().unwrap();
            idx += 1;
        }

        match cmd {
            'M' | 'm' => {
                if let (Some(x), Some(y)) = (next_f32(&mut idx), next_f32(&mut idx)) {
                    if current_contour.len() >= 3 {
                        contours.push(std::mem::take(&mut current_contour));
                    } else {
                        current_contour.clear();
                    }
                    curr = if cmd == 'm' {
                        curr + egui::vec2(x, y)
                    } else {
                        egui::vec2(x, y)
                    };
                    start_pt = curr;
                    current_contour.push(curr);
                    cmd = if cmd == 'm' { 'l' } else { 'L' };
                    last_ctrl = None;
                } else {
                    break;
                }
            }
            'L' | 'l' => {
                if let (Some(x), Some(y)) = (next_f32(&mut idx), next_f32(&mut idx)) {
                    curr = if cmd == 'l' {
                        curr + egui::vec2(x, y)
                    } else {
                        egui::vec2(x, y)
                    };
                    current_contour.push(curr);
                    last_ctrl = None;
                } else {
                    break;
                }
            }
            'H' | 'h' => {
                if let Some(x) = next_f32(&mut idx) {
                    curr.x = if cmd == 'h' { curr.x + x } else { x };
                    current_contour.push(curr);
                    last_ctrl = None;
                } else {
                    break;
                }
            }
            'V' | 'v' => {
                if let Some(y) = next_f32(&mut idx) {
                    curr.y = if cmd == 'v' { curr.y + y } else { y };
                    current_contour.push(curr);
                    last_ctrl = None;
                } else {
                    break;
                }
            }
            'C' | 'c' => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                ) {
                    let p0 = curr;
                    let c1 = if cmd == 'c' {
                        p0 + egui::vec2(x1, y1)
                    } else {
                        egui::vec2(x1, y1)
                    };
                    let c2 = if cmd == 'c' {
                        p0 + egui::vec2(x2, y2)
                    } else {
                        egui::vec2(x2, y2)
                    };
                    let p3 = if cmd == 'c' {
                        p0 + egui::vec2(x, y)
                    } else {
                        egui::vec2(x, y)
                    };
                    let steps = 24;
                    for s in 1..=steps {
                        let t = s as f32 / steps as f32;
                        let u = 1.0 - t;
                        let pt = p0 * (u * u * u)
                            + c1 * (3.0 * u * u * t)
                            + c2 * (3.0 * u * t * t)
                            + p3 * (t * t * t);
                        current_contour.push(pt);
                    }
                    curr = p3;
                    last_ctrl = Some(c2);
                } else {
                    break;
                }
            }
            'S' | 's' => {
                if let (Some(x2), Some(y2), Some(x), Some(y)) = (
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                ) {
                    let p0 = curr;
                    let c1 = match last_ctrl {
                        Some(lc) => p0 * 2.0 - lc,
                        None => p0,
                    };
                    let c2 = if cmd == 's' {
                        p0 + egui::vec2(x2, y2)
                    } else {
                        egui::vec2(x2, y2)
                    };
                    let p3 = if cmd == 's' {
                        p0 + egui::vec2(x, y)
                    } else {
                        egui::vec2(x, y)
                    };
                    let steps = 24;
                    for s in 1..=steps {
                        let t = s as f32 / steps as f32;
                        let u = 1.0 - t;
                        let pt = p0 * (u * u * u)
                            + c1 * (3.0 * u * u * t)
                            + c2 * (3.0 * u * t * t)
                            + p3 * (t * t * t);
                        current_contour.push(pt);
                    }
                    curr = p3;
                    last_ctrl = Some(c2);
                } else {
                    break;
                }
            }
            'Q' | 'q' => {
                if let (Some(x1), Some(y1), Some(x), Some(y)) = (
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                ) {
                    let p0 = curr;
                    let c1 = if cmd == 'q' {
                        p0 + egui::vec2(x1, y1)
                    } else {
                        egui::vec2(x1, y1)
                    };
                    let p2 = if cmd == 'q' {
                        p0 + egui::vec2(x, y)
                    } else {
                        egui::vec2(x, y)
                    };
                    let steps = 20;
                    for s in 1..=steps {
                        let t = s as f32 / steps as f32;
                        let u = 1.0 - t;
                        let pt = p0 * (u * u) + c1 * (2.0 * u * t) + p2 * (t * t);
                        current_contour.push(pt);
                    }
                    curr = p2;
                    last_ctrl = Some(c1);
                } else {
                    break;
                }
            }
            'T' | 't' => {
                if let (Some(x), Some(y)) = (next_f32(&mut idx), next_f32(&mut idx)) {
                    let p0 = curr;
                    let c1 = match last_ctrl {
                        Some(lc) => p0 * 2.0 - lc,
                        None => p0,
                    };
                    let p2 = if cmd == 't' {
                        p0 + egui::vec2(x, y)
                    } else {
                        egui::vec2(x, y)
                    };
                    let steps = 20;
                    for s in 1..=steps {
                        let t = s as f32 / steps as f32;
                        let u = 1.0 - t;
                        let pt = p0 * (u * u) + c1 * (2.0 * u * t) + p2 * (t * t);
                        current_contour.push(pt);
                    }
                    curr = p2;
                    last_ctrl = Some(c1);
                } else {
                    break;
                }
            }
            'A' | 'a' => {
                if let (
                    Some(_rx),
                    Some(_ry),
                    Some(_rot),
                    Some(_large),
                    Some(_sweep),
                    Some(x),
                    Some(y),
                ) = (
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                    next_f32(&mut idx),
                ) {
                    let end = if cmd == 'a' {
                        curr + egui::vec2(x, y)
                    } else {
                        egui::vec2(x, y)
                    };
                    current_contour.push(end);
                    curr = end;
                    last_ctrl = None;
                } else {
                    break;
                }
            }
            'Z' | 'z' => {
                curr = start_pt;
                if current_contour.len() >= 3 {
                    contours.push(std::mem::take(&mut current_contour));
                } else {
                    current_contour.clear();
                }
                last_ctrl = None;
            }
            _ => {
                idx += 1;
            }
        }
    }

    if current_contour.len() >= 3 {
        contours.push(current_contour);
    }
    contours
}

fn extract_xml_attr<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let pattern_dq = format!("{}=\"", attr_name);
    if let Some(pos) = tag.find(&pattern_dq) {
        let rest = &tag[pos + pattern_dq.len()..];
        if let Some(end) = rest.find('"') {
            return Some(&rest[..end]);
        }
    }
    let pattern_sq = format!("{}='", attr_name);
    if let Some(pos) = tag.find(&pattern_sq) {
        let rest = &tag[pos + pattern_sq.len()..];
        if let Some(end) = rest.find('\'') {
            return Some(&rest[..end]);
        }
    }
    None
}

/// Parses an entire SVG string or raw SVG `d` path string into normalised local contours centred at `(0,0)`
pub fn parse_svg_content_to_contours(svg_or_path: &str) -> Vec<Vec<egui::Vec2>> {
    let mut contours = Vec::new();
    let trimmed = svg_or_path.trim();

    if !trimmed.contains('<') {
        contours.extend(parse_svg_path_data(trimmed));
    } else {
        let mut rest = trimmed;
        while let Some(start) = rest.find('<') {
            if let Some(end) = rest[start..].find('>') {
                let tag = &rest[start..start + end + 1];
                if tag.starts_with("<path") {
                    if let Some(d) = extract_xml_attr(tag, "d") {
                        contours.extend(parse_svg_path_data(d));
                    }
                } else if tag.starts_with("<polygon") || tag.starts_with("<polyline") {
                    if let Some(pts_str) = extract_xml_attr(tag, "points") {
                        let toks = tokenize_svg_path(pts_str);
                        let mut poly = Vec::new();
                        let mut i = 0;
                        while i + 1 < toks.len() {
                            if let (Ok(x), Ok(y)) =
                                (toks[i].parse::<f32>(), toks[i + 1].parse::<f32>())
                            {
                                poly.push(egui::vec2(x, y));
                            }
                            i += 2;
                        }
                        if poly.len() >= 3 {
                            contours.push(poly);
                        }
                    }
                } else if tag.starts_with("<rect") {
                    let x = extract_xml_attr(tag, "x")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let y = extract_xml_attr(tag, "y")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let w = extract_xml_attr(tag, "width")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let h = extract_xml_attr(tag, "height")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    if w > 0.0 && h > 0.0 {
                        contours.push(vec![
                            egui::vec2(x, y),
                            egui::vec2(x + w, y),
                            egui::vec2(x + w, y + h),
                            egui::vec2(x, y + h),
                        ]);
                    }
                } else if tag.starts_with("<circle") {
                    let cx = extract_xml_attr(tag, "cx")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let cy = extract_xml_attr(tag, "cy")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let r = extract_xml_attr(tag, "r")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    if r > 0.0 {
                        let steps = 64;
                        let poly: Vec<egui::Vec2> = (0..steps)
                            .map(|i| {
                                let a = (i as f32 / steps as f32) * std::f32::consts::TAU;
                                egui::vec2(cx + r * a.cos(), cy + r * a.sin())
                            })
                            .collect();
                        contours.push(poly);
                    }
                } else if tag.starts_with("<ellipse") {
                    let cx = extract_xml_attr(tag, "cx")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let cy = extract_xml_attr(tag, "cy")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let rx = extract_xml_attr(tag, "rx")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let ry = extract_xml_attr(tag, "ry")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    if rx > 0.0 && ry > 0.0 {
                        let steps = 64;
                        let poly: Vec<egui::Vec2> = (0..steps)
                            .map(|i| {
                                let a = (i as f32 / steps as f32) * std::f32::consts::TAU;
                                egui::vec2(cx + rx * a.cos(), cy + ry * a.sin())
                            })
                            .collect();
                        contours.push(poly);
                    }
                }
                rest = &rest[start + end + 1..];
            } else {
                break;
            }
        }
    }

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;

    for c in &contours {
        for p in c {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
    }

    if min_x < max_x && min_y < max_y {
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        let max_dim = (max_x - min_x).max(max_y - min_y).max(1e-4);
        let norm_scale = 100.0 / max_dim;
        for c in &mut contours {
            for p in c {
                p.x = (p.x - cx) * norm_scale;
                p.y = (p.y - cy) * norm_scale;
            }
        }
    }

    contours
}

// ============================================================================
// SYSTEM FONTS (UNLIMITED DISCOVERY + ON-DEMAND LOADING) & FILE EXPLORER
// ============================================================================

fn scan_font_paths(dir: &Path, out: &mut Vec<(String, std::path::PathBuf)>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if out.len() >= limit {
                break;
            }
            let path = entry.path();
            if path.is_dir() {
                scan_font_paths(&path, out, limit);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let clean_name = stem.replace(['-', '_'], " ");
                        if !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&clean_name)) {
                            out.push((clean_name, path));
                        }
                    }
                }
            }
        }
    }
}

pub fn load_system_fonts(_ctx: &egui::Context) -> Vec<InstalledFont> {
    let mut list = vec![
        InstalledFont {
            display_name: "Default Sans-Serif".to_string(),
            egui_family: egui::FontFamily::Proportional,
            svg_family: "sans-serif".to_string(),
            file_path: None,
        },
        InstalledFont {
            display_name: "Default Monospace".to_string(),
            egui_family: egui::FontFamily::Monospace,
            svg_family: "monospace".to_string(),
            file_path: None,
        },
    ];

    let mut search_dirs = Vec::new();

    // 1. Prioritise local project folder & user's personal font directories first!
    search_dirs.push(std::path::PathBuf::from("fonts"));
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        search_dirs.push(Path::new(&xdg_data).join("fonts"));
    }
    if let Ok(home) = std::env::var("HOME") {
        search_dirs.push(Path::new(&home).join(".local/share/fonts"));
        search_dirs.push(Path::new(&home).join(".fonts"));
        search_dirs.push(Path::new(&home).join("Library/Fonts"));
    }
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        search_dirs.push(Path::new(&local_appdata).join("Microsoft\\Windows\\Fonts"));
    }

    // 2. Standard system-wide font directories
    search_dirs.extend([
        std::path::PathBuf::from("/usr/local/share/fonts"),
        std::path::PathBuf::from("/usr/share/fonts"),
        std::path::PathBuf::from("/run/current-system/sw/share/X11/fonts"),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/fonts"),
        std::path::PathBuf::from("/Library/Fonts"),
        std::path::PathBuf::from("/System/Library/Fonts"),
        std::path::PathBuf::from("C:\\Windows\\Fonts"),
    ]);

    let mut discovered = Vec::new();
    for dir in search_dirs {
        if dir.exists() {
            // Scan up to 3,500 font files across all directories
            scan_font_paths(&dir, &mut discovered, 3500);
        }
    }

    discovered.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    for (name, path) in discovered {
        let family_key = egui::FontFamily::Name(name.clone().into());
        list.push(InstalledFont {
            display_name: name.clone(),
            egui_family: family_key,
            svg_family: format!("'{}', sans-serif", name),
            file_path: Some(path),
        });
    }

    list
}

/// Ensures that a specific font index is loaded into egui's active `FontDefinitions` on demand
pub fn ensure_font_loaded(
    ctx: &egui::Context,
    installed_fonts: &[InstalledFont],
    loaded_set: &mut std::collections::HashSet<usize>,
    font_Defs: &mut egui::FontDefinitions,
    font_index: usize,
) {
    if font_index <= 1 || loaded_set.contains(&font_index) {
        return;
    }
    if let Some(font_info) = installed_fonts.get(font_index) {
        if let Some(path) = &font_info.file_path {
            if let Ok(bytes) = std::fs::read(path) {
                let name = font_info.display_name.clone();
                font_Defs
                    .font_data
                    .insert(name.clone(), egui::FontData::from_owned(bytes).into());
                font_Defs.families.insert(
                    font_info.egui_family.clone(),
                    vec![name, "Hack".to_string(), "Ubuntu-Light".to_string()],
                );
                ctx.set_fonts(font_Defs.clone());
            }
        }
        loaded_set.insert(font_index);
    }
}

/// Scans the current directory for all `.cbproj` project files
pub fn scan_saved_projects() -> Vec<String> {
    let mut projects = Vec::new();
    if let Ok(entries) = std::fs::read_dir(".") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("cbproj") {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            projects.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    projects.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    if projects.is_empty() {
        projects.push(PROJECT_FILE.to_string());
    }
    projects
}

/// Opens a native OS File Explorer window (Linux zenity/kdialog, macOS osascript, Windows PowerShell)
pub fn open_native_file_dialog(title: &str, save_mode: bool, filter_ext: &str) -> Option<String> {
    use std::process::Command;

    if cfg!(target_os = "linux") {
        let mut args = vec!["--file-selection", "--title", title];
        let filter_arg = format!("*.{}", filter_ext);
        if save_mode {
            args.push("--save");
            args.push("--confirm-overwrite");
        }
        args.push("--file-filter");
        args.push(&filter_arg);

        if let Ok(out) = Command::new("zenity").args(&args).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        let k_arg = if save_mode {
            "--getsavefilename"
        } else {
            "--getopenfilename"
        };
        if let Ok(out) = Command::new("kdialog")
            .args([k_arg, ".", &format!("*.{}", filter_ext)])
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        let script = if save_mode {
            format!(
                "POSIX path of (choose file name with prompt \"{}\")",
                title
            )
        } else {
            format!("POSIX path of (choose file with prompt \"{}\")", title)
        };
        if let Ok(out) = Command::new("osascript").args(["-e", &script]).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    } else if cfg!(target_os = "windows") {
        let dlg_class = if save_mode {
            "System.Windows.Forms.SaveFileDialog"
        } else {
            "System.Windows.Forms.OpenFileDialog"
        };
        let ps = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object {}; $f.Filter = 'Files (*.{})|*.{}|All (*.*)|*.*'; if ($f.ShowDialog() -eq 'OK') {{ $f.FileName }}",
            dlg_class, filter_ext, filter_ext
        );
        if let Ok(out) = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

pub fn colour_to_rgba_str(c: egui::Color32) -> String {
    format!("{},{},{},{}", c.r(), c.g(), c.b(), c.a())
}

pub fn parse_rgba_str(s: &str) -> Option<egui::Color32> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 4 {
        let r = parts[0].trim().parse::<u8>().ok()?;
        let g = parts[1].trim().parse::<u8>().ok()?;
        let b = parts[2].trim().parse::<u8>().ok()?;
        let a = parts[3].trim().parse::<u8>().ok()?;
        Some(egui::Color32::from_rgba_unmultiplied(r, g, b, a))
    } else {
        None
    }
}

pub fn load_custom_palettes() -> Vec<ColourPalette> {
    let mut palettes = default_palettes();
    if let Ok(content) = std::fs::read_to_string(PALETTES_FILE) {
        for line in content.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() == 5 {
                if let (Some(bg), Some(inner), Some(text), Some(shape)) = (
                    parse_rgba_str(parts[1]),
                    parse_rgba_str(parts[2]),
                    parse_rgba_str(parts[3]),
                    parse_rgba_str(parts[4]),
                ) {
                    palettes.push(ColourPalette {
                        name: parts[0].to_string(),
                        bg_colour: bg,
                        inner_colour: inner,
                        text_colour: text,
                        shape_colour: shape,
                        built_in: false,
                    });
                }
            }
        }
    }
    palettes
}

pub fn save_custom_palettes(palettes: &[ColourPalette]) -> std::io::Result<()> {
    let mut out = String::new();
    for p in palettes.iter().filter(|p| !p.built_in) {
        let safe_name = p.name.replace('|', "/");
        out.push_str(&format!(
            "{}|{}|{}|{}|{}\n",
            safe_name,
            colour_to_rgba_str(p.bg_colour),
            colour_to_rgba_str(p.inner_colour),
            colour_to_rgba_str(p.text_colour),
            colour_to_rgba_str(p.shape_colour),
        ));
    }
    std::fs::write(PALETTES_FILE, out)
}

fn serialize_contours(contours: &[Vec<egui::Vec2>]) -> String {
    contours
        .iter()
        .map(|c| {
            c.iter()
                .map(|p| format!("{:.2}:{:.2}", p.x, p.y))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn deserialize_contours(s: &str) -> Vec<Vec<egui::Vec2>> {
    s.split(';')
        .filter(|part| !part.is_empty())
        .map(|c_str| {
            c_str
                .split(',')
                .filter_map(|pt_str| {
                    let (xs, ys) = pt_str.split_once(':')?;
                    Some(egui::vec2(xs.parse().ok()?, ys.parse().ok()?))
                })
                .collect::<Vec<_>>()
        })
        .filter(|c| c.len() >= 3)
        .collect()
}
// ============================================================================
// SVG EXPORT & PROJECT SAVE/LOAD
// ============================================================================

impl BookCover {
    pub fn colour_to_hex(c: egui::Color32) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
    }

    pub fn colour_opacity(c: egui::Color32) -> f32 {
        c.a() as f32 / 255.0
    }

    fn render_shape_svg_element(
        &self,
        shape: &CoverShape,
        centre_x: f32,
        centre_y: f32,
        inner_x: f32,
        inner_y: f32,
        frame_w: f32,
        frame_h: f32,
    ) -> String {
        if !shape.visible {
            return String::new();
        }
        let mut out = String::new();
        let repeats = shape.repeat_count.max(1);

        for r_idx in 0..repeats {
            let x = centre_x + shape.x_offset + (r_idx as f32) * shape.repeat_step_x;
            let y = centre_y + shape.y_offset + (r_idx as f32) * shape.repeat_step_y;
            let rot = shape.rotation_deg;

            let fill_attr = if shape.filled {
                format!(
                    r#"fill="{}" fill-opacity="{:.3}""#,
                    Self::colour_to_hex(shape.colour),
                    Self::colour_opacity(shape.colour)
                )
            } else {
                r#"fill="none""#.to_string()
            };

            let stroke_attr = if shape.outlined {
                format!(
                    r#" stroke="{}" stroke-opacity="{:.3}" stroke-width="{}" stroke-linejoin="round""#,
                    Self::colour_to_hex(shape.stroke_colour),
                    Self::colour_opacity(shape.stroke_colour),
                    shape.stroke_width
                )
            } else {
                String::new()
            };

            match &shape.shape_type {
                ShapeType::Ellipse {
                    radius_x, radius_y, ..
                } => {
                    out.push_str(&format!(
                        r#"    <ellipse cx="{x:.2}" cy="{y:.2}" rx="{radius_x:.2}" ry="{radius_y:.2}" {fill_attr}{stroke_attr} transform="rotate({rot} {x:.2} {y:.2})" />"#
                    ));
                }
                ShapeType::HalfEllipse {
                    radius_x, radius_y, ..
                } => {
                    let x_start = x - radius_x;
                    let x_end = x + radius_x;
                    out.push_str(&format!(
                        r#"    <path d="M {x_start:.2} {y:.2} A {radius_x:.2} {radius_y:.2} 0 0 1 {x_end:.2} {y:.2} Z" {fill_attr}{stroke_attr} transform="rotate({rot} {x:.2} {y:.2})" />"#
                    ));
                }
                ShapeType::HorizontalLine { thickness } => {
                    let x1 = if shape.clip_to_window { inner_x } else { 0.0 };
                    let x2 = if shape.clip_to_window {
                        inner_x + frame_w
                    } else {
                        self.cover_width
                    };
                    let line_hex = Self::colour_to_hex(shape.colour);
                    let line_op = Self::colour_opacity(shape.colour);
                    out.push_str(&format!(
                        r#"    <line x1="{x1:.2}" y1="{y:.2}" x2="{x2:.2}" y2="{y:.2}" stroke="{line_hex}" stroke-opacity="{line_op:.3}" stroke-width="{thickness}" />"#
                    ));
                }
                ShapeType::VerticalLine { thickness } => {
                    let y1 = if shape.clip_to_window { inner_y } else { 0.0 };
                    let y2 = if shape.clip_to_window {
                        inner_y + frame_h
                    } else {
                        self.cover_height
                    };
                    let line_hex = Self::colour_to_hex(shape.colour);
                    let line_op = Self::colour_opacity(shape.colour);
                    out.push_str(&format!(
                        r#"    <line x1="{x:.2}" y1="{y1:.2}" x2="{x:.2}" y2="{y2:.2}" stroke="{line_hex}" stroke-opacity="{line_op:.3}" stroke-width="{thickness}" />"#
                    ));
                }
                _ => {
                    let contours = shape.rotated_contours();
                    let mut d = String::new();
                    for c in contours {
                        if let Some(first) = c.first() {
                            d.push_str(&format!("M {:.2} {:.2} ", x + first.x, y + first.y));
                            for p in c.iter().skip(1) {
                                d.push_str(&format!("L {:.2} {:.2} ", x + p.x, y + p.y));
                            }
                            d.push_str("Z ");
                        }
                    }
                    if !d.is_empty() {
                        out.push_str(&format!(
                            r#"    <path d="{}" {fill_attr}{stroke_attr} />"#,
                            d.trim()
                        ));
                    }
                }
            }
            out.push('\n');
        }
        out
    }

    pub fn to_svg(&self, installed_fonts: &[InstalledFont]) -> String {
        let width = self.cover_width;
        let height = self.cover_height;
        let out_w = (width * 2.0).round() as i32;
        let out_h = (height * 2.0).round() as i32;
        let frame_w = self.frame_width;
        let frame_h = self.frame_height;
        let inner_x = (width / 2.0) - (frame_w / 2.0) + self.frame_x_offset;
        let inner_y = self.frame_y_offset;
        let rounding = self.frame_rounding;
        let centre_x = inner_x + (frame_w / 2.0);
        let centre_y = inner_y + (frame_h / 2.0);

        let bg_hex = Self::colour_to_hex(self.bg_colour);
        let bg_op = Self::colour_opacity(self.bg_colour);
        let inner_hex = Self::colour_to_hex(self.inner_colour);
        let inner_op = Self::colour_opacity(self.inner_colour);

        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{out_w}" height="{out_h}" shape-rendering="geometricPrecision" text-rendering="geometricPrecision">
  <defs>
    <clipPath id="inner-clip">
      <rect x="{inner_x}" y="{inner_y}" width="{frame_w}" height="{frame_h}" rx="{rounding}" />
    </clipPath>
"#
        );

        if self.bg_gradient {
            svg.push_str(&format!(
                r#"    <linearGradient id="bg-grad" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="{}" stop-opacity="{:.3}" />
      <stop offset="100%" stop-color="{}" stop-opacity="{:.3}" />
    </linearGradient>
"#,
                bg_hex,
                bg_op,
                Self::colour_to_hex(self.bg_colour_bottom),
                Self::colour_opacity(self.bg_colour_bottom)
            ));
        }

        if self.inner_gradient {
            svg.push_str(&format!(
                r#"    <linearGradient id="inner-grad" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="{}" stop-opacity="{:.3}" />
      <stop offset="100%" stop-color="{}" stop-opacity="{:.3}" />
    </linearGradient>
"#,
                inner_hex,
                inner_op,
                Self::colour_to_hex(self.inner_colour_bottom),
                Self::colour_opacity(self.inner_colour_bottom)
            ));
        }

        // Vintage Paper Grain SVG Filter
        if self.paper_grain > 0.01 {
            let slope = (self.paper_grain * 0.35).clamp(0.02, 0.45);
            svg.push_str(&format!(
                r#"    <filter id="paper-grain" x="0%" y="0%" width="100%" height="100%">
      <feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves="3" result="noise" />
      <feColorMatrix type="saturate" values="0" in="noise" result="mono" />
      <feComponentTransfer in="mono" result="grain">
        <feFuncA type="linear" slope="{slope:.3}" />
      </feComponentTransfer>
      <feBlend mode="multiply" in="SourceGraphic" in2="grain" />
    </filter>
"#
            ));
        }

        // Pre-declare curved <path> definitions for any TextBlocks using `arch_bend`
        for (tb_idx, tb) in self.text_blocks.iter().enumerate() {
            if !tb.visible || tb.content.is_empty() || tb.arch_bend.abs() <= 1.0 {
                continue;
            }
            let formatted = tb.case_transform.apply(&tb.content);
            let lines: Vec<&str> = formatted.lines().collect();
            let total_h = (lines.len().saturating_sub(1) as f32) * tb.line_spacing;

            for (line_idx, line) in lines.iter().enumerate() {
                if line.len() <= 1 {
                    continue;
                }
                let local_line_y = -total_h * 0.5 + (line_idx as f32) * tb.line_spacing;
                let n = line.chars().count();
                let arc_rad = tb.arch_bend.to_radians();
                let approx_w = (n as f32) * (tb.font_size * 0.55 + tb.letter_spacing).max(2.0);
                let radius = (approx_w / arc_rad.abs()).max(20.0);

                // Span a wide semi-circle (-85 deg to +85 deg) so startOffset="50%" always centers cleanly
                let half_span = 85.0_f32.to_radians();
                let (sx, sy, ex, ey, sweep) = if tb.arch_bend > 0.0 {
                    (
                        -radius * half_span.sin(),
                        local_line_y + radius * (1.0 - half_span.cos()),
                        radius * half_span.sin(),
                        local_line_y + radius * (1.0 - half_span.cos()),
                        1,
                    )
                } else {
                    (
                        -radius * half_span.sin(),
                        local_line_y - radius * (1.0 - half_span.cos()),
                        radius * half_span.sin(),
                        local_line_y - radius * (1.0 - half_span.cos()),
                        0,
                    )
                };

                svg.push_str(&format!(
                    r#"    <path id="text-arch-{tb_idx}-{line_idx}" d="M {sx:.2} {sy:.2} A {radius:.2} {radius:.2} 0 0 {sweep} {ex:.2} {ey:.2}" fill="none" />
"#
                ));
            }
        }

        svg.push_str("  </defs>\n");

        // Outer Background
        if self.bg_gradient {
            svg.push_str(&format!(
                r#"  <rect width="{width}" height="{height}" rx="4" fill="url(#bg-grad)" />"#
            ));
        } else {
            svg.push_str(&format!(
                r#"  <rect width="{width}" height="{height}" rx="4" fill="{bg_hex}" fill-opacity="{bg_op:.3}" />"#
            ));
        }
        svg.push('\n');

        // Inner Window
        let win_stroke = if self.frame_outlined {
            format!(
                r#" stroke="{}" stroke-opacity="{:.3}" stroke-width="{}""#,
                Self::colour_to_hex(self.frame_stroke_colour),
                Self::colour_opacity(self.frame_stroke_colour),
                self.frame_stroke_width
            )
        } else {
            String::new()
        };

        if self.inner_gradient {
            svg.push_str(&format!(
                r#"  <rect x="{inner_x}" y="{inner_y}" width="{frame_w}" height="{frame_h}" rx="{rounding}" fill="url(#inner-grad)"{win_stroke} />"#
            ));
        } else {
            svg.push_str(&format!(
                r#"  <rect x="{inner_x}" y="{inner_y}" width="{frame_w}" height="{frame_h}" rx="{rounding}" fill="{inner_hex}" fill-opacity="{inner_op:.3}"{win_stroke} />"#
            ));
        }
        svg.push('\n');

        // Clipped Shapes
        svg.push_str("  <g clip-path=\"url(#inner-clip)\">\n");
        for shape in self.shapes.iter().filter(|s| s.clip_to_window) {
            svg.push_str(&self.render_shape_svg_element(
                shape, centre_x, centre_y, inner_x, inner_y, frame_w, frame_h,
            ));
        }
        svg.push_str("  </g>\n");

        // Unclipped Shapes (break outside the window)
        for shape in self.shapes.iter().filter(|s| !s.clip_to_window) {
            svg.push_str(&self.render_shape_svg_element(
                shape, centre_x, centre_y, inner_x, inner_y, frame_w, frame_h,
            ));
        }

        // Text Blocks (with Banner Box, Shadow, Faux Bold, Outlines, and Curved Arch support)
        for (tb_idx, tb) in self.text_blocks.iter().enumerate() {
            if !tb.visible || tb.content.is_empty() {
                continue;
            }
            let font_family = installed_fonts
                .get(tb.font_index)
                .map(|f| f.svg_family.as_str())
                .unwrap_or("sans-serif");
            let formatted = tb.case_transform.apply(&tb.content);
            let lines: Vec<&str> = formatted.lines().collect();
            let tx = (width / 2.0) + tb.x_offset;
            let ty = tb.y_pos;
            let rot = tb.rotation_deg;
            let t_hex = Self::colour_to_hex(tb.colour);
            let t_op = Self::colour_opacity(tb.colour);

            let anchor_str = match tb.alignment {
                TextAlign::Left => "start",
                TextAlign::Centre => "middle",
                TextAlign::Right => "end",
            };

            let weight_str = if tb.faux_bold { "900" } else { "bold" };

            let stroke_attr = if tb.outlined {
                format!(
                    r#" stroke="{}" stroke-opacity="{:.3}" stroke-width="{}" paint-order="stroke fill" stroke-linejoin="round""#,
                    Self::colour_to_hex(tb.outline_colour),
                    Self::colour_opacity(tb.outline_colour),
                    tb.outline_width * 2.0
                )
            } else if tb.faux_bold {
                format!(
                    r#" stroke="{t_hex}" stroke-opacity="{t_op:.3}" stroke-width="0.8" paint-order="stroke fill""#
                )
            } else {
                String::new()
            };

            if tb.clip_to_window {
                svg.push_str("  <g clip-path=\"url(#inner-clip)\">\n");
            }
            svg.push_str(&format!(
                "  <g transform=\"translate({tx:.2} {ty:.2}) rotate({rot})\">\n"
            ));

            let total_h = (lines.len().saturating_sub(1) as f32) * tb.line_spacing;

            // Optional Banner Box behind text
            if tb.banner_box {
                let max_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f32;
                let approx_line_w = (max_chars * (tb.font_size * 0.56 + tb.letter_spacing)).max(20.0);
                let bw = approx_line_w + tb.banner_padding.x * 2.0;
                let bh = total_h + tb.font_size + tb.banner_padding.y * 2.0;
                let bx = -bw * 0.5;
                let by = -bh * 0.5;
                let ban_hex = Self::colour_to_hex(tb.banner_colour);
                let ban_op = Self::colour_opacity(tb.banner_colour);
                let ban_r = tb.banner_rounding;
                svg.push_str(&format!(
                    r#"    <rect x="{bx:.2}" y="{by:.2}" width="{bw:.2}" height="{bh:.2}" rx="{ban_r:.2}" fill="{ban_hex}" fill-opacity="{ban_op:.3}" />
"#
                ));
            }

            let start_y = -total_h * 0.5 + (tb.font_size * 0.34);

            for (line_idx, line) in lines.iter().enumerate() {
                let escaped_line = escape_xml(line);
                let ly = start_y + (line_idx as f32 * tb.line_spacing);

                if tb.arch_bend.abs() > 1.0 && line.len() > 1 {
                    // Shadow pass on curved arch path
                    if tb.shadow {
                        let sh_hex = Self::colour_to_hex(tb.shadow_colour);
                        let sh_op = Self::colour_opacity(tb.shadow_colour);
                        let sx = tb.shadow_offset.x;
                        let sy = tb.shadow_offset.y;
                        svg.push_str(&format!(
                            r##"    <text transform="translate({sx:.2} {sy:.2})" text-anchor="middle" fill="{sh_hex}" fill-opacity="{sh_op:.3}" font-family="{font_family}" font-weight="{weight_str}" font-size="{}" letter-spacing="{}px"><textPath href="#text-arch-{tb_idx}-{line_idx}" startOffset="50%">{escaped_line}</textPath></text>
"##,
                            tb.font_size, tb.letter_spacing
                        ));
                    }

                    // Main pass on curved arch path
                    svg.push_str(&format!(
                        r##"    <text text-anchor="middle" fill="{t_hex}" fill-opacity="{t_op:.3}" font-family="{font_family}" font-weight="{weight_str}" font-size="{}" letter-spacing="{}px"{stroke_attr}><textPath href="#text-arch-{tb_idx}-{line_idx}" startOffset="50%">{escaped_line}</textPath></text>
"##,
                        tb.font_size, tb.letter_spacing
                    ));
                } else {
                    // Shadow pass on flat line
                    if tb.shadow {
                        let sh_hex = Self::colour_to_hex(tb.shadow_colour);
                        let sh_op = Self::colour_opacity(tb.shadow_colour);
                        let sx = tb.shadow_offset.x;
                        let sy = ly + tb.shadow_offset.y;
                        svg.push_str(&format!(
                            r##"    <text x="{sx:.2}" y="{sy:.2}" text-anchor="{anchor_str}" fill="{sh_hex}" fill-opacity="{sh_op:.3}" font-family="{font_family}" font-weight="{weight_str}" font-size="{}" letter-spacing="{}px">{escaped_line}</text>
"##,
                            tb.font_size, tb.letter_spacing
                        ));
                    }

                    // Main pass on flat line
                    svg.push_str(&format!(
                        r##"    <text x="0" y="{ly:.2}" text-anchor="{anchor_str}" fill="{t_hex}" fill-opacity="{t_op:.3}" font-family="{font_family}" font-weight="{weight_str}" font-size="{}" letter-spacing="{}px"{stroke_attr}>{escaped_line}</text>
"##,
                        tb.font_size, tb.letter_spacing
                    ));
                }
            }

            svg.push_str("  </g>\n");
            if tb.clip_to_window {
                svg.push_str("  </g>\n");
            }
        }

        // Vintage Paper Grain Overlay Rect
        if self.paper_grain > 0.01 {
            svg.push_str(&format!(
                r##"  <rect width="{width}" height="{height}" rx="4" fill="#ffffff" fill-opacity="0.01" filter="url(#paper-grain)" pointer-events="none" />
"##
            ));
        }

        svg.push_str("</svg>\n");
        svg
    }

    pub fn save_project(&self, path: &str) -> std::io::Result<()> {
        let mut out = String::new();
        out.push_str(&format!("cover_w={}\n", self.cover_width));
        out.push_str(&format!("cover_h={}\n", self.cover_height));
        out.push_str(&format!("bg_col={}\n", colour_to_rgba_str(self.bg_colour)));
        out.push_str(&format!("bg_grad={}\n", self.bg_gradient));
        out.push_str(&format!(
            "bg_col_bot={}\n",
            colour_to_rgba_str(self.bg_colour_bottom)
        ));
        out.push_str(&format!(
            "inner_col={}\n",
            colour_to_rgba_str(self.inner_colour)
        ));
        out.push_str(&format!("inner_grad={}\n", self.inner_gradient));
        out.push_str(&format!(
            "inner_col_bot={}\n",
            colour_to_rgba_str(self.inner_colour_bottom)
        ));
        out.push_str(&format!("paper_grain={}\n", self.paper_grain));
        out.push_str(&format!(
            "def_shape_col={}\n",
            colour_to_rgba_str(self.default_shape_colour)
        ));
        out.push_str(&format!("frame_w={}\n", self.frame_width));
        out.push_str(&format!("frame_h={}\n", self.frame_height));
        out.push_str(&format!("frame_x={}\n", self.frame_x_offset));
        out.push_str(&format!("frame_y={}\n", self.frame_y_offset));
        out.push_str(&format!("frame_r={}\n", self.frame_rounding));
        out.push_str(&format!("frame_out={}\n", self.frame_outlined));
        out.push_str(&format!("frame_sw={}\n", self.frame_stroke_width));
        out.push_str(&format!(
            "frame_sc={}\n",
            colour_to_rgba_str(self.frame_stroke_colour)
        ));

        for tb in &self.text_blocks {
            let safe_name = tb.name.replace('|', "/");
            let safe_content = tb.content.replace('|', "/").replace('\n', "\\n");
            out.push_str(&format!(
                "textblock={}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                safe_name,
                safe_content,
                tb.font_index,
                tb.font_size,
                tb.line_spacing,
                tb.letter_spacing,
                tb.x_offset,
                tb.y_pos,
                tb.rotation_deg,
                colour_to_rgba_str(tb.colour),
                tb.case_transform.to_id(),
                tb.alignment.to_id(),
                tb.faux_bold,
                tb.outlined,
                colour_to_rgba_str(tb.outline_colour),
                tb.outline_width,
                tb.shadow,
                colour_to_rgba_str(tb.shadow_colour),
                tb.shadow_offset.x,
                tb.shadow_offset.y,
                tb.banner_box,
                colour_to_rgba_str(tb.banner_colour),
                tb.banner_padding.x,
                tb.banner_padding.y,
                tb.banner_rounding,
                tb.arch_bend,
                tb.clip_to_window,
                tb.visible,
            ));
        }

        for s in &self.shapes {
            let safe_name = s.name.replace('|', "/");
            let common = format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                safe_name,
                s.x_offset,
                s.y_offset,
                s.rotation_deg,
                colour_to_rgba_str(s.colour),
                s.filled,
                s.outlined,
                s.stroke_width,
                colour_to_rgba_str(s.stroke_colour),
                s.enable_outer_align,
                s.align_offset,
                s.visible,
                s.locked,
                s.clip_to_window,
                s.repeat_count,
                s.repeat_step_x,
                s.repeat_step_y
            );
            let spec = match &s.shape_type {
                ShapeType::Ellipse {
                    radius_x,
                    radius_y,
                    lock_aspect,
                } => format!("Ellipse|{}|{}|{}", radius_x, radius_y, lock_aspect),
                ShapeType::HalfEllipse {
                    radius_x,
                    radius_y,
                    lock_aspect,
                } => format!("HalfEllipse|{}|{}|{}", radius_x, radius_y, lock_aspect),
                ShapeType::HorizontalLine { thickness } => format!("HLine|{}", thickness),
                ShapeType::VerticalLine { thickness } => format!("VLine|{}", thickness),
                ShapeType::Rectangle {
                    width,
                    height,
                    curve_func,
                    link_curves,
                    curve_top,
                    curve_right,
                    curve_bottom,
                    curve_left,
                } => format!(
                    "Rect|{}|{}|{}|{}|{}|{}|{}|{}",
                    width,
                    height,
                    curve_func.to_id(),
                    link_curves,
                    curve_top,
                    curve_right,
                    curve_bottom,
                    curve_left
                ),
                ShapeType::Triangle {
                    side_a,
                    side_b,
                    side_c,
                    curve_func,
                    curve_a,
                    curve_b,
                    curve_c,
                } => format!(
                    "Tri|{}|{}|{}|{}|{}|{}|{}",
                    side_a,
                    side_b,
                    side_c,
                    curve_func.to_id(),
                    curve_a,
                    curve_b,
                    curve_c
                ),
                ShapeType::PolygonStar {
                    sides,
                    outer_radius,
                    inner_radius,
                    use_star,
                    curve_func,
                    edge_curve,
                } => format!(
                    "Poly|{}|{}|{}|{}|{}|{}",
                    sides,
                    outer_radius,
                    inner_radius,
                    use_star,
                    curve_func.to_id(),
                    edge_curve
                ),
                ShapeType::ImportedSvg {
                    source_path,
                    scale_x,
                    scale_y,
                    lock_aspect,
                    contours,
                } => format!(
                    "Svg|{}|{}|{}|{}|{}",
                    source_path.replace('|', "/"),
                    scale_x,
                    scale_y,
                    lock_aspect,
                    serialize_contours(contours)
                ),
            };
            out.push_str(&format!("shape2={}|{}\n", common, spec));
        }

        std::fs::write(path, out)
    }

    pub fn load_project(path: &str) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut cover = BookCover::default();
        cover.shapes.clear();
        let mut loaded_text_blocks = Vec::new();

        for line in content.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k {
                    "cover_w" => cover.cover_width = v.parse().unwrap_or(380.0),
                    "cover_h" => cover.cover_height = v.parse().unwrap_or(560.0),
                    "bg_col" => {
                        if let Some(c) = parse_rgba_str(v) {
                            cover.bg_colour = c;
                        }
                    }
                    "bg_grad" => cover.bg_gradient = v.parse().unwrap_or(false),
                    "bg_col_bot" => {
                        if let Some(c) = parse_rgba_str(v) {
                            cover.bg_colour_bottom = c;
                        }
                    }
                    "inner_col" => {
                        if let Some(c) = parse_rgba_str(v) {
                            cover.inner_colour = c;
                        }
                    }
                    "inner_grad" => cover.inner_gradient = v.parse().unwrap_or(false),
                    "inner_col_bot" => {
                        if let Some(c) = parse_rgba_str(v) {
                            cover.inner_colour_bottom = c;
                        }
                    }
                    "paper_grain" => cover.paper_grain = v.parse().unwrap_or(0.0),
                    "def_shape_col" => {
                        if let Some(c) = parse_rgba_str(v) {
                            cover.default_shape_colour = c;
                        }
                    }
                    "frame_w" => cover.frame_width = v.parse().unwrap_or(324.0),
                    "frame_h" => cover.frame_height = v.parse().unwrap_or(324.0),
                    "frame_x" => cover.frame_x_offset = v.parse().unwrap_or(0.0),
                    "frame_y" => cover.frame_y_offset = v.parse().unwrap_or(75.0),
                    "frame_r" => cover.frame_rounding = v.parse().unwrap_or(4.0),
                    "frame_out" => cover.frame_outlined = v.parse().unwrap_or(false),
                    "frame_sw" => cover.frame_stroke_width = v.parse().unwrap_or(2.0),
                    "frame_sc" => {
                        if let Some(c) = parse_rgba_str(v) {
                            cover.frame_stroke_colour = c;
                        }
                    }
                    "textblock" => {
                        let p: Vec<&str> = v.split('|').collect();
                        if p.len() >= 28 {
                            loaded_text_blocks.push(TextBlock {
                                name: p[0].to_string(),
                                content: p[1].replace("\\n", "\n"),
                                font_index: p[2].parse().unwrap_or(0),
                                font_size: p[3].parse().unwrap_or(34.0),
                                line_spacing: p[4].parse().unwrap_or(38.0),
                                letter_spacing: p[5].parse().unwrap_or(0.0),
                                x_offset: p[6].parse().unwrap_or(0.0),
                                y_pos: p[7].parse().unwrap_or(38.0),
                                rotation_deg: p[8].parse().unwrap_or(0.0),
                                colour: parse_rgba_str(p[9]).unwrap_or(egui::Color32::WHITE),
                                case_transform: TextCase::from_id(p[10]),
                                alignment: TextAlign::from_id(p[11]),
                                faux_bold: p[12].parse().unwrap_or(false),
                                outlined: p[13].parse().unwrap_or(false),
                                outline_colour: parse_rgba_str(p[14])
                                    .unwrap_or(egui::Color32::BLACK),
                                outline_width: p[15].parse().unwrap_or(1.5),
                                shadow: p[16].parse().unwrap_or(false),
                                shadow_colour: parse_rgba_str(p[17])
                                    .unwrap_or(egui::Color32::from_black_alpha(120)),
                                shadow_offset: egui::vec2(
                                    p[18].parse().unwrap_or(2.0),
                                    p[19].parse().unwrap_or(2.0),
                                ),
                                banner_box: p[20].parse().unwrap_or(false),
                                banner_colour: parse_rgba_str(p[21])
                                    .unwrap_or(egui::Color32::from_black_alpha(160)),
                                banner_padding: egui::vec2(
                                    p[22].parse().unwrap_or(14.0),
                                    p[23].parse().unwrap_or(6.0),
                                ),
                                banner_rounding: p[24].parse().unwrap_or(4.0),
                                arch_bend: p[25].parse().unwrap_or(0.0),
                                clip_to_window: p[26].parse().unwrap_or(false),
                                visible: p[27].parse().unwrap_or(true),
                                locked: false,
                            });
                        }
                    }
                    "shape2" => {
                        let p: Vec<&str> = v.split('|').collect();
                        if p.len() >= 19 {
                            let name = p[0].to_string();
                            let x_offset = p[1].parse().unwrap_or(0.0);
                            let y_offset = p[2].parse().unwrap_or(0.0);
                            let rotation_deg = p[3].parse().unwrap_or(0.0);
                            let colour = parse_rgba_str(p[4]).unwrap_or(egui::Color32::WHITE);
                            let filled = p[5].parse().unwrap_or(true);
                            let outlined = p[6].parse().unwrap_or(false);
                            let stroke_width = p[7].parse().unwrap_or(3.0);
                            let stroke_colour = parse_rgba_str(p[8]).unwrap_or(colour);
                            let enable_outer_align = p[9].parse().unwrap_or(false);
                            let align_offset = p[10].parse().unwrap_or(18.0);
                            let visible = p[11].parse().unwrap_or(true);
                            let locked = p[12].parse().unwrap_or(false);
                            let clip_to_window = p[13].parse().unwrap_or(true);
                            let repeat_count = p[14].parse().unwrap_or(1);
                            let repeat_step_x = p[15].parse().unwrap_or(25.0);
                            let repeat_step_y = p[16].parse().unwrap_or(0.0);

                            let st = match p[17] {
                                "Ellipse" if p.len() >= 21 => Some(ShapeType::Ellipse {
                                    radius_x: p[18].parse().unwrap_or(40.0),
                                    radius_y: p[19].parse().unwrap_or(40.0),
                                    lock_aspect: p[20].parse().unwrap_or(true),
                                }),
                                "HalfEllipse" if p.len() >= 21 => Some(ShapeType::HalfEllipse {
                                    radius_x: p[18].parse().unwrap_or(45.0),
                                    radius_y: p[19].parse().unwrap_or(45.0),
                                    lock_aspect: p[20].parse().unwrap_or(true),
                                }),
                                "HLine" => Some(ShapeType::HorizontalLine {
                                    thickness: p[18].parse().unwrap_or(4.0),
                                }),
                                "VLine" => Some(ShapeType::VerticalLine {
                                    thickness: p[18].parse().unwrap_or(4.0),
                                }),
                                "Rect" if p.len() >= 26 => Some(ShapeType::Rectangle {
                                    width: p[18].parse().unwrap_or(80.0),
                                    height: p[19].parse().unwrap_or(80.0),
                                    curve_func: CurveFunction::from_id(p[20]),
                                    link_curves: p[21].parse().unwrap_or(true),
                                    curve_top: p[22].parse().unwrap_or(0.0),
                                    curve_right: p[23].parse().unwrap_or(0.0),
                                    curve_bottom: p[24].parse().unwrap_or(0.0),
                                    curve_left: p[25].parse().unwrap_or(0.0),
                                }),
                                "Tri" if p.len() >= 25 => Some(ShapeType::Triangle {
                                    side_a: p[18].parse().unwrap_or(90.0),
                                    side_b: p[19].parse().unwrap_or(90.0),
                                    side_c: p[20].parse().unwrap_or(90.0),
                                    curve_func: CurveFunction::from_id(p[21]),
                                    curve_a: p[22].parse().unwrap_or(0.0),
                                    curve_b: p[23].parse().unwrap_or(0.0),
                                    curve_c: p[24].parse().unwrap_or(0.0),
                                }),
                                "Poly" if p.len() >= 24 => Some(ShapeType::PolygonStar {
                                    sides: p[18].parse().unwrap_or(6),
                                    outer_radius: p[19].parse().unwrap_or(55.0),
                                    inner_radius: p[20].parse().unwrap_or(28.0),
                                    use_star: p[21].parse().unwrap_or(false),
                                    curve_func: CurveFunction::from_id(p[22]),
                                    edge_curve: p[23].parse().unwrap_or(0.0),
                                }),
                                "Svg" if p.len() >= 23 => Some(ShapeType::ImportedSvg {
                                    source_path: p[18].to_string(),
                                    scale_x: p[19].parse().unwrap_or(1.0),
                                    scale_y: p[20].parse().unwrap_or(1.0),
                                    lock_aspect: p[21].parse().unwrap_or(true),
                                    contours: deserialize_contours(p[22]),
                                }),
                                _ => None,
                            };

                            if let Some(shape_type) = st {
                                cover.shapes.push(CoverShape {
                                    name,
                                    shape_type,
                                    x_offset,
                                    y_offset,
                                    rotation_deg,
                                    colour,
                                    filled,
                                    outlined,
                                    stroke_width,
                                    stroke_colour,
                                    enable_outer_align,
                                    align_offset,
                                    visible,
                                    locked,
                                    clip_to_window,
                                    repeat_count,
                                    repeat_step_x,
                                    repeat_step_y,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if !loaded_text_blocks.is_empty() {
            cover.text_blocks = loaded_text_blocks;
        }
        Ok(cover)
    }
}