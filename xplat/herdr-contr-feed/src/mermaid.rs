//! `mermaid`: a code block with the language mermaid, drawn to an SVG file for the chat (#87). The renderer is
//! mermaid-rs-renderer (pure Rust, no browser and no node). The file name is a hash of the text and the colors,
//! so the same block in the same theme is drawn once; the mod draws the file with `image(...)`.

use mermaid_rs_renderer::{compute_layout, measure_svg_dimensions, parse_mermaid_strict, render_svg, LayoutConfig, Theme};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The theme colors the mod gives (theme() of Herdr). An empty one keeps the renderer's own.
#[derive(Debug, Default, Clone)]
pub struct Colors {
    /// A dark theme: the renderer's dark palette is the base, else its light one.
    pub dark: bool,
    /// Text: node labels, messages, notes.
    pub fg: String,
    /// The box behind the diagram.
    pub bg: String,
    /// Node and actor fills.
    pub fill: String,
    /// Node and actor borders.
    pub border: String,
    /// Edges, arrows and lifelines.
    pub line: String,
}

/// A drawn diagram: the file, and its size in px.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub path: PathBuf,
    pub width: f32,
    pub height: f32,
}

/// Only a color as a theme gives it (`#rgb`, `#rrggbb`, `#rrggbbaa`) goes into the SVG.
fn color(s: &str) -> Option<String> {
    let hex = s.strip_prefix('#')?;
    (matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| s.to_string())
}

fn theme(c: &Colors) -> Theme {
    let mut t = if c.dark { Theme::dark() } else { Theme::modern() };
    if let Some(fg) = color(&c.fg) {
        for f in [&mut t.primary_text_color, &mut t.text_color, &mut t.pie_title_text_color, &mut t.pie_legend_text_color] {
            *f = fg.clone();
        }
    }
    if let Some(bg) = color(&c.bg) {
        t.background = bg.clone();
        t.edge_label_background = bg;
    }
    if let Some(fill) = color(&c.fill) {
        for f in [&mut t.primary_color, &mut t.sequence_actor_fill, &mut t.secondary_color, &mut t.sequence_note_fill] {
            *f = fill.clone();
        }
    }
    if let Some(b) = color(&c.border) {
        for f in [&mut t.primary_border_color, &mut t.sequence_actor_border, &mut t.sequence_note_border, &mut t.cluster_border] {
            *f = b.clone();
        }
    }
    if let Some(l) = color(&c.line) {
        for f in [&mut t.line_color, &mut t.sequence_actor_line, &mut t.sequence_activation_border] {
            *f = l.clone();
        }
    }
    t
}

/// The file name: a hash of the renderer's version, the colors and the text.
pub fn file_name(text: &str, c: &Colors) -> String {
    let mut h = Sha256::new();
    for part in ["mmdr-0.3.1", if c.dark { "dark" } else { "light" }, &c.fg, &c.bg, &c.fill, &c.border, &c.line, text] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    let hex: String = h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect();
    format!("{hex}.svg")
}

/// The width and height of an SVG this module wrote (its root's `width` and `height`).
fn size_of(svg: &str) -> Option<(f32, f32)> {
    let root = &svg[svg.find("<svg")?..];
    let root = &root[..root.find('>')?];
    let attr = |name: &str| -> Option<f32> {
        let at = root.find(&format!(" {name}=\""))? + name.len() + 3;
        root[at..].split('"').next()?.trim_end_matches("px").parse().ok()
    };
    Some((attr("width")?, attr("height")?))
}

/// Draws `text` into `dir` (made when missing), or reads the file drawn before. Err: the text is not a diagram
/// the renderer reads, or the file cannot be written.
pub fn draw(text: &str, c: &Colors, dir: &Path) -> Result<Drawn, (&'static str, String)> {
    let path = dir.join(file_name(text, c));
    if let Some((width, height)) = std::fs::read_to_string(&path).ok().as_deref().and_then(size_of) {
        return Ok(Drawn { path, width, height });
    }
    let parsed = parse_mermaid_strict(text).map_err(|e| ("parse", e.to_string()))?;
    let t = theme(c);
    let cfg = LayoutConfig::default();
    let layout = compute_layout(&parsed.graph, &t, &cfg);
    let svg = render_svg(&layout, &t, &cfg);
    let dims = measure_svg_dimensions(&layout, &cfg, None);
    let (width, height) = size_of(&svg).unwrap_or((dims.width, dims.height));
    if !(width > 0.0 && height > 0.0) {
        return Err(("parse", "the diagram is empty".into()));
    }
    std::fs::create_dir_all(dir).map_err(|e| ("write", e.to_string()))?;
    // Written beside it and renamed, so a mod that reads at the same time never sees half a file.
    let tmp = path.with_extension(format!("svg.{}", std::process::id()));
    std::fs::write(&tmp, svg).and_then(|()| std::fs::rename(&tmp, &path)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ("write", e.to_string())
    })?;
    Ok(Drawn { path, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: &str = "flowchart TD\n  A[Issue] --> B{CI green?}\n  B -->|yes| C[Merge]\n  B -->|no| D[Fix round]\n  D --> B\n";
    const SEQ: &str = "sequenceDiagram\n  participant M as Mod\n  participant H as Helper\n  M->>H: mermaid --dark\n  H-->>M: path, width, height\n  Note over M: image(path)\n";

    fn dark() -> Colors {
        Colors { dark: true, fg: "#e6e6e6".into(), bg: "#26272b".into(), fill: "#1c1d20".into(), border: "#3a3b40".into(), line: "#9a9ba0".into() }
    }
    fn light() -> Colors {
        Colors { dark: false, fg: "#1d1d1f".into(), bg: "#f2f2f4".into(), fill: "#ffffff".into(), border: "#d4d4d8".into(), line: "#6b6b70".into() }
    }

    // The diagrams go to tmp/mermaid in the worktree as well, to look at them.
    fn keep(d: &Drawn, name: &str) {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/mermaid");
        if std::fs::create_dir_all(&out).is_ok() {
            let _ = std::fs::copy(&d.path, out.join(name));
        }
    }

    #[test]
    fn draws_a_flowchart_and_a_sequence_diagram_in_both_themes() {
        let dir = tempfile::tempdir().unwrap();
        for (text, kind) in [(FLOW, "flowchart"), (SEQ, "sequence")] {
            for (c, mode) in [(dark(), "dark"), (light(), "light")] {
                let d = draw(text, &c, dir.path()).unwrap();
                assert!(d.width > 50.0 && d.height > 50.0, "{kind} {mode}: {d:?}");
                let svg = std::fs::read_to_string(&d.path).unwrap();
                assert!(svg.starts_with("<svg") || svg.contains("<svg"), "{kind} {mode}");
                assert!(svg.contains(&c.fg), "{kind} {mode}: the theme's text color");
                keep(&d, &format!("{kind}-{mode}.svg"));
            }
        }
        assert!(std::fs::read_to_string(dir.path().join(file_name(FLOW, &dark()))).unwrap().contains("CI green?"));
        assert!(std::fs::read_to_string(dir.path().join(file_name(SEQ, &dark()))).unwrap().contains("Helper"));
    }

    #[test]
    fn the_same_text_and_colors_read_the_file_drawn_before() {
        let dir = tempfile::tempdir().unwrap();
        let a = draw(FLOW, &dark(), dir.path()).unwrap();
        let b = draw(FLOW, &dark(), dir.path()).unwrap();
        assert_eq!(a, b);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        // Another theme or another text: another file.
        assert_ne!(draw(FLOW, &light(), dir.path()).unwrap().path, a.path);
        assert_ne!(file_name("flowchart LR\n  A --> B\n", &dark()), file_name(FLOW, &dark()));
    }

    #[test]
    fn text_that_is_not_a_diagram_is_an_error_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let e = draw("this is not mermaid ((", &dark(), dir.path().join("m").as_path()).unwrap_err();
        assert_eq!(e.0, "parse");
        assert!(!dir.path().join("m").exists());
    }

    #[test]
    fn only_hex_colors_go_into_the_svg() {
        let mut c = dark();
        c.fg = "red\" onload=\"x".into();
        let t = theme(&c);
        assert_eq!(t.text_color, Theme::dark().text_color);
        assert_eq!(color("#abc"), Some("#abc".into()));
        assert_eq!(color("#12345"), None);
    }

    #[test]
    fn reads_the_size_of_the_root() {
        assert_eq!(size_of(r#"<svg xmlns="x" width="120.5" height="80" viewBox="0 0 1 1">"#), Some((120.5, 80.0)));
        assert_eq!(size_of("<p>"), None);
    }
}
