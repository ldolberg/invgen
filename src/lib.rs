//! invgen: markdown invoices to beautiful PDFs.

pub mod complete;
pub mod config;
pub mod config_edit;
pub mod credit;
pub mod duplicate;
pub mod einvoice;
pub mod frontmatter;
pub mod gui;
pub mod library;
pub mod lint;
pub mod lock;
pub mod lookup;
pub mod model;
pub mod parse;
pub mod render;
pub mod report;
pub mod schema;
pub mod send;
pub mod setup;

pub use model::Invoice;
