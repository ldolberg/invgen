//! invgen: markdown invoices to beautiful PDFs.

pub mod config;
pub mod gui;
pub mod library;
pub mod model;
pub mod parse;
pub mod render;

pub use model::Invoice;
