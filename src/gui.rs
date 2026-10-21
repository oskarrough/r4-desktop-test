//! The window shown when the app opens without a URL: R4 channels, a channel's tracks, and what's playing.
//! Small first: one view at a time with a transport along the bottom. With room, channels and tracks sit side by side.
use crate::channels::{self, Channel};
use crate::player::{self, Command, Player, State};
use crate::tracks::{self, Track};
use egui::{Key, Modifiers, PointerButton, Rect, RichText, Sense, vec2};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// From this width channels and tracks sit side by side.
const WIDE: f32 = 560.0;
/// Height of a list row, an easy target in a small window.
const ROW: f32 = 30.0;

fastframe_icons::icons! {
    /// Every icon the window draws.
    pub enum Icon {
        prefix: "radio4000-icon-",
        directory: "../assets/icons/",
        Back => lucide "arrow-left",
        Play => "play-filled",
        Pause => "pause-filled",
        Prev => "skip-back-filled",
        Next => "skip-forward-filled",
    }
}

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([280.0, 420.0])
            .with_min_inner_size([180.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Radio4000",
        options,
        Box::new(|cc| {
            let mut fonts = fastframe_fonts::FontSetup::default().definitions();
            let rendering = fastframe_text::detect();
            rendering.apply_to(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx
                .all_styles_mut(|style| rendering.apply_to_visuals(&mut style.visuals));
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fastframe_icons::install::<Icon>(&cc.egui_ctx);

            let (tx, rx) = mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(channels::fetch());
                ctx.request_repaint();
            });
            let ctx = cc.egui_ctx.clone();
            Ok(Box::new(App {
                rx,
                channels: None,
                tracks: HashMap::new(),
                open: None,
                now: false,
                player: Player::spawn(&[], move || ctx.request_repaint()),
                scrub: None,
                reveal: false,
                actions: Vec::new(),
            }))
        }),
    )
}

struct App {
    rx: mpsc::Receiver<Result<Vec<Channel>, String>>,
    channels: Option<Result<Vec<Channel>, String>>,
    /// Every channel opened so far, by slug, so going back to one is instant.
    tracks: HashMap<String, Tracks>,
    /// The channel being looked at.
    open: Option<Open>,
    /// The now-playing view covers everything else.
    now: bool,
    player: Player,
    scrub: Option<Scrub>,
    /// Scroll the playing track into view on the next frame.
    reveal: bool,
    actions: Vec<Action>,
}

struct Open {
    slug: String,
    name: String,
}

struct Tracks {
    rx: mpsc::Receiver<Result<Vec<Track>, String>>,
    list: Option<Result<Vec<Track>, String>>,
}

/// Where the seek bar points while it is dragged, and after, until mpv has caught up.
struct Scrub {
    to: f64,
    dragging: bool,
    at: Instant,
}

/// What the views asked for this frame, done once they are drawn.
enum Action {
    Open(String, String),
    Back,
    ShowNow,
    /// Play this track of the open channel.
    Play(usize),
    Toggle,
    Send(Command),
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive();
        self.keys(ui.ctx());
        let wide = ui.ctx().content_rect().width() >= WIDE;
        let state = &self.player.state;
        let playing = state.queue.as_deref();
        let actions = &mut self.actions;

        if !self.now {
            egui::Panel::bottom("transport").show(ui, |ui| {
                transport(ui, state, &mut self.scrub, actions);
            });
        }
        if self.now {
            egui::CentralPanel::default().show(ui, |ui| {
                now_playing(ui, state, &mut self.scrub, actions);
            });
        } else if wide {
            let open = self.open.as_ref().map(|o| o.slug.as_str());
            egui::Panel::left("channels")
                .resizable(true)
                .default_size(240.0)
                .show(ui, |ui| {
                    // Truncated rows ask for no width, and the panel would shrink to fit them.
                    ui.set_min_width(ui.available_width());
                    channel_list(ui, &self.channels, open, playing, actions);
                });
            egui::CentralPanel::default().show(ui, |ui| match &self.open {
                Some(o) => tracklist(ui, o, &self.tracks, state, false, &mut self.reveal, actions),
                None => {
                    ui.weak("Pick a channel.");
                }
            });
        } else {
            egui::CentralPanel::default().show(ui, |ui| match &self.open {
                Some(o) => tracklist(ui, o, &self.tracks, state, true, &mut self.reveal, actions),
                None => channel_list(ui, &self.channels, None, playing, actions),
            });
        }

        for a in std::mem::take(&mut self.actions) {
            self.act(ui.ctx(), a);
        }
    }
}

impl App {
    fn receive(&mut self) {
        if let Ok(result) = self.rx.try_recv() {
            self.channels = Some(result);
        }
        for t in self.tracks.values_mut() {
            if let Ok(result) = t.rx.try_recv() {
                t.list = Some(result);
            }
        }
        let before = (self.player.state.queue.clone(), self.player.state.index);
        self.player.update();
        let s = &self.player.state;
        if s.index.is_some() && (s.queue.clone(), s.index) != before {
            self.reveal = true;
        }
        // Let go of a finished scrub once mpv reports the new spot, or gives up on it.
        if let Some(sc) = &self.scrub
            && !sc.dragging
            && ((s.pos - sc.to).abs() < 1.5 || sc.at.elapsed() > Duration::from_secs(2))
        {
            self.scrub = None;
        }
    }

    fn keys(&mut self, ctx: &egui::Context) {
        ctx.input_mut(|i| {
            let mut key = |k, a| {
                if i.consume_key(Modifiers::NONE, k) {
                    self.actions.push(a);
                }
            };
            key(Key::Space, Action::Toggle);
            key(Key::N, Action::Send(Command::Next));
            key(Key::P, Action::Send(Command::Prev));
            key(Key::ArrowLeft, Action::Send(Command::SeekBy(-10.0)));
            key(Key::ArrowRight, Action::Send(Command::SeekBy(10.0)));
            key(Key::Escape, Action::Back);
            key(Key::Backspace, Action::Back);
            if i.pointer.button_pressed(PointerButton::Extra1) {
                self.actions.push(Action::Back);
            }
        });
    }

    fn act(&mut self, ctx: &egui::Context, a: Action) {
        match a {
            Action::Open(slug, name) => {
                self.tracks
                    .entry(slug.clone())
                    .or_insert_with(|| Tracks::fetch(ctx, &slug));
                self.reveal = self.player.state.queue.as_deref() == Some(&slug);
                self.open = Some(Open { slug, name });
                self.now = false;
            }
            Action::Back => {
                if self.now {
                    self.now = false;
                } else {
                    self.open = None;
                }
            }
            Action::ShowNow => self.now = self.player.state.queue.is_some(),
            Action::Play(index) => self.play(index),
            // Space with nothing loaded starts the open channel from the top.
            Action::Toggle if self.player.state.queue.is_none() => self.play(0),
            Action::Toggle => self.player.send(Command::Toggle),
            Action::Send(c) => self.player.send(c),
        }
    }

    fn play(&mut self, index: usize) {
        let Some(open) = &self.open else { return };
        let Some(Ok(list)) = self.tracks.get(&open.slug).and_then(|t| t.list.as_ref()) else {
            return;
        };
        if index >= list.len() {
            return;
        }
        self.player.send(Command::Play {
            queue: open.slug.clone(),
            channel: open.name.clone(),
            tracks: list
                .iter()
                .map(|t| player::Track {
                    url: t.url.clone(),
                    title: title(t).to_string(),
                })
                .collect(),
            index,
        });
    }
}

impl Tracks {
    fn fetch(ctx: &egui::Context, slug: &str) -> Self {
        let (tx, rx) = mpsc::channel();
        let (ctx, slug) = (ctx.clone(), slug.to_string());
        std::thread::spawn(move || {
            let _ = tx.send(tracks::fetch(&slug));
            ctx.request_repaint();
        });
        Tracks { rx, list: None }
    }
}

fn title(t: &Track) -> &str {
    t.title.as_deref().unwrap_or(&t.url)
}

/// The bar along the bottom: a thin seek bar, then the buttons and what's on. Clicking the title opens the now-playing view.
fn transport(ui: &mut egui::Ui, s: &State, scrub: &mut Option<Scrub>, actions: &mut Vec<Action>) {
    if let Some(e) = &s.error {
        ui.label(format!("Can't play: {e}"));
        return;
    }
    seek_bar(ui, s, scrub, actions);
    ui.horizontal(|ui| {
        buttons(ui, s, 18.0, actions);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (top, bottom) = if s.queue.is_some() {
                let time = s.dur.map_or_else(String::new, |d| {
                    format!(" · {} / {}", clock(shown(s, scrub)), clock(d))
                });
                (
                    RichText::new(&s.title).strong(),
                    format!("{}{time}", s.channel),
                )
            } else {
                (RichText::new("Nothing playing").weak(), String::new())
            };
            let a = ui.add(egui::Label::new(top).truncate().sense(Sense::click()));
            let b = ui.add(
                egui::Label::new(RichText::new(bottom).weak().small())
                    .truncate()
                    .sense(Sense::click()),
            );
            if a.clicked() || b.clicked() {
                actions.push(Action::ShowNow);
            }
        });
    });
}

/// The now-playing view: big title and buttons, nothing else.
fn now_playing(ui: &mut egui::Ui, s: &State, scrub: &mut Option<Scrub>, actions: &mut Vec<Action>) {
    if back_button(ui).clicked() {
        actions.push(Action::Back);
    }
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 90.0).max(8.0));
        ui.add(egui::Label::new(RichText::new(&s.title).heading()).wrap());
        if let Some(slug) = &s.queue
            && ui.link(RichText::new(&s.channel).weak()).clicked()
        {
            actions.push(Action::Open(slug.clone(), s.channel.clone()));
        }
        ui.add_space(12.0);
        seek_bar(ui, s, scrub, actions);
        ui.horizontal(|ui| {
            ui.weak(clock(shown(s, scrub)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(s.dur.map_or_else(|| "-:--".into(), clock));
            });
        });
        ui.add_space(8.0);
        // Centre the row of buttons by hand; a horizontal layout always starts at the left.
        let size = 30.0;
        let width = 3.0 * (size + 12.0) + 2.0 * ui.spacing().item_spacing.x;
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            buttons(ui, s, size, actions);
        });
    });
}

/// Previous, play/pause and next. Play shows a spinner while the track resolves.
fn buttons(ui: &mut egui::Ui, s: &State, size: f32, actions: &mut Vec<Action>) {
    let loaded = s.queue.is_some();
    if ui
        .add_enabled(loaded, icon_button(ui, Icon::Prev, size))
        .on_hover_text("Previous (p)")
        .clicked()
    {
        actions.push(Action::Send(Command::Prev));
    }
    if s.loading() {
        let edge = size + 12.0;
        ui.allocate_ui(vec2(edge, edge), |ui| {
            ui.centered_and_justified(|ui| ui.add(egui::Spinner::new().size(size)));
        });
    } else {
        let (icon, tip) = if loaded && !s.paused {
            (Icon::Pause, "Pause (space)")
        } else {
            (Icon::Play, "Play (space)")
        };
        if ui
            .add(icon_button(ui, icon, size))
            .on_hover_text(tip)
            .clicked()
        {
            actions.push(Action::Toggle);
        }
    }
    if ui
        .add_enabled(loaded, icon_button(ui, Icon::Next, size))
        .on_hover_text("Next (n)")
        .clicked()
    {
        actions.push(Action::Send(Command::Next));
    }
}

fn icon_button(ui: &egui::Ui, icon: Icon, size: f32) -> egui::Button<'static> {
    // Button::new, unlike Button::image, keeps the icon at the size asked for.
    egui::Button::new(icon.image(ui.visuals().text_color(), size))
        .frame_when_inactive(false)
        .min_size(vec2(size + 12.0, size + 12.0))
}

fn back_button(ui: &mut egui::Ui) -> egui::Response {
    ui.add(icon_button(ui, Icon::Back, 18.0))
        .on_hover_text("Back (esc)")
}

/// Where the track is, or where it is being dragged to.
fn shown(s: &State, scrub: &Option<Scrub>) -> f64 {
    scrub.as_ref().map_or(s.pos, |sc| sc.to)
}

/// A thin bar that thickens under the pointer. Seeks once it is let go, not on every step of a drag.
fn seek_bar(ui: &mut egui::Ui, s: &State, scrub: &mut Option<Scrub>, actions: &mut Vec<Action>) {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::click_and_drag());
    let dur = s.dur.unwrap_or(0.0);
    if dur > 0.0 {
        if let Some(p) = resp.interact_pointer_pos() {
            let frac = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
            *scrub = Some(Scrub {
                to: frac * dur,
                dragging: true,
                at: Instant::now(),
            });
        }
        if (resp.drag_stopped() || resp.clicked())
            && let Some(sc) = scrub
        {
            sc.dragging = false;
            actions.push(Action::Send(Command::Seek(sc.to)));
        }
    }
    let frac = if dur > 0.0 {
        (shown(s, scrub) / dur).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let thick = if resp.hovered() || resp.dragged() {
        6.0
    } else {
        3.0
    };
    let rail = Rect::from_center_size(rect.center(), vec2(rect.width(), thick));
    let mut done = rail;
    done.set_right(rail.left() + rail.width() * frac);
    let v = ui.visuals();
    ui.painter()
        .rect_filled(rail, thick / 2.0, v.widgets.inactive.bg_fill);
    ui.painter()
        .rect_filled(done, thick / 2.0, v.selection.bg_fill);
    if dur > 0.0 {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
}

fn channel_list(
    ui: &mut egui::Ui,
    channels: &Option<Result<Vec<Channel>, String>>,
    open: Option<&str>,
    playing: Option<&str>,
    actions: &mut Vec<Action>,
) {
    let list = match channels {
        None => {
            ui.spinner();
            return;
        }
        Some(Err(e)) => {
            ui.label(format!("Could not load channels: {e}"));
            return;
        }
        Some(Ok(list)) => list,
    };
    rows(ui, list.len(), None, |ui, i| {
        let c = &list[i];
        // The playing channel stands out in bold; right_text keeps the name at the left.
        let mut name = RichText::new(&c.name);
        if playing == Some(&c.slug) {
            name = name.strong();
        }
        let row = egui::Button::selectable(open == Some(&c.slug), name).right_text("");
        if ui.add(row).clicked() {
            actions.push(Action::Open(c.slug.clone(), c.name.clone()));
        }
    });
}

fn tracklist(
    ui: &mut egui::Ui,
    open: &Open,
    tracks: &HashMap<String, Tracks>,
    s: &State,
    back: bool,
    reveal: &mut bool,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        if back && back_button(ui).clicked() {
            actions.push(Action::Back);
        }
        ui.add(egui::Label::new(RichText::new(&open.name).strong()).truncate());
    });
    let list = match tracks.get(&open.slug).and_then(|t| t.list.as_ref()) {
        None => {
            ui.spinner();
            return;
        }
        Some(Err(e)) => {
            ui.label(format!("Could not load tracks: {e}"));
            return;
        }
        Some(Ok(list)) => list,
    };
    let current = s.index.filter(|_| s.queue.as_deref() == Some(&open.slug));
    let target = current.filter(|_| std::mem::take(reveal));
    rows(ui, list.len(), target, |ui, i| {
        let row = egui::Button::selectable(current == Some(i), title(&list[i])).right_text("");
        if ui.add(row).clicked() {
            actions.push(Action::Play(i));
        }
    });
}

/// A list of `n` tall rows that only draws the visible ones, scrolled to centre `reveal` when given.
fn rows(
    ui: &mut egui::Ui,
    n: usize,
    reveal: Option<usize>,
    mut row: impl FnMut(&mut egui::Ui, usize),
) {
    let stride = ROW + ui.spacing().item_spacing.y;
    let mut area = egui::ScrollArea::vertical().auto_shrink(false);
    if let Some(i) = reveal {
        let top = i as f32 * stride - (ui.available_height() - stride) / 2.0;
        area = area.vertical_scroll_offset(top.max(0.0));
    }
    area.show_rows(ui, ROW, n, |ui, range| {
        for i in range {
            ui.scope(|ui| {
                ui.spacing_mut().interact_size.y = ROW;
                ui.spacing_mut().button_padding.y =
                    (ROW - ui.text_style_height(&egui::TextStyle::Button)) / 2.0;
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.set_width(ui.available_width());
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    row(ui, i)
                });
            });
        }
    });
}

/// Seconds as m:ss, or h:mm:ss for long mixes.
fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn clock_reads_like_a_player() {
        assert_eq!(super::clock(65.9), "1:05");
        assert_eq!(super::clock(3725.0), "1:02:05");
    }
}
