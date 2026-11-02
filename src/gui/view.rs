//! Drawing: header, sidebar, editor, preview, status bar, toasts and dialogs.

use super::theme::{self, Palette};
use super::{
    App, Pending, ThemeChoice, ToastKind, clients, highlight, onboarding, open_external, tilde,
};
use crate::model::{Status, format_date, format_money};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id, Layout,
    Margin, Pos2, Rect, RichText, Sense, Shadow, Shape, Stroke, StrokeKind, UiBuilder, Vec2, pos2,
    vec2,
};
use std::time::{Duration, Instant};

const PAGE_GAP: f32 = 22.0;
const PAGE_PAD: f32 = 28.0;

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let p = theme::current(&ctx);

    egui::Panel::top("header")
        .frame(
            Frame::new()
                .fill(p.surface)
                .inner_margin(Margin::symmetric(12, 8))
                .stroke(Stroke::new(1.0, p.border)),
        )
        .show_separator_line(false)
        .show(ui, |ui| header(app, ui, &p));

    egui::Panel::bottom("status")
        .frame(
            Frame::new()
                .fill(p.surface)
                .inner_margin(Margin::symmetric(12, 4))
                .stroke(Stroke::new(1.0, p.border)),
        )
        .show_separator_line(false)
        .show(ui, |ui| status_bar(app, ui, &p));

    let mut open = app.prefs.sidebar;
    egui::Panel::left("library")
        .frame(
            Frame::new()
                .fill(p.bg)
                .inner_margin(Margin::symmetric(8, 10))
                .stroke(Stroke::new(1.0, p.border)),
        )
        .resizable(true)
        .default_size(250.0)
        .size_range(190.0..=420.0)
        .show_separator_line(false)
        .show_collapsible(ui, &mut open, |ui| sidebar(app, ui, &p));
    app.prefs.sidebar = open;

    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg))
        .show(ui, |ui| work_area(app, ui, &p));

    clients_window(app, &ctx, &p);
    toasts(app, &ctx, &p);
    confirm_modal(app, &ctx, &p);
    onboarding_modal(app, &ctx, &p);
    drop_overlay(&ctx, &p);
}

fn clients_window(app: &mut App, ctx: &egui::Context, p: &Palette) {
    if !app.clients_open {
        return;
    }
    let mut open = true;
    let cfg = app.cfg.clone();
    let path = tilde(&app.config_path);
    let action = clients::show(&mut app.clients, &mut open, ctx, p, &cfg, &path);
    app.clients_open = open;
    if let Some(a) = action {
        app.handle_clients(a);
    }
}

fn onboarding_modal(app: &mut App, ctx: &egui::Context, p: &Palette) {
    let path = tilde(&app.config_path);
    let Some(form) = app.onboarding.as_mut() else {
        return;
    };
    if let Some(outcome) = onboarding::show(form, ctx, p, &path) {
        app.handle_onboarding(outcome);
    }
}

// ------------------------------------------------------------------ header

fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(icon).size(15.0))
            .frame(false)
            .min_size(vec2(28.0, 28.0)),
    )
    .on_hover_text(tip)
}

fn header(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        ui.set_min_height(36.0);
        if icon_button(ui, "\u{2630}", "Toggle invoice list (Ctrl+B)").clicked() {
            app.prefs.sidebar = !app.prefs.sidebar;
        }
        ui.add_space(4.0);

        // Buttons claim the right side first; the rest gets what is left.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let theme_icon = match app.prefs.theme {
                ThemeChoice::Dark => "\u{1F319}",
                ThemeChoice::Light => "\u{2600}",
                ThemeChoice::System => "\u{25D0}",
            };
            let theme_tip = match app.prefs.theme {
                ThemeChoice::Dark => "Theme: dark (click for light)",
                ThemeChoice::Light => "Theme: light (click to follow system)",
                ThemeChoice::System => "Theme: follows system (click for dark)",
            };
            if icon_button(ui, theme_icon, theme_tip).clicked() {
                app.cycle_theme(&ctx);
            }
            ui.add_space(4.0);

            let export = egui::Button::new(
                RichText::new("Export PDF")
                    .font(theme::strong(13.0))
                    .color(Color32::WHITE),
            )
            .fill(p.accent)
            .corner_radius(CornerRadius::same(6));
            let resp = ui.add(export).on_hover_text(format!(
                "Ctrl+E: {}\nShift+Ctrl+E or right click: choose location",
                tilde(&app.export_path())
            ));
            if resp.clicked() {
                app.export(None);
            }
            if resp.secondary_clicked() {
                app.export_as_dialog();
            }
            if ui
                .button("Open PDF")
                .on_hover_text("Open the exported PDF in your viewer")
                .clicked()
            {
                app.open_pdf();
            }
            ui.add_space(8.0);
            if ui
                .button("Save")
                .on_hover_text("Ctrl+S (Shift+Ctrl+S: save as)")
                .clicked()
            {
                app.save(None);
            }
            if ui.button("Open").on_hover_text("Ctrl+O").clicked() {
                app.open_dialog();
            }
            ui.spacing_mut().item_spacing.x = 2.0;
            new_menu(app, ui, p);
            if ui
                .button("New")
                .on_hover_text("Ctrl+N: new invoice with the next number")
                .clicked()
            {
                app.request(Pending::New(None));
            }
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_space(6.0);
            if ui
                .button("Clients")
                .on_hover_text("Add, edit and delete clients")
                .clicked()
            {
                app.open_clients();
            }
            ui.add_space(12.0);

            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                header_info(app, ui, p);
            });
        });
    });
}

/// The arrow next to "New": pick the client for the new invoice.
fn new_menu(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let mut pick = None;
    let mut manage = false;
    ui.menu_button("\u{25BE}", |ui| {
        ui.set_min_width(220.0);
        ui.label(
            RichText::new("NEW INVOICE FOR")
                .font(theme::strong(10.5))
                .color(p.text_weak),
        );
        if app.cfg.clients.is_empty() {
            ui.label(RichText::new("No clients yet").color(p.text_weak));
        }
        for (key, c) in &app.cfg.clients {
            let name = if c.party.name.is_empty() {
                key.clone()
            } else {
                c.party.name.clone()
            };
            if ui
                .button(name)
                .on_hover_text(format!("client: {key}"))
                .clicked()
            {
                pick = Some(key.clone());
            }
        }
        ui.separator();
        if ui.button("Manage clients...").clicked() {
            manage = true;
        }
    })
    .response
    .on_hover_text("New invoice for a specific client");
    if let Some(key) = pick {
        app.request(Pending::New(Some(key)));
    }
    if manage {
        app.open_clients();
    }
}

fn header_info(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    // File name, dirty dot and location.
    let name_w = ui.available_width().min(300.0);
    ui.allocate_ui_with_layout(vec2(name_w, 36.0), Layout::top_down(Align::Min), |ui| {
        ui.set_max_width(name_w);
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(app.display_name())
                        .font(theme::strong(14.0))
                        .color(p.text_strong),
                )
                .truncate(),
            );
            if app.dirty() {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 3.5, p.accent);
            }
        });
        let loc = match &app.path {
            Some(path) => path.parent().map(tilde).unwrap_or_default(),
            None => format!("new, saves to {}", tilde(&app.cfg.invoices_dir)),
        };
        ui.add(egui::Label::new(RichText::new(loc).size(11.0).color(p.text_weak)).truncate());
    });

    // Invoice summary from the last good parse, as much as fits.
    let Some(inv) = app.invoice.clone() else {
        return;
    };
    let total = format_money(&inv.currency, inv.total());
    let client = if inv.to.name.is_empty() {
        "No client".to_string()
    } else {
        inv.to.name.clone()
    };
    let fits = |ui: &egui::Ui, w: f32| ui.available_width() > w;
    if !fits(ui, 140.0) {
        return;
    }
    ui.add_space(10.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 28.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, p.border);
    ui.add_space(10.0);
    summary_item(ui, p, "TOTAL", &total, Some(p.text_strong), 16.0);
    if fits(ui, 120.0) {
        ui.add_space(14.0);
        summary_item(ui, p, "INVOICE", &inv.number, None, 13.5);
    }
    if fits(ui, 90.0) {
        ui.add_space(14.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(RichText::new("STATUS").size(9.5).color(p.text_weak));
            status_pill(ui, p, Some(inv.status));
        });
    }
    if fits(ui, 120.0) {
        ui.add_space(14.0);
        let w = ui.available_width().min(260.0);
        ui.allocate_ui_with_layout(vec2(w, 36.0), Layout::top_down(Align::Min), |ui| {
            ui.set_max_width(w);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new("CLIENT").size(9.5).color(p.text_weak));
            ui.add(
                egui::Label::new(
                    RichText::new(client)
                        .font(theme::strong(13.5))
                        .color(p.text),
                )
                .truncate(),
            );
        });
    }
}

fn summary_item(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    value: &str,
    color: Option<Color32>,
    size: f32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(RichText::new(label).size(9.5).color(p.text_weak));
        ui.label(
            RichText::new(value)
                .font(theme::strong(size))
                .color(color.unwrap_or(p.text)),
        );
    });
}

fn status_colors(p: &Palette, s: Option<Status>) -> (Color32, &'static str) {
    match s {
        Some(Status::Paid) => (p.ok, "paid"),
        Some(Status::Pending) => (p.warn, "pending"),
        Some(Status::Overdue) => (p.error, "overdue"),
        Some(Status::Draft) => (p.text_weak, "draft"),
        None => (p.error, "error"),
    }
}

fn status_pill(ui: &mut egui::Ui, p: &Palette, s: Option<Status>) -> egui::Response {
    let (color, label) = status_colors(p, s);
    let font = FontId::new(10.5, FontFamily::Name(theme::STRONG.into()));
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, color);
    let size = galley.size() + vec2(14.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(255), color.gamma_multiply(0.16));
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

// ------------------------------------------------------------------ sidebar

fn sidebar(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("INVOICES")
                .font(theme::strong(11.0))
                .color(p.text_weak),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(app.library.len().to_string())
                    .size(11.0)
                    .color(p.text_weak),
            );
        });
    });
    ui.add(
        egui::Label::new(
            RichText::new(tilde(&app.cfg.invoices_dir))
                .size(10.5)
                .color(p.text_weak),
        )
        .truncate(),
    );
    ui.add_space(6.0);

    if app.library.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No invoices yet").color(p.text_weak));
            ui.label(
                RichText::new("Ctrl+N creates one")
                    .size(11.0)
                    .color(p.text_weak),
            );
        });
        return;
    }

    let current = app.path.clone();
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for e in &app.library {
                let selected = current.as_ref() == Some(&e.path);
                let width = ui.available_width();
                let (rect, resp) = ui.allocate_exact_size(vec2(width, 52.0), Sense::click());
                let fill = if selected {
                    p.accent_soft
                } else if resp.hovered() {
                    p.surface
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
                if selected {
                    let bar = Rect::from_min_size(
                        rect.left_top() + vec2(0.0, 10.0),
                        vec2(3.0, rect.height() - 20.0),
                    );
                    ui.painter()
                        .rect_filled(bar, CornerRadius::same(2), p.accent);
                }
                let inner = rect.shrink2(vec2(12.0, 9.0));
                let painter = ui.painter_at(rect);
                // Top row: number and total.
                let total = painter.layout_no_wrap(e.total.clone(), theme::strong(12.0), p.text);
                let total_w = total.size().x;
                painter.galley(
                    pos2(inner.right() - total_w, inner.top() + 1.0),
                    total,
                    p.text,
                );
                let num = truncated(
                    &painter,
                    &e.number,
                    theme::strong(13.0),
                    p.text_strong,
                    inner.width() - total_w - 8.0,
                );
                painter.galley(inner.left_top(), num, p.text_strong);
                // Bottom row: client and status pill.
                let (color, label) = status_colors(p, e.status);
                let galley = painter.layout_no_wrap(
                    label.to_string(),
                    FontId::new(9.5, FontFamily::Name(theme::STRONG.into())),
                    color,
                );
                let pill = Rect::from_min_size(
                    pos2(
                        inner.right() - galley.size().x - 12.0,
                        inner.bottom() - galley.size().y - 3.0,
                    ),
                    galley.size() + vec2(12.0, 3.0),
                );
                painter.rect_filled(pill, CornerRadius::same(255), color.gamma_multiply(0.16));
                painter.galley(pill.center() - galley.size() / 2.0, galley, color);
                let client = truncated(
                    &painter,
                    &e.client,
                    FontId::proportional(11.5),
                    p.text_weak,
                    inner.width() - pill.width() - 8.0,
                );
                painter.galley(
                    pos2(inner.left(), inner.bottom() - client.size().y),
                    client,
                    p.text_weak,
                );

                let resp = resp.on_hover_ui(|ui| {
                    ui.label(tilde(&e.path));
                    if let Some(d) = e.date {
                        ui.label(RichText::new(format_date(d)).color(p.text_weak));
                    }
                });
                if resp.clicked() && !selected {
                    clicked = Some(e.path.clone());
                }
                resp.on_hover_cursor(CursorIcon::PointingHand);
            }
        });
    if let Some(path) = clicked {
        app.request(Pending::Open(path));
    }
}

/// Single line galley cut with an ellipsis to `max_w`.
fn truncated(
    painter: &egui::Painter,
    text: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, max_w.max(10.0));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    painter.layout_job(job)
}

// ------------------------------------------------------------------ status bar

fn status_bar(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    ui.spacing_mut().interact_size.y = 18.0;
    ui.horizontal(|ui| {
        ui.set_height(18.0);
        let (dot, text) = match &app.error {
            Some((line, msg)) if *line > 0 => (p.error, format!("Line {line}: {msg}")),
            Some((_, msg)) => (p.error, msg.clone()),
            None if app.invoice.is_some() => (p.ok, "Preview up to date".to_string()),
            None => (p.text_weak, "Rendering...".to_string()),
        };
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 3.5, dot);
        let resp = ui.add(
            egui::Label::new(
                RichText::new(text)
                    .size(11.5)
                    .color(if app.error.is_some() {
                        p.error
                    } else {
                        p.text_weak
                    }),
            )
            .truncate()
            .sense(Sense::click()),
        );
        if let Some((line, _)) = app.error
            && line > 0
        {
            if resp.clicked() {
                app.goto_line = Some(line);
            }
            resp.on_hover_text("Click to jump to the line");
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let small = |s: String| RichText::new(s).size(11.5).color(p.text_weak);
            let fit = app.prefs.fit_width;
            if ui
                .add(
                    egui::Button::new(small("Fit width".into()))
                        .frame(false)
                        .selected(fit),
                )
                .on_hover_text("Ctrl+0")
                .clicked()
            {
                app.prefs.fit_width = !fit;
            }
            ui.label(small(format!("{:.0}%", app.prefs.zoom * 100.0)));
            if !app.pages.is_empty() {
                let n = app.pages.len();
                ui.label(small(format!("{n} page{}", if n == 1 { "" } else { "s" })));
            }
            if app.in_flight > 0 || app.preview_due.is_some() {
                ui.add(egui::Spinner::new().size(12.0).color(p.text_weak));
            }
            if let Some(inv) = &app.invoice {
                ui.label(small(format!(
                    "{} item{}, due {}",
                    inv.items.len(),
                    if inv.items.len() == 1 { "" } else { "s" },
                    inv.due
                        .map(format_date)
                        .unwrap_or_else(|| "on receipt".into())
                )));
            }
        });
    });
}

// ------------------------------------------------------------------ split view

fn work_area(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let full = ui.max_rect();
    let min_side = 260.0_f32.min(full.width() / 3.0);
    let split_x = (full.left() + full.width() * app.prefs.split)
        .clamp(full.left() + min_side, full.right() - min_side);

    let editor_rect = Rect::from_min_max(full.min, pos2(split_x, full.bottom()));
    let preview_rect = Rect::from_min_max(pos2(split_x + 1.0, full.top()), full.max);

    // Divider.
    let handle = Rect::from_center_size(pos2(split_x, full.center().y), vec2(8.0, full.height()));
    let resp = ui
        .interact(handle, Id::unique("split-divider"), Sense::drag())
        .on_hover_cursor(CursorIcon::ResizeHorizontal);
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        app.prefs.split = ((pos.x - full.left()) / full.width()).clamp(0.15, 0.85);
    }
    if resp.double_clicked() {
        app.prefs.split = 0.46;
    }

    ui.scope_builder(UiBuilder::new().max_rect(editor_rect), |ui| {
        editor(app, ui, p)
    });
    ui.scope_builder(UiBuilder::new().max_rect(preview_rect), |ui| {
        preview(app, ui, p)
    });

    let active = resp.hovered() || resp.dragged();
    let line_color = if active { p.accent } else { p.border };
    let w = if active { 2.0 } else { 1.0 };
    ui.painter().rect_filled(
        Rect::from_center_size(pos2(split_x, full.center().y), vec2(w, full.height())),
        0.0,
        line_color,
    );
}

// ------------------------------------------------------------------ editor

fn line_of_char(text: &str, char_idx: usize) -> usize {
    1 + text.chars().take(char_idx).filter(|c| *c == '\n').count()
}

fn editor(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, p.editor_bg);
    let font = FontId::monospace(13.5);
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    let lines = app.text.split('\n').count().max(1);
    let digits = lines.to_string().len().max(2) as f32;
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let gutter = digits * char_w + 28.0;

    let error_line = app.error.as_ref().map(|e| e.0).unwrap_or(0);
    let error_msg = app.error.as_ref().map(|e| e.1.clone());
    let pal = *p;
    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut job = highlight::highlight(buf.as_str(), font.clone(), &pal, error_line);
        job.wrap.max_width = wrap_width;
        ui.fonts_mut(|f| f.layout_job(job))
    };

    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("editor-scroll")
        .auto_shrink([false, false]);
    if let Some(line) = app.goto_line.take() {
        let y = (line.saturating_sub(1) as f32 * row_h - rect.height() / 3.0).max(0.0);
        scroll = scroll.vertical_scroll_offset(y);
    }

    scroll.show(ui, |ui| {
        let bg_idx = ui.painter().add(Shape::Noop);
        let gutter_idx = ui.painter().add(Shape::Noop);
        let width = ui.available_width();
        let out = egui::TextEdit::multiline(&mut app.text)
            .id_salt("editor")
            .font(font.clone())
            .frame(Frame::NONE.inner_margin(Margin {
                left: gutter as i8,
                right: 14,
                top: 14,
                bottom: 120,
            }))
            .desired_width(width)
            .desired_rows(40)
            .lock_focus(true)
            .layouter(&mut layouter)
            .show(ui);

        if out.response.response.changed() {
            app.text_changed(false);
        }

        let galley = &out.galley;
        let origin = out.galley_pos;
        let full_w = ui.min_rect().width().max(width);
        let left = ui.min_rect().left();
        let cursor_line = out
            .cursor_range
            .map(|c| line_of_char(&app.text, c.primary.index.0));
        let has_focus = out.response.response.has_focus();

        let mut bg = Vec::new();
        let mut nums = Vec::new();
        let mut line = 1usize;
        let mut starts_line = true;
        let mut err_row_rect: Option<Rect> = None;
        // Each logical line spans one or more rows; band them all.
        for row in galley.rows.iter() {
            let r = row.rect().translate(origin.to_vec2());
            let band = Rect::from_min_max(pos2(left, r.top()), pos2(left + full_w, r.bottom()));
            if line == error_line {
                bg.push(Shape::rect_filled(band, 0.0, p.error_bg));
                if err_row_rect.is_none() {
                    err_row_rect = Some(r);
                }
            } else if has_focus && Some(line) == cursor_line {
                bg.push(Shape::rect_filled(
                    band,
                    0.0,
                    p.text
                        .gamma_multiply(if ui.visuals().dark_mode { 0.04 } else { 0.05 }),
                ));
            }
            if starts_line {
                let color = if line == error_line {
                    p.error
                } else if Some(line) == cursor_line {
                    p.text
                } else {
                    p.gutter
                };
                let g =
                    ui.painter()
                        .layout_no_wrap(line.to_string(), FontId::monospace(12.0), color);
                let pos = pos2(
                    left + gutter - 14.0 - g.size().x,
                    r.top() + (row_h - g.size().y) / 2.0,
                );
                nums.push(Shape::galley(pos, g, color));
                if line == error_line {
                    nums.push(Shape::circle_filled(
                        pos2(left + 8.0, r.center().y),
                        3.0,
                        p.error,
                    ));
                }
            }
            starts_line = row.ends_with_newline;
            if row.ends_with_newline {
                line += 1;
            }
        }
        // Gutter divider.
        let clip = ui.clip_rect();
        nums.push(Shape::line_segment(
            [
                pos2(left + gutter - 6.0, clip.top()),
                pos2(left + gutter - 6.0, clip.bottom()),
            ],
            Stroke::new(1.0, p.border.gamma_multiply(0.6)),
        ));
        ui.painter().set(bg_idx, Shape::Vec(bg));
        ui.painter().set(gutter_idx, Shape::Vec(nums));

        // Inline error message at the end of the failing line.
        if let (Some(r), Some(msg)) = (err_row_rect, error_msg) {
            let font = FontId::proportional(11.5);
            let text = format!("\u{25CF} {msg}");
            let x = r.right() + 28.0;
            let room = ui.clip_rect().right() - 14.0 - x;
            if room > 60.0 {
                let g = truncated(ui.painter(), &text, font, p.error.gamma_multiply(0.9), room);
                let pos = pos2(x, r.center().y - g.size().y / 2.0);
                ui.painter().galley(pos, g, p.error);
            }
        }
    });
}

// ------------------------------------------------------------------ preview

fn preview(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let ctx = ui.ctx().clone();
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, p.canvas);
    app.preview_size = rect.size();

    if app.pages.is_empty() {
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.centered_and_justified(|ui| {
                let msg = match &app.error {
                    Some((_, m)) => format!("Nothing to preview yet\n\n{m}"),
                    None => "Rendering preview...".into(),
                };
                ui.label(RichText::new(msg).color(p.text_weak));
            });
        });
        return;
    }

    let max_page_w = app
        .pages
        .iter()
        .map(|pg| pg.size_pt.x)
        .fold(0.0_f32, f32::max)
        .max(1.0);
    if app.prefs.fit_width {
        let avail = rect.width() - 2.0 * PAGE_PAD - 10.0;
        app.prefs.zoom = (avail / max_page_w).clamp(0.25, 5.0);
    }

    // Ctrl+wheel zoom around the pointer.
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        let (zd, ptr) = ctx.input(|i| (i.zoom_delta(), i.pointer.hover_pos()));
        if (zd - 1.0).abs() > 1e-4 {
            let anchor = ptr.map(|p| p - rect.min).map(|v| v.to_pos2());
            // Wheel notches arrive as large steps; soften them.
            app.set_zoom(app.prefs.zoom * zd.powf(0.4), anchor);
        }
    }
    let zoom = app.prefs.zoom;

    let scroll_id = Id::unique("preview-scroll");
    let mut area = egui::ScrollArea::both()
        .id_salt(scroll_id)
        .auto_shrink([false, false]);
    if let Some((anchor, factor)) = app.zoom_anchor.take() {
        let off = app.preview_offset;
        let new = (off + anchor.to_vec2()) * factor - anchor.to_vec2();
        area = area.scroll_offset(new.max(Vec2::ZERO));
    }

    let failing = app.error.is_some();
    let out = area.show(ui, |ui| {
        let content_w = (max_page_w * zoom + 2.0 * PAGE_PAD).max(rect.width() - 2.0);
        let total_h: f32 = app.pages.iter().map(|pg| pg.size_pt.y * zoom).sum::<f32>()
            + PAGE_GAP * (app.pages.len().saturating_sub(1)) as f32
            + 2.0 * PAGE_PAD;
        let (area_rect, _) = ui.allocate_exact_size(vec2(content_w, total_h), Sense::hover());
        let mut y = area_rect.top() + PAGE_PAD;
        let painter = ui.painter();
        for pg in &app.pages {
            let size = pg.size_pt * zoom;
            let page = Rect::from_min_size(pos2(area_rect.center().x - size.x / 2.0, y), size);
            let page =
                Rect::from_min_size(pos2(page.min.x.round(), page.min.y.round()), page.size());
            if ui.is_rect_visible(page.expand(30.0)) {
                painter.add(
                    Shadow {
                        offset: [0, 6],
                        blur: 24,
                        spread: 0,
                        color: p.page_shadow,
                    }
                    .as_shape(page, 2),
                );
                painter.rect_filled(page, 2, Color32::WHITE);
                painter.image(
                    pg.tex.id(),
                    page,
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                painter.rect_stroke(
                    page,
                    2,
                    Stroke::new(1.0, Color32::from_black_alpha(18)),
                    StrokeKind::Outside,
                );
            }
            y += size.y + PAGE_GAP;
        }
    });
    app.preview_offset = out.state.unclamped_offset();

    // Overlays: stale preview veil + error pill, spinner.
    if failing {
        ui.painter().rect_filled(
            rect,
            0.0,
            p.canvas
                .gamma_multiply(if ui.visuals().dark_mode { 0.35 } else { 0.30 }),
        );
        let msg = match &app.error {
            Some((l, m)) if *l > 0 => format!("Line {l}: {m}"),
            Some((_, m)) => m.clone(),
            None => String::new(),
        };
        let text = format!("Showing last good preview   \u{2022}   {msg}");
        let font = FontId::proportional(12.0);
        let g = ui
            .painter()
            .layout(text, font, p.error, (rect.width() - 80.0).min(560.0));
        let pill = Rect::from_center_size(
            pos2(
                rect.center().x,
                rect.top() + 18.0 + (g.size().y + 12.0) / 2.0,
            ),
            g.size() + vec2(24.0, 12.0),
        );
        ui.painter().add(
            Shadow {
                offset: [0, 3],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha(60),
            }
            .as_shape(pill, 8),
        );
        ui.painter().rect(
            pill,
            8,
            p.surface,
            Stroke::new(1.0, p.error.gamma_multiply(0.5)),
            StrokeKind::Inside,
        );
        ui.painter().galley(pill.min + vec2(12.0, 6.0), g, p.error);
    }
    if app.in_flight > 0 {
        let c = pos2(rect.right() - 22.0, rect.top() + 20.0);
        let r = Rect::from_center_size(c, vec2(16.0, 16.0));
        ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
            ui.add(egui::Spinner::new().size(14.0).color(p.text_weak));
        });
    }
}

// ------------------------------------------------------------------ toasts

fn toasts(app: &mut App, ctx: &egui::Context, p: &Palette) {
    const LIFE: f32 = 4.0;
    let now = Instant::now();
    app.toasts.retain(|t| {
        now.duration_since(t.born).as_secs_f32() < LIFE + if t.open.is_some() { 3.0 } else { 0.0 }
    });
    if app.toasts.is_empty() {
        return;
    }
    ctx.request_repaint_after(Duration::from_millis(33));
    let screen = ctx.content_rect();
    let mut open_path = None;
    let mut y = screen.bottom() - 40.0;
    for (i, t) in app.toasts.iter().enumerate().rev().take(4) {
        let life = LIFE + if t.open.is_some() { 3.0 } else { 0.0 };
        let age = now.duration_since(t.born).as_secs_f32();
        let alpha = (age / 0.18)
            .min(1.0)
            .min((life - age) / 0.4)
            .clamp(0.0, 1.0);
        let slide = (1.0 - (age / 0.18).min(1.0)) * 12.0;
        let accent = match t.kind {
            ToastKind::Success => p.ok,
            ToastKind::Error => p.error,
            ToastKind::Info => p.accent,
        };
        let resp = egui::Area::new(Id::unique(("toast", i, t.born)))
            .order(egui::Order::Tooltip)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(pos2(screen.right() - 18.0, y + slide))
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(9))
                    .shadow(Shadow {
                        offset: [0, 6],
                        blur: 20,
                        spread: 0,
                        color: Color32::from_black_alpha(80),
                    })
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, accent);
                            ui.add(egui::Label::new(RichText::new(&t.text).color(p.text)).wrap());
                            if let Some(path) = &t.open
                                && ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Open")
                                                .color(p.accent)
                                                .font(theme::strong(12.5)),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                            {
                                open_path = Some(path.clone());
                            }
                        });
                    });
            });
        y -= resp.response.rect.height() + 8.0;
    }
    if let Some(path) = open_path {
        open_external(&path);
    }
}

// ------------------------------------------------------------------ dialogs

fn confirm_modal(app: &mut App, ctx: &egui::Context, p: &Palette) {
    let Some(action) = app.confirm.clone() else {
        return;
    };
    let verb = match &action {
        Pending::New(_) => "creating a new invoice",
        Pending::Open(_) => "opening another file",
        Pending::Close => "closing",
    };
    let mut choice = None;
    let resp = egui::Modal::new(Id::unique("confirm-unsaved"))
        .backdrop_color(Color32::from_black_alpha(120))
        .frame(
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20))
                .shadow(ctx.global_style().visuals.window_shadow),
        )
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new("Unsaved changes")
                    .font(theme::strong(16.0))
                    .color(p.text_strong),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "{} has changes that are not saved. Save them before {verb}?",
                    app.display_name()
                ))
                .color(p.text),
            );
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let save = egui::Button::new(
                    RichText::new("Save")
                        .font(theme::strong(13.0))
                        .color(Color32::WHITE),
                )
                .fill(p.accent);
                if ui.add(save).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    choice = Some(1);
                }
                if ui.button("Discard").clicked() {
                    choice = Some(2);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(0);
                }
            });
        });
    if resp.should_close() && choice.is_none() {
        choice = Some(0);
    }
    match choice {
        Some(0) => app.confirm = None,
        Some(1) => {
            app.confirm = None;
            app.save(Some(action));
        }
        Some(2) => {
            app.confirm = None;
            // Treat as saved so the action goes through without asking again.
            app.saved_text = app.text.clone();
            app.perform(action);
        }
        _ => {}
    }
}

fn drop_overlay(ctx: &egui::Context, p: &Palette) {
    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
    if !hovering {
        return;
    }
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::unique("drop"),
    ));
    painter.rect_filled(screen, 0.0, Color32::from_black_alpha(140));
    let r = screen.shrink(24.0);
    painter.rect_stroke(r, 14, Stroke::new(2.0, p.accent), StrokeKind::Inside);
    painter.text(
        r.center(),
        Align2::CENTER_CENTER,
        "Drop a .md invoice to open it",
        theme::strong(20.0),
        Color32::WHITE,
    );
}
