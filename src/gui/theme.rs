//! Visual identity: palette, fonts and egui style tweaks.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, Visuals,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Named family for the semibold UI face (falls back to the regular face).
pub const STRONG: &str = "ui-strong";

/// Colors that are not part of egui's `Visuals` (editor syntax, preview canvas...).
#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Color32,
    pub accent_soft: Color32,
    pub bg: Color32,
    pub surface: Color32,
    pub editor_bg: Color32,
    pub canvas: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_strong: Color32,
    pub text_weak: Color32,
    pub gutter: Color32,
    pub syn_key: Color32,
    pub syn_string: Color32,
    pub syn_number: Color32,
    pub syn_comment: Color32,
    pub syn_punct: Color32,
    pub syn_heading: Color32,
    pub error: Color32,
    pub error_bg: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub page_shadow: Color32,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            accent: Color32::from_rgb(0x7c, 0x93, 0xff),
            accent_soft: Color32::from_rgba_unmultiplied(0x7c, 0x93, 0xff, 38),
            bg: Color32::from_rgb(0x15, 0x16, 0x19),
            surface: Color32::from_rgb(0x1c, 0x1d, 0x21),
            editor_bg: Color32::from_rgb(0x18, 0x19, 0x1c),
            canvas: Color32::from_rgb(0x0f, 0x10, 0x12),
            border: Color32::from_rgb(0x2a, 0x2c, 0x31),
            text: Color32::from_rgb(0xc9, 0xcd, 0xd4),
            text_strong: Color32::from_rgb(0xf0, 0xf2, 0xf5),
            text_weak: Color32::from_rgb(0x7d, 0x83, 0x8d),
            gutter: Color32::from_rgb(0x4a, 0x4f, 0x58),
            syn_key: Color32::from_rgb(0x8f, 0xa4, 0xff),
            syn_string: Color32::from_rgb(0xa3, 0xd0, 0x8c),
            syn_number: Color32::from_rgb(0xf0, 0xae, 0x6e),
            syn_comment: Color32::from_rgb(0x60, 0x66, 0x70),
            syn_punct: Color32::from_rgb(0x5a, 0x60, 0x6b),
            syn_heading: Color32::from_rgb(0xf2, 0xf4, 0xf8),
            error: Color32::from_rgb(0xff, 0x6b, 0x6b),
            error_bg: Color32::from_rgba_unmultiplied(0xff, 0x55, 0x55, 34),
            ok: Color32::from_rgb(0x5f, 0xd3, 0x8d),
            warn: Color32::from_rgb(0xf2, 0xc0, 0x5c),
            page_shadow: Color32::from_black_alpha(150),
        }
    } else {
        Palette {
            accent: Color32::from_rgb(0x4a, 0x63, 0xe6),
            accent_soft: Color32::from_rgba_unmultiplied(0x4a, 0x63, 0xe6, 30),
            bg: Color32::from_rgb(0xf4, 0xf4, 0xf6),
            surface: Color32::from_rgb(0xff, 0xff, 0xff),
            editor_bg: Color32::from_rgb(0xfc, 0xfc, 0xfd),
            canvas: Color32::from_rgb(0xe6, 0xe7, 0xeb),
            border: Color32::from_rgb(0xdf, 0xe1, 0xe6),
            text: Color32::from_rgb(0x2f, 0x33, 0x3a),
            text_strong: Color32::from_rgb(0x11, 0x13, 0x17),
            text_weak: Color32::from_rgb(0x80, 0x86, 0x90),
            gutter: Color32::from_rgb(0xb0, 0xb5, 0xbd),
            syn_key: Color32::from_rgb(0x3a, 0x52, 0xd4),
            syn_string: Color32::from_rgb(0x2f, 0x7d, 0x32),
            syn_number: Color32::from_rgb(0xb4, 0x5a, 0x12),
            syn_comment: Color32::from_rgb(0x9a, 0xa0, 0xa8),
            syn_punct: Color32::from_rgb(0xa5, 0xab, 0xb4),
            syn_heading: Color32::from_rgb(0x0b, 0x0c, 0x0f),
            error: Color32::from_rgb(0xd6, 0x33, 0x33),
            error_bg: Color32::from_rgba_unmultiplied(0xe5, 0x3e, 0x3e, 26),
            ok: Color32::from_rgb(0x1f, 0x9d, 0x55),
            warn: Color32::from_rgb(0xc2, 0x83, 0x0c),
            page_shadow: Color32::from_black_alpha(60),
        }
    }
}

pub fn current(ctx: &egui::Context) -> Palette {
    palette(ctx.global_style().visuals.dark_mode)
}

fn visuals(dark: bool) -> Visuals {
    let p = palette(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let r = CornerRadius::same(6);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 40 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 30 }),
    };
    v.extreme_bg_color = p.editor_bg;
    v.text_edit_bg_color = Some(p.editor_bg);
    v.faint_bg_color = p.surface;
    v.code_bg_color = p.surface;
    v.hyperlink_color = p.accent;
    v.error_fg_color = p.error;
    v.warn_fg_color = p.warn;
    v.selection.bg_fill = if dark {
        Color32::from_rgba_unmultiplied(0x7c, 0x93, 0xff, 70)
    } else {
        Color32::from_rgba_unmultiplied(0x4a, 0x63, 0xe6, 55)
    };
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg;
    w.noninteractive.weak_bg_fill = p.bg;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    let (inactive, hovered, active) = if dark {
        (
            Color32::from_rgb(0x24, 0x26, 0x2b),
            Color32::from_rgb(0x2d, 0x30, 0x36),
            Color32::from_rgb(0x35, 0x38, 0x40),
        )
    } else {
        (
            Color32::from_rgb(0xec, 0xed, 0xf0),
            Color32::from_rgb(0xe2, 0xe4, 0xe9),
            Color32::from_rgb(0xd7, 0xda, 0xe0),
        )
    };
    w.inactive.bg_fill = inactive;
    w.inactive.weak_bg_fill = inactive;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = hovered;
    w.hovered.weak_bg_fill = hovered;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text_strong);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;
    w.active.bg_fill = active;
    w.active.weak_bg_fill = active;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text_strong);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;
    w.open = w.active;
    v
}

/// Install visuals for both themes plus spacing and text sizes.
pub fn install_style(ctx: &egui::Context) {
    ctx.set_visuals_of(Theme::Dark, visuals(true));
    ctx.set_visuals_of(Theme::Light, visuals(false));
    ctx.all_styles_mut(|style| {
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 6.0);
        s.button_padding = egui::vec2(10.0, 5.0);
        s.interact_size.y = 28.0;
        s.window_margin = Margin::same(14);
        s.menu_margin = Margin::same(6);
        s.scroll.bar_width = 8.0;
        s.scroll.floating = true;
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(17.0, FontFamily::Name(STRONG.into())),
            ),
            (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.5, FontFamily::Monospace),
            ),
        ]
        .into();
    });
    ctx.options_mut(|o| {
        // Ctrl+= / Ctrl+- drive the PDF zoom, not the UI scale.
        o.zoom_with_keyboard = false;
    });
}

/// Locate a system font: `fc-match` first (must actually match the family),
/// then a list of well known paths.
fn find_font(family: &str, style: &str, paths: &[&str]) -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    for p in paths {
        let p = if let Some(rest) = p.strip_prefix("~/") {
            home.join(rest)
        } else {
            PathBuf::from(p)
        };
        if p.is_file() {
            return Some(p);
        }
    }
    let out = std::process::Command::new("fc-match")
        .arg("-f")
        .arg("%{family}\n%{file}")
        .arg(format!("{family}:style={style}"))
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut lines = s.lines();
    let fam = lines.next()?.to_lowercase();
    let file = lines.next()?;
    let want = family.to_lowercase().replace(' ', "");
    if fam.replace(' ', "").contains(&want) && Path::new(file).is_file() {
        Some(PathBuf::from(file))
    } else {
        None
    }
}

fn load(path: Option<PathBuf>) -> Option<FontData> {
    let bytes = std::fs::read(path?).ok()?;
    Some(FontData::from_owned(bytes))
}

/// Use the bundled Inter for the UI and JetBrains Mono / Fira Mono (editor) from the system,
/// keeping egui's bundled fonts as fallbacks for missing glyphs.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();

    // Inter ships inside the binary (assets/fonts, also used by the PDF renderer),
    // so the UI looks the same everywhere, including inside a Flatpak sandbox.
    let ui_regular = Some(FontData::from_static(include_bytes!(
        "../../assets/fonts/Inter-Regular.otf"
    )));
    let ui_strong = Some(FontData::from_static(include_bytes!(
        "../../assets/fonts/Inter-SemiBold.otf"
    )));
    let mono = load(
        find_font(
            "JetBrains Mono",
            "Regular",
            &[
                "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
                "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
                "~/.local/share/fonts/JetBrainsMono-Regular.ttf",
                "~/.local/share/fonts/JetBrains Mono Regular Nerd Font Complete Mono.ttf",
            ],
        )
        .or_else(|| {
            find_font(
                "Fira Mono",
                "Regular",
                &[
                    "/usr/share/fonts/opentype/fira/FiraMono-Regular.otf",
                    "/usr/share/fonts/truetype/fira/FiraMono-Regular.ttf",
                    "~/.local/share/fonts/Fira Mono Regular Nerd Font Complete Mono.otf",
                ],
            )
        }),
    );

    if let Some(f) = ui_regular {
        defs.font_data.insert("inter".into(), Arc::new(f));
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "inter".into());
    }
    let mut strong = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(f) = ui_strong {
        defs.font_data.insert("inter-semibold".into(), Arc::new(f));
        strong.insert(0, "inter-semibold".into());
    }
    defs.families
        .insert(FontFamily::Name(STRONG.into()), strong);
    if let Some(f) = mono {
        defs.font_data.insert("mono".into(), Arc::new(f));
        defs.families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "mono".into());
    }
    ctx.set_fonts(defs);
}

pub fn strong(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(STRONG.into()))
}
