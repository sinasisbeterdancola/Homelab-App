//! Look & feel: theme colours, Font Awesome icons and small reusable widgets.

use eframe::egui::{
    self, Button, Color32, FontData, FontDefinitions, FontFamily, Frame, Margin, Response, RichText,
    Sense, Stroke, TextEdit, Ui,
};

pub mod theme {
    use eframe::egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(13, 15, 20);
    pub const CARD: Color32 = Color32::from_rgb(22, 25, 32);
    pub const INPUT: Color32 = Color32::from_rgb(16, 18, 24);
    pub const RAISED: Color32 = Color32::from_rgb(32, 36, 46);
    pub const RAISED_HOVER: Color32 = Color32::from_rgb(42, 47, 59);
    pub const LINE: Color32 = Color32::from_rgb(36, 40, 50);
    pub const LINE_STRONG: Color32 = Color32::from_rgb(58, 64, 78);
    pub const TEXT: Color32 = Color32::from_rgb(232, 235, 241);
    pub const MUTED: Color32 = Color32::from_rgb(136, 145, 162);
    pub const ACCENT: Color32 = Color32::from_rgb(91, 140, 255);
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(116, 158, 255);
    pub const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
    pub const AMBER: Color32 = Color32::from_rgb(251, 191, 36);
    pub const RED: Color32 = Color32::from_rgb(248, 113, 113);
    pub const CARD_PAD: f32 = 20.0;
}
use theme::*;

/// Font Awesome 6 Free (solid) code points.
pub mod icon {
    pub const SERVER: &str = "\u{f233}";
    pub const HOUSE: &str = "\u{f015}";
    pub const PLUS: &str = "\u{2b}";
    pub const XMARK: &str = "\u{f00d}";
    pub const TRASH: &str = "\u{f1f8}";
    pub const TERMINAL: &str = "\u{f120}";
    pub const REFRESH: &str = "\u{f021}";
    pub const TEMP: &str = "\u{f2c9}";
    pub const GAUGE: &str = "\u{f625}";
    pub const MEMORY: &str = "\u{f538}";
    pub const DISK: &str = "\u{f0a0}";
    pub const CHIP: &str = "\u{f2db}";
    pub const CLOCK: &str = "\u{f017}";
    pub const PLUG: &str = "\u{f1e6}";
    pub const WARN: &str = "\u{f071}";
    pub const USER: &str = "\u{f007}";
    pub const LOCK: &str = "\u{f023}";
    pub const NETWORK: &str = "\u{f6ff}";
    pub const TAG: &str = "\u{f02b}";
    pub const SHIELD: &str = "\u{f3ed}";
    pub const BOLT: &str = "\u{f0e7}";
    pub const STETHOSCOPE: &str = "\u{f0f1}";
    pub const WIFI: &str = "\u{f1eb}";
    pub const GLOBE: &str = "\u{f0ac}";
    // Not f002 (magnifying glass): egui's default font maps that code point to "fl".
    pub const SEARCH: &str = "\u{f1c0}";
    pub const STOPWATCH: &str = "\u{f2f2}";
    pub const DOWNLOAD: &str = "\u{f019}";
    pub const UPLOAD: &str = "\u{f093}";
    pub const BOX: &str = "\u{f466}";
    pub const LIST_CHECK: &str = "\u{f0ae}";
    pub const FILE: &str = "\u{f15c}";
    pub const SWAP: &str = "\u{f362}";
    pub const CHART: &str = "\u{f080}";
    pub const DESKTOP: &str = "\u{f390}";
    pub const ROUTER: &str = "\u{f6ff}";
    pub const PLAY: &str = "\u{f04b}";
    pub const POWER: &str = "\u{f011}";
    pub const PEN: &str = "\u{f304}";
    pub const REBOOT: &str = "\u{f2f9}";
    pub const CLOUD_DOWN: &str = "\u{f0ed}";
}

/// Small spaced-out uppercase heading for a group of rows.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .extra_letter_spacing(1.2)
            .color(MUTED),
    );
}

/// Label on the left, coloured value on the right.
pub fn diag_row(ui: &mut Ui, icon: &str, label: &str, value: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::hover());
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(13.0), MUTED);
        ui.label(RichText::new(label).color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let value: String = if value.chars().count() > 46 {
                value.chars().take(45).chain(['…']).collect()
            } else {
                value.to_string()
            };
            ui.label(RichText::new(value).color(color));
        });
    });
}

/// Big number with a unit, for speedtest results.
pub fn speed_tile(ui: &mut Ui, icon: &str, label: &str, value: &str, unit: &str) {
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.label(RichText::new(format!("{icon}  {label}")).small().color(MUTED));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(value).size(24.0).strong().color(TEXT));
        ui.label(RichText::new(unit).small().color(MUTED));
    });
}

/// Rounded inset panel used for sections in dialogs.
pub fn inset(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::none()
        .fill(BG)
        .stroke(Stroke::new(1.0_f32, LINE))
        .rounding(12.0)
        .inner_margin(Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Installs the icon font and the dark theme. Everything is embedded in the
/// exe, so there is nothing to load from disk or the network at startup.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("fa".into(), FontData::from_static(include_bytes!("../assets/fa-solid-900.ttf")));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.get_mut(&family).unwrap().push("fa".into());
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0_f32, LINE_STRONG);
    v.window_rounding = 16.0.into();
    v.window_shadow = egui::epaint::Shadow { offset: egui::vec2(0.0, 12.0), blur: 40.0, spread: 0.0, color: Color32::from_black_alpha(150) };
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    for (w, fill) in [
        (&mut v.widgets.inactive, RAISED),
        (&mut v.widgets.hovered, RAISED_HOVER),
        (&mut v.widgets.active, RAISED_HOVER),
        (&mut v.widgets.open, RAISED),
    ] {
        w.weak_bg_fill = fill;
        w.bg_fill = fill;
        w.rounding = 8.0.into();
        w.fg_stroke.color = TEXT;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.noninteractive.fg_stroke.color = TEXT;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, LINE_STRONG);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        use egui::{FontId, TextStyle::*};
        s.text_styles = [
            (Heading, FontId::proportional(21.0)),
            (Body, FontId::proportional(14.0)),
            (Button, FontId::proportional(14.0)),
            (Small, FontId::proportional(12.0)),
            (Monospace, FontId::monospace(12.5)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
        s.spacing.interact_size.y = 32.0;
        s.spacing.window_margin = Margin::same(24.0);
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.floating = true;
    });
}

pub fn page_frame() -> Frame {
    Frame::none().fill(BG).inner_margin(Margin { left: 28.0, right: 28.0, top: 4.0, bottom: 20.0 })
}

pub fn text_input(text: &mut String) -> TextEdit<'_> {
    TextEdit::singleline(text).desired_width(f32::INFINITY).margin(egui::vec2(10.0, 8.0))
}

pub fn field_label(ui: &mut Ui, icon: &str, text: &str) {
    ui.label(RichText::new(format!("{icon}  {text}")).small().color(MUTED));
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    let resp = ui.add(
        Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(ACCENT)
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(0.0, 34.0)),
    );
    if resp.hovered() {
        ui.painter().rect_filled(resp.rect, 8.0, ACCENT_HOVER.gamma_multiply(0.25));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(Button::new(RichText::new(text).color(TEXT)).min_size(egui::vec2(0.0, 34.0)))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn danger_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(Color32::from_rgb(220, 70, 70))
            .min_size(egui::vec2(0.0, 30.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str) -> Response {
    ui.add(
        Button::new(RichText::new(icon).size(14.0).color(MUTED))
            .frame(false)
            .min_size(egui::vec2(30.0, 30.0)),
    )
    .on_hover_text(tip)
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Rounded square with a coloured icon.
pub fn icon_tile(ui: &mut Ui, icon: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), Sense::hover());
    ui.painter().rect_filled(rect, 10.0, color.gamma_multiply(0.14));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(17.0),
        color,
    );
}

/// Small rounded status label with a dot, sized exactly to its text.
pub fn pill(ui: &mut Ui, text: &str, color: Color32, pulse: bool) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), color);
    let (pad_x, dot, gap) = (10.0, 7.0, 6.0);
    let size = egui::vec2(pad_x + dot + gap + galley.size().x + pad_x, galley.size().y + 9.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

    let alpha = if pulse {
        ui.ctx().request_repaint();
        0.45 + 0.55 * (ui.input(|i| i.time) as f32 * 3.0).sin().abs()
    } else {
        1.0
    };
    let p = ui.painter();
    p.rect_filled(rect, 99.0, color.gamma_multiply(0.13));
    p.circle_filled(egui::pos2(rect.left() + pad_x + dot / 2.0, rect.center().y), dot / 2.0, color.gamma_multiply(alpha));
    let text_pos = egui::pos2(rect.left() + pad_x + dot + gap, rect.center().y - galley.size().y / 2.0);
    p.galley(text_pos, galley, color);
}

/// One stat: icon + label, big value, thin coloured bar.
pub fn metric(ui: &mut Ui, icon: &str, label: &str, value: &str, frac: f64, color: Color32) {
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(RichText::new(format!("{icon}  {label}")).small().color(MUTED));
    ui.label(RichText::new(value).size(15.0).strong().color(TEXT));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, LINE);
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0) as f32);
    if fill.width() > 0.0 {
        p.rect_filled(fill, 3.0, color);
    }
}

/// Small line chart of recent values. Newest point on the right; `capacity`
/// fixes the x-scale so the line grows in from the right as data arrives.
pub fn sparkline(
    ui: &mut Ui,
    label: &str,
    values: &std::collections::VecDeque<f32>,
    capacity: usize,
    unit: &str,
    color: Color32,
) {
    let (min, max) = values.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("{min:.1} – {max:.1} {unit}")).small().color(MUTED));
        });
    });

    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 6.0, BG);
    let inner = rect.shrink2(egui::vec2(8.0, 6.0));
    // At least a 2-degree window, so small jitter doesn't look like big swings.
    let span = (max - min).max(2.0);
    let lo = (min + max) / 2.0 - span / 2.0;
    let step = inner.width() / (capacity.max(2) - 1) as f32;
    let n = values.len();
    let points: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = inner.right() - (n - 1 - i) as f32 * step;
            let y = inner.bottom() - (v - lo) / span * inner.height();
            egui::pos2(x, y)
        })
        .collect();
    p.add(egui::Shape::line(points.clone(), Stroke::new(1.6_f32, color)));
    if let Some(last) = points.last() {
        p.circle_filled(*last, 2.8, color);
    }
}

/// Tinted message box; `icon: None` shows a spinner.
pub fn notice(ui: &mut Ui, icon: Option<&str>, text: &str, color: Color32) {
    Frame::none()
        .fill(color.gamma_multiply(0.08))
        .rounding(10.0)
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                match icon {
                    Some(i) => {
                        ui.label(RichText::new(i).color(color));
                    }
                    None => {
                        ui.add(egui::Spinner::new().size(14.0).color(color));
                    }
                }
                // Long messages (errors with paths) wrap instead of running off-screen.
                ui.add(egui::Label::new(RichText::new(text).color(TEXT)).wrap());
            });
        });
}

pub fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, LINE);
}

/// Darkens everything behind a modal dialog.
pub fn dim_background(ctx: &egui::Context) {
    let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new("dim"));
    ctx.layer_painter(layer).rect_filled(ctx.screen_rect(), 0.0, Color32::from_black_alpha(160));
}
