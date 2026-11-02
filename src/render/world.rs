//! A minimal, hermetic typst `World`: one main source, one virtual JSON data
//! file, embedded fonts, no package access and a fixed `today`.

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::FontStore;

/// Everything that stays constant between compiles.
pub(super) struct Shared {
    pub library: LazyHash<Library>,
    pub fonts: FontStore,
    pub main: FileId,
    pub data: FileId,
    pub source: Source,
}

/// Per-compile view: the shared state plus this invoice's data.
pub(super) struct InvoiceWorld<'a> {
    pub shared: &'a Shared,
    pub data: Bytes,
    pub today: Option<Datetime>,
}

impl World for InvoiceWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.shared.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.shared.fonts.book()
    }

    fn main(&self) -> FileId {
        self.shared.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.shared.main {
            Ok(self.shared.source.clone())
        } else if id == self.shared.data {
            Err(FileError::NotSource)
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.shared.data {
            Ok(self.data.clone())
        } else if id == self.shared.main {
            Ok(Bytes::from_string(self.shared.source.text().to_string()))
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.shared.fonts.font(index)
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        self.today
    }
}
