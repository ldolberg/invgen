//! Small form widgets shared by the welcome form and the Clients window:
//! labelled rows in a two-column grid with inline validation messages.

use super::theme::{self, Palette};
use eframe::egui::{self, RichText};

/// Result of checking one field.
#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    Ok,
    /// Shown in the warning color; does not block saving.
    Warn(String),
    /// Shown in the error color; blocks saving.
    Err(String),
}

impl Check {
    pub fn is_err(&self) -> bool {
        matches!(self, Check::Err(_))
    }
}

/// An optional number field.
pub fn number_check(s: &str) -> Check {
    match crate::setup::number(s) {
        Ok(_) => Check::Ok,
        Err(e) => Check::Err(e),
    }
}

const LABEL_W: f32 = 118.0;

pub fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.add_space(10.0);
    ui.label(
        RichText::new(title)
            .font(theme::strong(11.0))
            .color(p.text_weak),
    );
    ui.add_space(2.0);
}

pub fn grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 6.0])
        .min_col_width(LABEL_W)
        .show(ui, add);
}

fn label(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).color(p.text));
}

fn message(ui: &mut egui::Ui, p: &Palette, check: &Check) {
    let (text, color) = match check {
        Check::Ok => return,
        Check::Warn(m) => (m, p.warn),
        Check::Err(m) => (m, p.error),
    };
    ui.label("");
    ui.label(RichText::new(text).size(11.5).color(color));
    ui.end_row();
}

/// A single-line field with a placeholder and its inline message.
pub fn text(
    ui: &mut egui::Ui,
    p: &Palette,
    name: &str,
    value: &mut String,
    hint: &str,
    check: &Check,
) -> egui::Response {
    label(ui, p, name);
    let mut edit = egui::TextEdit::singleline(value)
        .hint_text(hint)
        .desired_width(f32::INFINITY);
    if check.is_err() {
        edit = edit.text_color(p.error);
    }
    let resp = ui.add(edit);
    ui.end_row();
    message(ui, p, check);
    resp
}

/// A multi-line field where each line becomes one list entry.
pub fn lines(ui: &mut egui::Ui, p: &Palette, name: &str, value: &mut String, hint: &str) {
    label(ui, p, name);
    ui.add(
        egui::TextEdit::multiline(value)
            .hint_text(hint)
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    ui.end_row();
}

/// A weak hint under the previous row.
pub fn hint_row(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label("");
    ui.label(RichText::new(text).size(11.0).color(p.text_weak));
    ui.end_row();
}
