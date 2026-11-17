use eframe::egui;

const BG: egui::Color32 = egui::Color32::from_rgb(9, 9, 11);
const PANEL: egui::Color32 = egui::Color32::from_rgb(20, 19, 22);
const PANEL_HI: egui::Color32 = egui::Color32::from_rgb(39, 37, 41);
const TEXT: egui::Color32 = egui::Color32::from_rgb(244, 240, 245);
const MUTED: egui::Color32 = egui::Color32::from_rgb(166, 160, 171);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 35, 77);
const MINT: egui::Color32 = egui::Color32::from_rgb(202, 239, 222);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page { Home, Downloads, Active, History, Settings, About }

struct YtDownApp {
    page: Page,
    url: String,
    folder: String,
    quality: usize,
    status: String,
    search: String,
    playing: bool,
}

impl Default for YtDownApp {
    fn default() -> Self {
        Self { page: Page::Home, url: String::new(), folder: "~/Downloads/YT_DOWN".into(), quality: 1,
            status: "Ready when you are".into(), search: String::new(), playing: false }
    }
}

impl YtDownApp {
    fn card<R>(&self, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        egui::Frame::new().fill(PANEL).corner_radius(14).inner_margin(20).show(ui, add).inner
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, label: &str, page: Page, icon: &str) {
        let selected = self.page == page;
        let button = egui::Button::new(egui::RichText::new(format!("{icon}    {label}")).size(14.0)
            .color(if selected { TEXT } else { MUTED }).strong())
            .fill(if selected { PANEL_HI } else { egui::Color32::TRANSPARENT })
            .corner_radius(9).min_size(egui::vec2(0.0, 40.0));
        if ui.add(button).clicked() { self.page = page; }
    }

    fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.label(egui::RichText::new(title).size(34.0).strong().color(TEXT));
        ui.add_space(3.0);
        ui.label(egui::RichText::new(subtitle).size(13.0).color(MUTED));
    }

    fn topbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("‹   ›").size(21.0).color(MUTED));
            ui.add_space(20.0);
            ui.add_sized([ui.available_width() - 120.0, 38.0], egui::TextEdit::singleline(&mut self.search)
                .hint_text("⌕   Search your downloads").margin(egui::vec2(14.0, 9.0)));
            ui.add_space(16.0);
            ui.label(egui::RichText::new("⚙").size(20.0).color(MUTED));
        });
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        Self::heading(ui, "Download", "Your YouTube library, in one place");
        ui.add_space(24.0);
        self.card(ui, |ui| {
            ui.label(egui::RichText::new("Start a download").size(18.0).strong().color(TEXT));
            ui.label(egui::RichText::new("Paste a video or playlist link to get started.").size(13.0).color(MUTED));
            ui.add_space(16.0);
            ui.add_sized([ui.available_width(), 42.0], egui::TextEdit::singleline(&mut self.url)
                .hint_text("https://www.youtube.com/watch?v=...").margin(egui::vec2(12.0, 10.0)));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Quality").color(MUTED));
                egui::ComboBox::from_id_salt("quality").selected_text(["480p (SD)", "720p (HD)", "1080p (Full HD)", "Best available", "Audio only (MP3)"][self.quality])
                    .show_ui(ui, |ui| { for (i, v) in ["480p (SD)", "720p (HD)", "1080p (Full HD)", "Best available", "Audio only (MP3)"].iter().enumerate() { ui.selectable_value(&mut self.quality, i, *v); }});
                ui.add_space(16.0);
                ui.label(egui::RichText::new("Save to").color(MUTED));
                ui.add_sized([ui.available_width() - 6.0, 30.0], egui::TextEdit::singleline(&mut self.folder));
            });
            ui.add_space(18.0);
            let download = egui::Button::new(egui::RichText::new("↓   Start download").strong().color(TEXT))
                .fill(ACCENT).corner_radius(9).min_size(egui::vec2(170.0, 42.0));
            if ui.add(download).clicked() {
                self.status = if self.url.trim().is_empty() { "Paste a YouTube link first".into() } else { "Download engine is ready to connect".into() };
                if !self.url.trim().is_empty() { self.page = Page::Active; }
            }
        });
        ui.add_space(17.0);
        ui.horizontal(|ui| {
            self.stat_card(ui, "LIBRARY", "0", "downloads saved");
            self.stat_card(ui, "IN PROGRESS", "0", "active downloads");
            self.stat_card(ui, "THIS WEEK", "-", "items downloaded");
        });
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Quick access").size(18.0).strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(egui::RichText::new("View all  →").color(MINT)); });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            self.quick_card(ui, "♫", "Audio downloads", "MP3 files from videos");
            self.quick_card(ui, "▣", "Video downloads", "Your saved videos");
            self.quick_card(ui, "▤", "Playlists", "Browse playlist downloads");
        });
    }

    fn stat_card(&self, ui: &mut egui::Ui, title: &str, value: &str, hint: &str) {
        self.card(ui, |ui| {
            ui.set_min_width((ui.available_width() - 22.0) / 3.0);
            ui.label(egui::RichText::new(title).size(10.0).strong().color(MUTED));
            ui.add_space(7.0);
            ui.label(egui::RichText::new(value).size(25.0).strong().color(TEXT));
            ui.label(egui::RichText::new(hint).size(12.0).color(MUTED));
        });
    }

    fn quick_card(&self, ui: &mut egui::Ui, icon: &str, title: &str, hint: &str) {
        self.card(ui, |ui| {
            ui.set_min_width((ui.available_width() - 20.0) / 3.0);
            ui.label(egui::RichText::new(icon).size(22.0).color(MINT));
            ui.add_space(10.0);
            ui.label(egui::RichText::new(title).strong().color(TEXT));
            ui.label(egui::RichText::new(hint).size(11.0).color(MUTED));
        });
    }

    fn generic_page(&self, ui: &mut egui::Ui) {
        let (title, subtitle, empty, icon) = match self.page {
            Page::Downloads => ("My downloads", "Everything you have saved", "Your library is empty for now.", "♫"),
            Page::Active => ("Active downloads", "Keep an eye on what is in progress", "No active downloads", "↓"),
            Page::History => ("History", "Pick up where you left off", "Your download history will appear here.", "◷"),
            Page::Settings => ("Settings", "Make YT_DOWN work the way you like", "Preferences", "⚙"),
            _ => ("About YT_DOWN", "Simple downloads, neatly managed", "Version 1.0.0  ·  Rust desktop edition", "ⓘ"),
        };
        Self::heading(ui, title, subtitle);
        ui.add_space(24.0);
        self.card(ui, |ui| {
            ui.set_min_height(170.0);
            ui.vertical_centered(|ui| {
                ui.add_space(23.0);
                ui.label(egui::RichText::new(icon).size(32.0).color(MINT));
                ui.add_space(9.0);
                ui.label(egui::RichText::new(empty).size(17.0).strong().color(TEXT));
                ui.label(egui::RichText::new("Add a link from Download to see it here.").size(13.0).color(MUTED));
            });
        });
    }
}

impl eframe::App for YtDownApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        let mut visuals = ctx.style().visuals.clone();
        visuals.window_fill = BG;
        visuals.panel_fill = BG;
        visuals.extreme_bg_color = egui::Color32::from_rgb(28, 27, 30);
        visuals.widgets.inactive.bg_fill = PANEL_HI;
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(52, 48, 53);
        visuals.selection.bg_fill = ACCENT;
        ctx.set_visuals(visuals);

        egui::TopBottomPanel::top("title").exact_height(34.0).frame(egui::Frame::new().fill(egui::Color32::from_rgb(24, 24, 27))).show(ctx, |ui| {
            ui.vertical_centered(|ui| { ui.label(egui::RichText::new("YT_DOWN").size(12.0).strong().color(TEXT)); });
        });
        egui::SidePanel::left("sidebar").resizable(false).exact_width(218.0).frame(egui::Frame::new().fill(egui::Color32::from_rgb(16, 15, 18)).inner_margin(14)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::Frame::new().fill(MINT).corner_radius(20).inner_margin(7).show(ui, |ui| { ui.label(egui::RichText::new("♫").size(16.0).strong().color(BG)); });
                ui.add_space(5.0);
                ui.label(egui::RichText::new("YT_").size(19.0).strong().color(TEXT).text_style(egui::TextStyle::Heading));
                ui.label(egui::RichText::new("DOWN").size(19.0).strong().color(MUTED).text_style(egui::TextStyle::Heading));
            });
            ui.add_space(26.0);
            self.nav_button(ui, "Download", Page::Home, "⌂");
            self.nav_button(ui, "My downloads", Page::Downloads, "♫");
            self.nav_button(ui, "Active downloads", Page::Active, "↧");
            self.nav_button(ui, "History", Page::History, "◷");
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(egui::RichText::new("YOUR LIBRARY").size(10.0).strong().color(MUTED));
            ui.add_space(8.0);
            ui.horizontal(|ui| { ui.label("♫"); ui.label(egui::RichText::new("Audio").color(MUTED)); });
            ui.add_space(7.0);
            ui.horizontal(|ui| { ui.label("▣"); ui.label(egui::RichText::new("Video").color(MUTED)); });
            ui.add_space(7.0);
            ui.horizontal(|ui| { ui.label("▤"); ui.label(egui::RichText::new("Playlists").color(MUTED)); });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(5.0);
                self.nav_button(ui, "About", Page::About, "ⓘ");
                self.nav_button(ui, "Settings", Page::Settings, "⚙");
            });
        });

        egui::TopBottomPanel::bottom("player").exact_height(75.0).frame(egui::Frame::new().fill(egui::Color32::from_rgb(15, 14, 17)).inner_margin(egui::Margin::symmetric(18, 10))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::Frame::new().fill(PANEL_HI).corner_radius(7).inner_margin(10).show(ui, |ui| { ui.label(egui::RichText::new("♫").size(18.0).color(MINT)); });
                ui.vertical(|ui| { ui.label(egui::RichText::new("Ready to download").size(13.0).strong().color(TEXT)); ui.label(egui::RichText::new(&self.status).size(11.0).color(MUTED)); });
                ui.add_space(24.0);
                if ui.button(if self.playing { "Ⅱ" } else { "▶" }).clicked() { self.playing = !self.playing; }
                ui.label(egui::RichText::new("•  •  •").color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("YT_DOWN  ·  v1.0.0").size(11.0).color(MUTED));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("◖))").color(MUTED));
                });
            });
        });

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 20, right: 20, top: 12, bottom: 18 })).show(ctx, |ui| {
            self.topbar(ui);
            ui.add_space(18.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.page { Page::Home => self.home(ui), _ => self.generic_page(ui) }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default()
        .with_title("YT_DOWN").with_inner_size([1200.0, 800.0]).with_min_inner_size([900.0, 620.0]), ..Default::default() };
    eframe::run_native("YT_DOWN", options, Box::new(|_cc| Ok(Box::new(YtDownApp::default()))))
}
