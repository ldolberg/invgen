//! invgen: markdown invoices to beautiful PDFs.

pub mod config;
pub mod config_edit;
pub mod duplicate;
pub mod frontmatter;
pub mod gui;
pub mod library;
pub mod lock;
pub mod lookup;
pub mod model;
pub mod parse;
pub mod render;
pub mod report;
pub mod send;
pub mod setup;

pub use model::Invoice;
