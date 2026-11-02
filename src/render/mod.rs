//! Invoice -> Typst -> PDF / preview images.
//!
//! The typst template lives in `assets/invoice.typ` and is compiled with a
//! hermetic in-process `World` (see `world.rs`). Invoice data never touches
//! typst markup: it is serialized to JSON, exposed as the virtual file
//! `/invoice.json`, and read by the template with `json(...)`, so user text is
//! always treated as plain text.
//!
//! A custom template (`template:` in the config or front matter) replaces
//! the built-in one and receives the same data; see docs/TEMPLATES.md.

mod data;
pub mod i18n;
mod world;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, anyhow};
use chrono::Datelike;
use typst::diag::{Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::Font;
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

use crate::model::Invoice;
use world::{InvoiceWorld, Shared};

pub use data::CONTRACT_VERSION;

/// The built-in invoice template (typst markup). Reads all data from
/// `/invoice.json`. `invgen template --export` writes it out.
pub const TEMPLATE: &str = include_str!("../../assets/invoice.typ");

/// Fonts bundled into the binary. Order matters only for ties.
const FONTS: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/Inter-Regular.otf"),
    include_bytes!("../../assets/fonts/Inter-Italic.otf"),
    include_bytes!("../../assets/fonts/Inter-Medium.otf"),
    include_bytes!("../../assets/fonts/Inter-SemiBold.otf"),
    include_bytes!("../../assets/fonts/Inter-Bold.otf"),
    include_bytes!("../../assets/fonts/MrsSaintDelafield-Regular.ttf"),
];

/// A rendered page as straight (non-premultiplied) RGBA8.
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Holds loaded fonts and the typst template. Cheap to reuse; create once.
/// Must be Send + Sync so the GUI can render on a worker thread.
pub struct Renderer {
    shared: Shared,
    /// Serializes compiles: comemo's global cache is shared anyway, and this
    /// keeps eviction bookkeeping simple and memory bounded.
    lock: Mutex<()>,
}

/// A compiled document.
pub struct Compiled {
    doc: PagedDocument,
    number: String,
}

fn virtual_file(path: &str) -> FileId {
    let vpath = VirtualPath::new(path).expect("static virtual path is valid");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

impl Renderer {
    pub fn new() -> anyhow::Result<Renderer> {
        let mut fonts = FontStore::new();
        for data in FONTS {
            let mut any = false;
            for font in Font::iter(Bytes::new(*data)) {
                let info = font.info().clone();
                fonts.push((font, info));
                any = true;
            }
            if !any {
                return Err(anyhow!("failed to load a bundled font"));
            }
        }
        // typst's own fonts as a fallback for glyph coverage.
        fonts.extend(typst_kit::fonts::embedded());

        let shared = Shared {
            library: LazyHash::new(Library::builder().build()),
            fonts,
            builtin: Source::new(virtual_file("/main.typ"), TEMPLATE.to_string()),
        };
        Ok(Renderer {
            shared,
            lock: Mutex::new(()),
        })
    }

    pub fn compile(&self, inv: &Invoice) -> anyhow::Result<Compiled> {
        let mut files = Vec::new();
        let logo = match &inv.from.logo {
            Some(path) => {
                let (vpath, bytes) = load_logo(path)?;
                files.push((virtual_file(&vpath), bytes));
                Some(vpath)
            }
            None => None,
        };
        let json =
            serde_json::to_vec(&data::View::new(inv, logo)).context("serializing invoice")?;
        files.push((virtual_file("/invoice.json"), Bytes::new(json)));

        let (source, root) = match &inv.template {
            None => (self.shared.builtin.clone(), None),
            Some(path) => load_template(path)?,
        };
        let d = inv.date;
        let world = InvoiceWorld {
            shared: &self.shared,
            source,
            files,
            root,
            today: Datetime::from_ymd(d.year(), d.month() as u8, d.day() as u8),
        };

        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let result = typst::compile::<PagedDocument>(&world);
        // Keep the memoization cache warm for the next (similar) compile but
        // drop entries unused for a while so memory stays bounded.
        typst::comemo::evict(10);

        let doc = result
            .output
            .map_err(|errs| anyhow!(format_diags(&world, inv.template.as_deref(), &errs)))?;
        Ok(Compiled {
            doc,
            number: inv.number.clone(),
        })
    }
}

/// Image formats typst can embed, by file extension.
const LOGO_FORMATS: &[&str] = &["svg", "png", "jpg", "jpeg", "gif", "webp"];

/// Read the logo file and pick the virtual path it is served at.
fn load_logo(path: &Path) -> anyhow::Result<(String, Bytes)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| LOGO_FORMATS.contains(&e.as_str()))
        .ok_or_else(|| {
            anyhow!(
                "logo {}: unsupported image type (use {})",
                path.display(),
                LOGO_FORMATS.join(", ")
            )
        })?;
    let bytes =
        std::fs::read(path).with_context(|| format!("cannot read logo {}", path.display()))?;
    if !looks_like(&ext, &bytes) {
        return Err(anyhow!(
            "logo {} is not a valid {} file",
            path.display(),
            ext.to_ascii_uppercase()
        ));
    }
    Ok((format!("/logo.{ext}"), Bytes::new(bytes)))
}

/// Cheap signature check so a broken logo fails with the logo's path rather
/// than a decoding error inside the template.
fn looks_like(ext: &str, b: &[u8]) -> bool {
    match ext {
        "png" => b.starts_with(b"\x89PNG"),
        "jpg" | "jpeg" => b.starts_with(&[0xFF, 0xD8]),
        "gif" => b.starts_with(b"GIF8"),
        "webp" => b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP",
        "svg" => {
            let head = &b[..b.len().min(4096)];
            String::from_utf8_lossy(head).contains("<svg")
        }
        _ => false,
    }
}

/// Load a custom template. Its directory becomes the typst project root so
/// it can import its own files and images.
fn load_template(path: &Path) -> anyhow::Result<(Source, Option<PathBuf>)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read template {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("template {}: invalid file name", path.display()))?;
    let vpath = VirtualPath::new(format!("/{name}"))
        .map_err(|e| anyhow!("template {}: {e}", path.display()))?;
    let id = RootedPath::new(VirtualRoot::Project, vpath).intern();
    let root = path.parent().map(Path::to_path_buf);
    Ok((Source::new(id, text), root))
}

fn format_diags(
    world: &InvoiceWorld,
    template: Option<&Path>,
    diags: &[SourceDiagnostic],
) -> String {
    let mut out = match template {
        None => String::from("invoice template failed to compile:"),
        Some(p) => format!("template {} failed to compile:", p.display()),
    };
    for d in diags.iter().filter(|d| d.severity == Severity::Error) {
        let loc = d.span.id().and_then(|id| {
            let src = world.source(id).ok()?;
            let line = world
                .range(d.span)
                .and_then(|r| src.lines().byte_to_line(r.start))?
                + 1;
            Some(match world.disk_path(id) {
                Some(path) => format!("{}:{line}: ", path.display()),
                None => format!("template line {line}: "),
            })
        });
        out.push_str(&format!("\n  {}{}", loc.unwrap_or_default(), d.message));
        for h in &d.hints {
            out.push_str(&format!("\n    hint: {}", h.v));
        }
    }
    out
}

impl Compiled {
    pub fn page_count(&self) -> usize {
        self.doc.pages().len()
    }

    pub fn pdf(&self) -> anyhow::Result<Vec<u8>> {
        let options = typst_pdf::PdfOptions {
            ident: typst::foundations::Smart::Custom(self.number.clone()),
            ..Default::default()
        };
        typst_pdf::pdf(&self.doc, &options).map_err(|errs| {
            let msgs: Vec<String> = errs.iter().map(|e| e.message.to_string()).collect();
            anyhow!("PDF export failed: {}", msgs.join("; "))
        })
    }

    /// Render pages at `pixels_per_pt` (2.0 is a good preview density).
    pub fn render_pages(&self, pixels_per_pt: f32) -> Vec<PageImage> {
        let opts = typst_render::RenderOptions {
            pixel_per_pt: typst::utils::Scalar::new(pixels_per_pt.max(0.05) as f64),
            render_bleed: false,
        };
        self.doc
            .pages()
            .iter()
            .map(|page| {
                let pixmap = typst_render::render(page, &opts);
                let mut rgba = Vec::with_capacity(pixmap.data().len());
                for px in pixmap.pixels() {
                    let c = px.demultiply();
                    rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
                }
                PageImage {
                    width: pixmap.width(),
                    height: pixmap.height(),
                    rgba,
                }
            })
            .collect()
    }
}

#[allow(dead_code)]
fn _assert_thread_safety() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Renderer>();
    send_sync::<Compiled>();
    send_sync::<PageImage>();
}

#[cfg(test)]
mod tests;
