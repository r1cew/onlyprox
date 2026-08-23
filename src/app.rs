use std::sync::{Arc, Mutex};

use eframe::egui;
use tokio::sync::mpsc;

use crate::backend::{self, Command, UiConfig, UiState};

pub struct OnlyProxApp {
    tx: mpsc::Sender<Command>,
    shared: Arc<Mutex<UiState>>,

    show_sources: bool,
    new_sub_name: String,
    new_sub_url: String,
    new_custom_link: String,
}

impl OnlyProxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, shared) = backend::spawn(cc.egui_ctx.clone());
        let _ = tx.try_send(Command::LoadAll);

        Self {
            tx,
            shared,
            show_sources: false,
            new_sub_name: String::new(),
            new_sub_url: String::new(),
            new_custom_link: String::new(),
        }
    }

    fn send(&self, cmd: Command) {
        let _ = self.tx.try_send(cmd);
    }
}

// ---------------------------------------------------------------------
// Палитра
// ---------------------------------------------------------------------

mod palette {
    use eframe::egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(18, 18, 18);
    pub const PANEL: Color32 = Color32::from_rgb(28, 28, 30);
    pub const CARD: Color32 = Color32::from_rgb(36, 36, 38);
    pub const CARD_HOVER: Color32 = Color32::from_rgb(46, 46, 49);
    pub const CARD_SELECTED: Color32 = Color32::from_rgb(58, 58, 62);
    pub const CARD_CONNECTED: Color32 = Color32::from_rgb(20, 66, 34);
    pub const ACCENT: Color32 = Color32::from_rgb(48, 209, 88);
    pub const ACCENT_BLUE: Color32 = Color32::from_rgb(10, 132, 255);
    pub const DANGER: Color32 = Color32::from_rgb(255, 92, 92);
    pub const TEXT: Color32 = Color32::from_rgb(235, 235, 237);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(150, 150, 156);
}

impl eframe::App for OnlyProxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = self.shared.lock().unwrap().clone();

        egui::SidePanel::left("left_panel")
            .resizable(false)
            .exact_width(230.0)
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(egui::Margin::same(14.0)))
            .show(ctx, |ui| self.left_panel(ui, &state));

        if self.show_sources {
            egui::SidePanel::right("sources_panel")
                .resizable(false)
                .exact_width(360.0)
                .frame(egui::Frame::none().fill(palette::PANEL).inner_margin(egui::Margin::same(16.0)))
                .show(ctx, |ui| self.sources_panel(ui, &state));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::PANEL).inner_margin(egui::Margin::same(14.0)))
            .show(ctx, |ui| self.config_list(ui, &state));

        if state.is_searching {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.tx.try_send(Command::Shutdown);
        std::thread::sleep(std::time::Duration::from_millis(350));
    }
}

impl OnlyProxApp {
    fn left_panel(&mut self, ui: &mut egui::Ui, state: &UiState) {
        ui.add_space(10.0);

        ui.vertical_centered(|ui| {
            let size = egui::vec2(112.0, 112.0);
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

            let base = if state.is_connected { palette::ACCENT } else { palette::CARD };
            let color = if resp.hovered() { base.linear_multiply(1.2) } else { base };
            let icon_color = if state.is_connected { egui::Color32::WHITE } else { palette::TEXT };

            ui.painter().rect_filled(rect, egui::Rounding::same(16.0), color);
            draw_power_icon(ui.painter(), rect.center(), 22.0, icon_color);

            if resp.clicked() {
                self.send(Command::ToggleVpn);
            }

            ui.add_space(10.0);
            let status = if state.is_connected { "Подключено" } else { "Отключено" };
            let status_color = if state.is_connected { palette::ACCENT } else { palette::TEXT_MUTED };
            ui.label(egui::RichText::new(status).color(status_color).strong());
        });

        ui.add_space(20.0);

        let btn_h = 54.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), btn_h), egui::Sense::click());
        let bg = if resp.hovered() { palette::CARD_HOVER } else { palette::CARD };
        ui.painter().rect_filled(rect, egui::Rounding::same(10.0), bg);

        if state.is_searching {
            let fill_w = rect.width() * state.search_progress.clamp(0.0, 1.0);
            let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
            ui.painter().rect_filled(fill_rect, egui::Rounding::same(10.0), palette::ACCENT_BLUE);
        }

        let label = if state.is_searching { state.search_stage.as_str() } else { "Поиск конфигураций" };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.5),
            egui::Color32::WHITE,
        );

        if resp.clicked() && !state.is_searching {
            self.send(Command::StartSearch);
        }

        ui.add_space(10.0);

        let src_h = 40.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), src_h), egui::Sense::click());
        let bg = if self.show_sources {
            palette::ACCENT_BLUE
        } else if resp.hovered() {
            palette::CARD_HOVER
        } else {
            palette::CARD
        };
        ui.painter().rect_filled(rect, egui::Rounding::same(8.0), bg);
        let count = state.sources.subscriptions.len() + state.sources.custom_links.len();
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("Источники ({count})"),
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        );
        if resp.clicked() {
            self.show_sources = !self.show_sources;
        }

        if let Some(err) = &state.last_error {
            ui.add_space(14.0);
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(58, 24, 24))
                .rounding(8.0)
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(err).color(palette::DANGER).small());
                });
        }
    }

    fn config_list(&mut self, ui: &mut egui::Ui, state: &UiState) {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Конфигурации").color(palette::TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("⟳  Обновить").fill(palette::CARD)).clicked() {
                    self.send(Command::RetestConfigs);
                }
            });
        });
        ui.add_space(8.0);

        if state.configs.is_empty() && !state.is_searching {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Пока нет проверенных серверов").color(palette::TEXT_MUTED));
                ui.label(egui::RichText::new("Нажмите «Поиск конфигураций» слева").color(palette::TEXT_MUTED));
            });
            return;
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, cfg) in state.configs.iter().enumerate() {
                self.config_row(ui, i, cfg);
                ui.add_space(6.0);
            }
        });
    }

    fn config_row(&mut self, ui: &mut egui::Ui, index: usize, cfg: &UiConfig) {
        let row_h = 48.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());

        let bg = if cfg.is_connected {
            palette::CARD_CONNECTED
        } else if cfg.selected {
            palette::CARD_SELECTED
        } else if resp.hovered() {
            palette::CARD_HOVER
        } else {
            palette::CARD
        };
        ui.painter().rect_filled(rect, egui::Rounding::same(8.0), bg);

        let badge_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(8.0, 8.0),
            egui::vec2(44.0, row_h - 16.0),
        );
        draw_country_badge(ui.painter(), badge_rect, &cfg.flag);

        ui.painter().text(
            egui::pos2(badge_rect.max.x + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &cfg.name,
            egui::FontId::proportional(14.0),
            palette::TEXT,
        );

        let stats_x = rect.max.x - 12.0;
        ui.painter().text(
            egui::pos2(stats_x, rect.center().y - 8.0),
            egui::Align2::RIGHT_CENTER,
            &cfg.speed,
            egui::FontId::proportional(12.0),
            palette::ACCENT,
        );
        ui.painter().text(
            egui::pos2(stats_x, rect.center().y + 8.0),
            egui::Align2::RIGHT_CENTER,
            &cfg.ping,
            egui::FontId::proportional(12.0),
            palette::TEXT_MUTED,
        );

        if cfg.is_connected {
            ui.painter().text(
                egui::pos2(stats_x - 90.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                "● подключено",
                egui::FontId::proportional(11.0),
                palette::ACCENT,
            );
        }

        if resp.clicked() {
            self.send(Command::SelectConfig(index));
        }
    }

    fn sources_panel(&mut self, ui: &mut egui::Ui, state: &UiState) {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Источники").color(palette::TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("Готово").fill(palette::ACCENT_BLUE)).clicked() {
                    self.show_sources = false;
                }
            });
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Применяются при следующем «Поиске конфигураций»")
                .color(palette::TEXT_MUTED)
                .small(),
        );
        ui.add_space(14.0);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            section_header(ui, "ПОДПИСКИ", state.sources.subscriptions.len());
            ui.add_space(6.0);

            let mut remove_sub = None;
            let mut toggle_sub = None;
            for (i, sub) in state.sources.subscriptions.iter().enumerate() {
                let mut enabled = sub.enabled;
                let removed_clicked = subscription_card(ui, &sub.name, &sub.url, &mut enabled);
                if enabled != sub.enabled {
                    toggle_sub = Some(i);
                }
                if removed_clicked {
                    remove_sub = Some(i);
                }
                ui.add_space(6.0);
            }
            if let Some(i) = remove_sub {
                self.send(Command::RemoveSubscription(i));
            }
            if let Some(i) = toggle_sub {
                self.send(Command::ToggleSubscription(i));
            }

            ui.add_space(8.0);
            let can_submit_sub = !self.new_sub_url.trim().is_empty();
            let added_sub = add_form(
                ui,
                "Название подписки",
                &mut self.new_sub_name,
                "URL подписки",
                &mut self.new_sub_url,
                "+ Добавить подписку",
                can_submit_sub,
            );
            if added_sub {
                self.send(Command::AddSubscription {
                    name: self.new_sub_name.trim().to_string(),
                    url: self.new_sub_url.trim().to_string(),
                });
                self.new_sub_name.clear();
                self.new_sub_url.clear();
            }

            ui.add_space(20.0);
            section_header(ui, "СВОИ СЕРВЕРЫ", state.sources.custom_links.len());
            ui.add_space(6.0);

            let mut remove_custom = None;
            let mut toggle_custom = None;
            for (i, link) in state.sources.custom_links.iter().enumerate() {
                let mut enabled = link.enabled;
                let removed_clicked = custom_link_card(ui, &link.label, &link.raw, &mut enabled);
                if enabled != link.enabled {
                    toggle_custom = Some(i);
                }
                if removed_clicked {
                    remove_custom = Some(i);
                }
                ui.add_space(6.0);
            }
            if let Some(i) = remove_custom {
                self.send(Command::RemoveCustomLink(i));
            }
            if let Some(i) = toggle_custom {
                self.send(Command::ToggleCustomLink(i));
            }

            ui.add_space(8.0);
            ui.label(egui::RichText::new("vless:// vmess:// trojan:// ss://").color(palette::TEXT_MUTED).small());
            ui.add_space(4.0);
            egui::Frame::none()
                .fill(palette::CARD)
                .rounding(8.0)
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let edit = egui::TextEdit::singleline(&mut self.new_custom_link)
                            .desired_width(ui.available_width() - 90.0)
                            .hint_text("Вставьте ссылку сервера");
                        ui.add(edit);
                        if ui.add(egui::Button::new("+ Добавить").fill(palette::ACCENT_BLUE)).clicked()
                            && !self.new_custom_link.trim().is_empty()
                        {
                            self.send(Command::AddCustomLink(self.new_custom_link.trim().to_string()));
                            self.new_custom_link.clear();
                        }
                    });
                });

            if let Some(err) = &state.last_error {
                ui.add_space(10.0);
                ui.colored_label(palette::DANGER, err);
            }
        });
    }
}

fn section_header(ui: &mut egui::Ui, title: &str, count: usize) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).color(palette::TEXT_MUTED).small().strong());
        ui.label(egui::RichText::new(format!("{count}")).color(palette::TEXT_MUTED).small());
    });
}

fn subscription_card(ui: &mut egui::Ui, name: &str, url: &str, enabled: &mut bool) -> bool {
    let mut remove_clicked = false;
    egui::Frame::none()
        .fill(palette::CARD)
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                toggle_switch(ui, enabled);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(name).color(palette::TEXT));
                    ui.label(egui::RichText::new(truncate(url, 40)).color(palette::TEXT_MUTED).small());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if delete_button(ui) {
                        remove_clicked = true;
                    }
                });
            });
        });
    remove_clicked
}

fn custom_link_card(ui: &mut egui::Ui, label: &str, raw: &str, enabled: &mut bool) -> bool {
    let mut remove_clicked = false;
    let (proto, proto_color) = protocol_tag(raw);

    egui::Frame::none()
        .fill(palette::CARD)
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                toggle_switch(ui, enabled);
                ui.add_space(8.0);

                egui::Frame::none()
                    .fill(proto_color)
                    .rounding(4.0)
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(proto).color(egui::Color32::WHITE).small().strong());
                    });

                ui.add_space(8.0);
                ui.label(egui::RichText::new(truncate(label, 26)).color(palette::TEXT));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if delete_button(ui) {
                        remove_clicked = true;
                    }
                });
            });
        });
    remove_clicked
}

fn protocol_tag(raw: &str) -> (&'static str, egui::Color32) {
    if raw.starts_with("vless://") {
        ("VLESS", egui::Color32::from_rgb(88, 101, 242))
    } else if raw.starts_with("vmess://") {
        ("VMESS", egui::Color32::from_rgb(235, 130, 52))
    } else if raw.starts_with("trojan://") {
        ("TROJAN", egui::Color32::from_rgb(200, 60, 60))
    } else if raw.starts_with("ss://") {
        ("SS", egui::Color32::from_rgb(52, 168, 130))
    } else {
        ("?", palette::TEXT_MUTED)
    }
}

fn delete_button(ui: &mut egui::Ui) -> bool {
    let size = egui::vec2(28.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let bg = if resp.hovered() { egui::Color32::from_rgb(58, 24, 24) } else { egui::Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, egui::Rounding::same(6.0), bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "🗑",
        egui::FontId::proportional(13.0),
        if resp.hovered() { palette::DANGER } else { palette::TEXT_MUTED },
    );
    resp.clicked()
}

/// Классический для egui рецепт pill-переключателя (аналог iOS-тумблера).
fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = egui::vec2(38.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }

    let t = ui.ctx().animate_bool(resp.id, *on);
    let bg = lerp_color(palette::CARD_HOVER, palette::ACCENT, t);
    let radius = rect.height() / 2.0;

    ui.painter().rect_filled(rect, egui::Rounding::same(radius), bg);

    let knob_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    let knob_center = egui::pos2(knob_x, rect.center().y);
    ui.painter().circle_filled(knob_center, radius - 3.0, egui::Color32::WHITE);

    resp
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

fn add_form(
    ui: &mut egui::Ui,
    hint_a: &str,
    val_a: &mut String,
    hint_b: &str,
    val_b: &mut String,
    button_label: &str,
    can_submit: bool,
) -> bool {
    let mut submit = false;
    egui::Frame::none()
        .fill(palette::CARD)
        .rounding(10.0)
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(val_a).hint_text(hint_a).desired_width(f32::INFINITY));
            ui.add_space(6.0);
            ui.add(egui::TextEdit::singleline(val_b).hint_text(hint_b).desired_width(f32::INFINITY));
            ui.add_space(8.0);
            if ui
                .add_sized([ui.available_width(), 32.0], egui::Button::new(button_label).fill(palette::ACCENT_BLUE))
                .clicked()
                && can_submit
            {
                submit = true;
            }
        });
    submit
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Рисует иконку питания вручную — окружность с разрывом сверху и
/// вертикальная чёрточка. Не зависит от того, есть ли нужный глиф в шрифте.
fn draw_power_icon(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: egui::Color32) {
    let stroke = egui::Stroke::new(radius * 0.16, color);

    let start_deg: f32 = -55.0;
    let end_deg: f32 = 235.0;
    let steps = 24;
    let mut prev: Option<egui::Pos2> = None;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let deg = start_deg + (end_deg - start_deg) * t;
        let rad = deg.to_radians();
        let p = center + egui::vec2(rad.sin(), -rad.cos()) * radius;
        if let Some(prev_p) = prev {
            painter.line_segment([prev_p, p], stroke);
        }
        prev = Some(p);
    }

    let top = center + egui::vec2(0.0, -radius * 1.15);
    let mid = center + egui::vec2(0.0, -radius * 0.15);
    painter.line_segment([top, mid], stroke);
}

/// Детерминированный по коду страны цвет, чтобы бейджи визуально
/// различались, но не мигали случайными цветами между запусками.
fn color_for_code(code: &str) -> egui::Color32 {
    let mut hash: u32 = 2166136261;
    for b in code.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    let hue = (hash % 360) as f32 / 360.0;
    hsv_to_rgb(hue, 0.45, 0.55)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> egui::Color32 {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Рисует бейдж страны: цветной прямоугольник со скруглением и кодом
/// страны текстом (например "DE"). Замена флагу-эмодзи, который egui не
/// умеет рендерить как единый глиф (нет шейпинга/лигатур).
fn draw_country_badge(painter: &egui::Painter, rect: egui::Rect, code: &str) {
    let is_unknown = code.trim().is_empty() || code == "—";
    let bg = if is_unknown { palette::CARD_HOVER } else { color_for_code(code) };
    painter.rect_filled(rect, egui::Rounding::same(6.0), bg);

    let label = if is_unknown { "—".to_string() } else { code.to_string() };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        egui::Color32::WHITE,
    );
}
