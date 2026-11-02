//! invgen: markdown invoices to beautiful PDFs.

pub mod config;
pub mod config_edit;
pub mod gui;
pub mod library;
pub mod model;
pub mod parse;
pub mod render;
pub mod setup;

pub use model::Invoice;
