//! A minimal, hermetic typst `World`: one main source, virtual data files
//! (the invoice JSON, the logo), embedded fonts, no package access and a
//! fixed `today`. A custom template may additionally read files from its own
//! directory (imports, images); nothing else on disk is reachable.

use std::path::PathBuf;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::FontStore;

/// Everything that stays constant between compiles.
pub(super) struct Shared {
    pub library: LazyHash<Library>,
    pub fonts: FontStore,
    /// The built-in template.
    pub builtin: Source,
}

/// Per-compile view: the shared state plus this invoice's data and template.
pub(super) struct InvoiceWorld<'a> {
    pub shared: &'a Shared,
    /// The template being compiled (built-in or custom).
    pub source: Source,
    /// Virtual files: the invoice JSON and the logo.
    pub files: Vec<(FileId, Bytes)>,
    /// Directory of a custom template; its files are readable.
    pub root: Option<PathBuf>,
    pub today: Option<Datetime>,
}

impl InvoiceWorld<'_> {
    fn virtual_file(&self, id: FileId) -> Option<&Bytes> {
        self.files.iter().find(|(f, _)| *f == id).map(|(_, b)| b)
    }

    /// The real path of a file under the custom template's directory.
    pub fn disk_path(&self, id: FileId) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let path = id.get();
        if !matches!(path.root(), VirtualRoot::Project) {
            return None;
        }
        path.vpath().realize(root).ok()
    }

    fn read_disk(&self, id: FileId) -> FileResult<Vec<u8>> {
        let path = self.disk_path(id).ok_or(FileError::AccessDenied)?;
        std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))
    }
}

impl World for InvoiceWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.shared.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.shared.fonts.book()
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else if self.virtual_file(id).is_some() {
            Err(FileError::NotSource)
        } else {
            let bytes = self.read_disk(id)?;
            let text = String::from_utf8(bytes).map_err(|_| FileError::InvalidUtf8)?;
            Ok(Source::new(id, text))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if let Some(b) = self.virtual_file(id) {
            Ok(b.clone())
        } else if id == self.source.id() {
            Ok(Bytes::from_string(self.source.text().to_string()))
        } else {
            self.read_disk(id).map(Bytes::new)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.shared.fonts.font(index)
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        self.today
    }
}
