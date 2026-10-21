//! The window shown when the app opens without a URL: R4 channels, a channel's tracks, and what's playing.
//! Small first: one view at a time with a transport along the bottom. With room, channels and tracks sit side by side.
use crate::channels::{self, Channel};
use crate::player::{self, Command, Player, State};
use crate::style::*;
use crate::tracks::{self, Track};
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, FontId, Galley, Key, Modifiers, PointerButton, Pos2, Rect, Response, Sense, Ui, pos2,
    vec2,
};
use fastframe_fonts::Weight;
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// From this width channels and tracks sit side by side.
const WIDE: f32 = 560.0;
/// A list row.
const ROW: f32 = 8.0 * UNIT;
/// The bar along the top of a view: back, title, what's playing.
const HEADER: f32 = 10.0 * UNIT;
/// The transport along the bottom, its progress line included.
const BAR: f32 = 14.0 * UNIT;
/// How long the volume meter stays up after a change, and how long it then takes to fade.
const METER: Duration = Duration::from_millis(1200);
const FADE: Duration = Duration::from_millis(300);
/// Volume per arrow press; a notch of the wheel moves about as much.
const STEP: f64 = 5.0;
/// The progress line on the transport's top edge, thicker under the pointer.
const LINE: f32 = 2.0;
/// Transport icons, and the square each one answers clicks in.
const ICON: f32 = 16.0;
const HIT: f32 = 8.0 * UNIT;

fastframe_icons::icons! {
    /// Every icon the window draws.
    pub enum Icon {
        prefix: "radio4000-icon-",
        directory: "../assets/icons/",
        Play => "play-filled",
        Pause => "pause-filled",
        Prev => "skip-back-filled",
        Next => "skip-forward-filled",
        Speaker => lucide "volume-2",
        Muted => lucide "volume-x",
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
            crate::style::install(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fastframe_icons::install::<Icon>(&cc.egui_ctx);

            let (tx, rx) = mpsc::channel();
            let demo = crate::demo::from_env();
            if demo.is_none() {
                let ctx = cc.egui_ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(channels::fetch());
                    ctx.request_repaint();
                });
            }
            let ctx = cc.egui_ctx.clone();
            let mut app = App {
                rx,
                channels: None,
                tracks: HashMap::new(),
                open: None,
                now: false,
                player: Player::spawn(&[], move || ctx.request_repaint()),
                scrub: None,
                volume: None,
                reveal: false,
                actions: Vec::new(),
                demo: demo.is_some(),
            };
            if let Some(d) = demo {
                let (_, rx) = mpsc::channel();
                let open = Open {
                    slug: "oskar".into(),
                    name: "Radio Oskar".into(),
                };
                app.tracks.insert(
                    open.slug.clone(),
                    Tracks {
                        rx,
                        list: Some(Ok(d.tracks)),
                    },
                );
                app.channels = Some(Ok(d.channels));
                app.player.state = d.state;
                app.open = d.open.then_some(open);
                app.now = d.now;
                app.reveal = true;
                if d.volume {
                    app.volume = Some(Volume {
                        to: app.player.state.volume,
                        at: Instant::now() + Duration::from_secs(3600),
                    });
                }
            }
            Ok(Box::new(app))
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
    /// The volume last asked for, shown on the meter until it fades.
    volume: Option<Volume>,
    /// Scroll the playing track into view on the next frame.
    reveal: bool,
    actions: Vec<Action>,
    /// Showing the demo: the player's state stays as the demo set it.
    demo: bool,
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

/// A change of volume, ahead of mpv reporting it.
#[derive(Clone, Copy)]
struct Volume {
    to: f64,
    at: Instant,
}

impl Volume {
    /// How visible the meter is: fully for `METER`, then fading out over `FADE`.
    fn opacity(&self) -> f32 {
        let t = self.at.elapsed().saturating_sub(METER);
        1.0 - (t.as_secs_f32() / FADE.as_secs_f32()).min(1.0)
    }
}

/// What the views asked for this frame, done once they are drawn.
enum Action {
    Open(String, String),
    Back,
    ShowNow,
    /// Play this track of the open channel.
    Play(usize),
    Toggle,
    /// Up or down by this much.
    Volume(f64),
    ToggleMute,
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

        let flat = egui::Frame::NONE.fill(BG);
        if !self.now {
            egui::Panel::bottom("transport")
                .frame(flat)
                .exact_size(BAR)
                .show_separator_line(false)
                .show(ui, |ui| transport(ui, state, &mut self.scrub, actions));
        }
        if self.now {
            egui::CentralPanel::no_frame().frame(flat).show(ui, |ui| {
                now_playing(ui, state, &mut self.scrub, self.volume, actions);
            });
        } else if wide {
            let open = self.open.as_ref().map(|o| o.slug.as_str());
            egui::Panel::left("channels")
                .frame(flat)
                .resizable(true)
                .default_size(240.0)
                .show(ui, |ui| {
                    header(ui, "Radio4000", false, None, actions);
                    channel_list(ui, &self.channels, open, playing, false, actions);
                });
            egui::CentralPanel::no_frame()
                .frame(flat)
                .show(ui, |ui| match &self.open {
                    Some(o) => {
                        tracklist(ui, o, &self.tracks, state, false, &mut self.reveal, actions)
                    }
                    None => {
                        header(ui, "", false, Some(state), actions);
                        note(ui, "Pick a channel.");
                    }
                });
        } else {
            egui::CentralPanel::no_frame()
                .frame(flat)
                .show(ui, |ui| match &self.open {
                    Some(o) => {
                        tracklist(ui, o, &self.tracks, state, true, &mut self.reveal, actions)
                    }
                    None => {
                        header(ui, "Radio4000", false, Some(state), actions);
                        channel_list(ui, &self.channels, None, playing, true, actions);
                    }
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
        if !self.demo {
            self.player.update();
        }
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
        if let Some(v) = &self.volume
            && v.opacity() <= 0.0
        {
            self.volume = None;
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
            key(Key::M, Action::ToggleMute);
            // Up and down are the iPod's wheel: volume, while the now-playing view is up.
            if self.now {
                key(Key::ArrowUp, Action::Volume(STEP));
                key(Key::ArrowDown, Action::Volume(-STEP));
            }
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
            Action::Volume(by) => {
                let from = self.volume.map_or(self.player.state.volume, |v| v.to);
                let to = (from + by).clamp(0.0, 100.0);
                self.player.send(Command::Volume(to));
                self.volume = Some(Volume {
                    to,
                    at: Instant::now(),
                });
                if self.demo {
                    self.player.state.volume = to;
                    self.player.state.muted = false;
                }
            }
            Action::ToggleMute => {
                self.player.send(Command::ToggleMute);
                if self.demo {
                    self.player.state.muted = !self.player.state.muted;
                }
            }
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

/// The bar along the bottom: the progress line on its top edge, what's on at the left, play and next at the right.
/// Clicking what's on opens the now-playing view.
fn transport(ui: &mut Ui, s: &State, scrub: &mut Option<Scrub>, actions: &mut Vec<Action>) {
    let rect = ui.max_rect();
    let hit = Rect::from_min_size(rect.min, vec2(rect.width(), 2.0 * UNIT));
    seek(ui, hit, true, LINE, s, scrub, actions);

    let cy = rect.top() + LINE + (BAR - LINE) / 2.0;
    let loaded = s.queue.is_some();
    let next = pos2(rect.right() - MARGIN - half(Icon::Next, ICON), cy);
    let play = next - vec2(HIT, 0.0);
    if icon_button(ui, Icon::Next, next, ICON, loaded).clicked() {
        actions.push(Action::Send(Command::Next));
    }
    play_button(ui, s, play, ICON, actions);

    let x = rect.left() + MARGIN;
    let width = play.x - HIT / 2.0 - x;
    let lines = if let Some(e) = &s.error {
        vec![(
            format!("Can't play: {e}"),
            font(SMALL, Weight::Regular),
            SECONDARY,
        )]
    } else if loaded {
        let time = s.dur.map_or_else(String::new, |d| {
            format!(" · {} / {}", clock(shown(s, scrub)), clock(d))
        });
        vec![
            (s.title.clone(), font(BODY, Weight::SemiBold), TEXT),
            (
                format!("{}{time}", s.channel),
                font(SMALL, Weight::Regular),
                SECONDARY,
            ),
        ]
    } else {
        vec![(
            "Nothing playing".into(),
            font(BODY, Weight::Regular),
            SECONDARY,
        )]
    };
    let gap = 2.0 * UNIT;
    let mut y = cy - gap * (lines.len() - 1) as f32;
    let mut area = Rect::NOTHING;
    for (text, f, color) in lines {
        area = area.union(put(ui, fit(ui, &text, f, color, width), x, y));
        y += 2.0 * gap;
    }
    let area = Rect::from_x_y_ranges(x..=x + width, rect.top() + LINE..=rect.bottom());
    if loaded
        && ui
            .interact(area, ui.id().with("now"), Sense::click())
            .clicked()
    {
        actions.push(Action::ShowNow);
    }
}

/// The now-playing view, laid out like Apple's: title and channel at the margin, the seek bar with its times,
/// then previous, play and next, the whole group centred in the window.
fn now_playing(
    ui: &mut Ui,
    s: &State,
    scrub: &mut Option<Scrub>,
    volume: Option<Volume>,
    actions: &mut Vec<Action>,
) {
    let top = header(ui, "Now playing", true, None, actions);
    speaker(ui, top, s, volume, actions);
    let area = ui.available_rect_before_wrap();
    let x = area.left() + MARGIN;
    let width = area.width() - 2.0 * MARGIN;

    let mut job = LayoutJob::simple(s.title.clone(), font(LARGE, Weight::SemiBold), TEXT, width);
    job.wrap.max_rows = 2;
    let title = ui.painter().layout_job(job);
    let channel = fit(ui, &s.channel, font(BODY, Weight::Regular), ACCENT, width);
    let big = 2.0 * ICON;
    // Each step below is a gap on the grid, from the title's top to the buttons' bottom.
    let height =
        title.size().y + UNIT + channel.size().y + 6.0 * UNIT + 8.0 * UNIT + 8.0 * UNIT + big;
    let mut y = (area.top() + (area.height() - height) / 2.0).max(area.top() + 2.0 * UNIT);
    y = (y / UNIT).round() * UNIT;

    put_top(ui, title.clone(), x, y);
    y += title.size().y + UNIT;
    let r = put_top(ui, channel.clone(), x, y);
    if let Some(slug) = &s.queue {
        let r = ui.interact(r, ui.id().with("channel"), Sense::click());
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            actions.push(Action::Open(slug.clone(), s.channel.clone()));
        }
    }
    y += channel.size().y + 6.0 * UNIT;

    let hit = Rect::from_min_size(pos2(x, y), vec2(width, 4.0 * UNIT));
    seek(ui, hit, false, 2.0 * LINE, s, scrub, actions);
    y += 4.0 * UNIT + 4.0 * UNIT;
    let small = font(SMALL, Weight::Regular);
    put(
        ui,
        fit(ui, &clock(shown(s, scrub)), small.clone(), SECONDARY, width),
        x,
        y,
    );
    let left = s.dur.map_or_else(
        || "-:--".into(),
        |d| format!("−{}", clock(d - shown(s, scrub))),
    );
    let g = fit(ui, &left, small, SECONDARY, width);
    put(ui, g.clone(), x + width - g.size().x, y);
    y += 8.0 * UNIT + big / 2.0;

    let loaded = s.queue.is_some();
    let step = (width / 3.0).min(24.0 * UNIT);
    let mid = pos2(area.center().x, y);
    let big_hit = |ui: &mut Ui, icon, c, enabled| {
        icon_button_sized(ui, icon, c, big * 0.75, 12.0 * UNIT, enabled)
    };
    if big_hit(ui, Icon::Prev, mid - vec2(step, 0.0), loaded).clicked() {
        actions.push(Action::Send(Command::Prev));
    }
    play_button_sized(ui, s, mid, big, 12.0 * UNIT, actions);
    if big_hit(ui, Icon::Next, mid + vec2(step, 0.0), loaded).clicked() {
        actions.push(Action::Send(Command::Next));
    }
}

/// Where the track is, or where it is being dragged to.
fn shown(s: &State, scrub: &Option<Scrub>) -> f64 {
    scrub.as_ref().map_or(s.pos, |sc| sc.to)
}

fn channel_list(
    ui: &mut Ui,
    channels: &Option<Result<Vec<Channel>, String>>,
    open: Option<&str>,
    playing: Option<&str>,
    opens: bool,
    actions: &mut Vec<Action>,
) {
    let list = match channels {
        None => return note(ui, "Loading…"),
        Some(Err(e)) => return note(ui, &format!("Could not load channels: {e}")),
        Some(Ok(list)) => list,
    };
    rows(ui, list.len(), None, |ui, i| {
        let c = &list[i];
        let selected = open == Some(c.slug.as_str());
        if row(ui, &c.name, selected, playing == Some(&c.slug), opens).clicked() {
            actions.push(Action::Open(c.slug.clone(), c.name.clone()));
        }
    });
}

fn tracklist(
    ui: &mut Ui,
    open: &Open,
    tracks: &HashMap<String, Tracks>,
    s: &State,
    back: bool,
    reveal: &mut bool,
    actions: &mut Vec<Action>,
) {
    header(ui, &open.name, back, Some(s), actions);
    let list = match tracks.get(&open.slug).and_then(|t| t.list.as_ref()) {
        None => return note(ui, "Loading…"),
        Some(Err(e)) => return note(ui, &format!("Could not load tracks: {e}")),
        Some(Ok(list)) => list,
    };
    let current = s.index.filter(|_| s.queue.as_deref() == Some(&open.slug));
    let target = current.filter(|_| std::mem::take(reveal));
    rows(ui, list.len(), target, |ui, i| {
        if row(ui, title(&list[i]), false, current == Some(i), false).clicked() {
            actions.push(Action::Play(i));
        }
    });
}

/// The bar along the top: a back chevron, the title centred, and what's playing at the right, as on an iPod.
fn header(
    ui: &mut Ui,
    title: &str,
    back: bool,
    s: Option<&State>,
    actions: &mut Vec<Action>,
) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER), Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        Rect::from_x_y_ranges(rect.x_range(), rect.bottom() - 1.0..=rect.bottom()),
        0.0,
        HAIRLINE,
    );
    let cy = rect.top() + (HEADER - 1.0) / 2.0;
    let side = MARGIN + 4.0 * UNIT;
    let g = fit(
        ui,
        title,
        font(BODY, Weight::SemiBold),
        TEXT,
        rect.width() - 2.0 * side,
    );
    put(ui, g.clone(), rect.center().x - g.size().x / 2.0, cy);

    if back {
        let hit = Rect::from_x_y_ranges(
            rect.left()..=rect.left() + side,
            rect.top()..=rect.bottom() - 1.0,
        );
        let r = ui.interact(hit, ui.id().with("back"), Sense::click());
        chevron(
            ui,
            pos2(rect.left() + MARGIN, cy),
            -1.0,
            5.0,
            if r.hovered() { ACCENT } else { TEXT },
        );
        if r.on_hover_text("Back (esc)").clicked() {
            actions.push(Action::Back);
        }
    }
    if let Some(s) = s.filter(|s| s.queue.is_some()) {
        let icon = if s.paused { Icon::Pause } else { Icon::Play };
        let size = 12.0;
        let c = pos2(rect.right() - MARGIN - half(icon, size), cy);
        let hit = Rect::from_center_size(c, vec2(HIT, HIT));
        let r = ui.interact(hit, ui.id().with("playing"), Sense::click());
        icon.image(ACCENT, size)
            .paint_at(ui, Rect::from_center_size(c, vec2(size, size)));
        if r.on_hover_text("Now playing").clicked() {
            actions.push(Action::ShowNow);
        }
    }
    rect
}

/// The speaker at the header's right, opposite back: click to mute. Scrolling anywhere on the view, or up and down,
/// turns the volume like the Watch's crown, and the header's hairline fills to show it, then fades.
fn speaker(
    ui: &mut Ui,
    header: Rect,
    s: &State,
    volume: Option<Volume>,
    actions: &mut Vec<Action>,
) {
    let cy = header.top() + (HEADER - 1.0) / 2.0;
    // Both speakers end at x 23 of their 24-unit box, so the body stays put as the waves turn to a cross;
    // the box ends at the margin so the stroke's soft edge does too.
    let right = header.right() - MARGIN;
    let icon_rect = Rect::from_min_max(
        pos2(right - ICON, cy - ICON / 2.0),
        pos2(right, cy + ICON / 2.0),
    );
    let hit = Rect::from_center_size(icon_rect.center(), vec2(HIT, HIT));
    let r = ui.interact(hit, ui.id().with("speaker"), Sense::click());
    let (icon, tip) = if s.muted {
        (Icon::Muted, "Unmute (m)")
    } else {
        (Icon::Speaker, "Mute (m)")
    };
    let color = if r.hovered() { ACCENT } else { TEXT };
    icon.image(color, ICON).paint_at(ui, icon_rect);
    if r.on_hover_text(tip).clicked() {
        actions.push(Action::ToggleMute);
    }

    let view = header.union(ui.available_rect_before_wrap());
    let by: f64 = ui.input(|i| {
        i.raw
            .events
            .iter()
            .map(|e| match e {
                egui::Event::MouseWheel { unit, delta, .. } => {
                    let y = f64::from(delta.y);
                    match unit {
                        egui::MouseWheelUnit::Line => y * STEP,
                        egui::MouseWheelUnit::Point => y / 8.0,
                        egui::MouseWheelUnit::Page => y * 4.0 * STEP,
                    }
                }
                _ => 0.0,
            })
            .sum()
    });
    if by != 0.0 && ui.rect_contains_pointer(view) {
        actions.push(Action::Volume(by));
    }

    let Some(v) = volume else { return };
    let fade = v.opacity();
    let level = if s.muted { 0.0 } else { (v.to / 100.0) as f32 };
    let meter = Rect::from_x_y_ranges(header.x_range(), header.bottom() - 2.0..=header.bottom());
    let mut done = meter;
    done.set_right(meter.left() + meter.width() * level);
    ui.painter()
        .rect_filled(meter, 0.0, HAIRLINE.gamma_multiply(fade));
    ui.painter()
        .rect_filled(done, 0.0, TEXT.gamma_multiply(fade));
    ui.ctx().request_repaint();
}

/// One row of a list: the name at the margin, then a playing dot and, for rows that open something, a chevron.
fn row(ui: &mut Ui, label: &str, selected: bool, playing: bool, opens: bool) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    if !ui.is_rect_visible(rect) {
        return r;
    }
    let fill = if selected {
        SELECTED
    } else if r.hovered() || r.has_focus() {
        HOVER
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    let cy = rect.center().y;
    let mut right = rect.right() - MARGIN;
    if opens {
        chevron(ui, pos2(right, cy), 1.0, 4.0, DISABLED);
        right -= 4.0 + 2.0 * UNIT;
    }
    if playing {
        ui.painter()
            .circle_filled(pos2(right - 3.0, cy), 3.0, ACCENT);
        right -= 6.0 + 2.0 * UNIT;
    }
    let x = rect.left() + MARGIN;
    put(
        ui,
        fit(ui, label, font(BODY, Weight::Regular), TEXT, right - x),
        x,
        cy,
    );
    r
}

/// A list of `n` rows that only draws the visible ones, scrolled to centre `reveal` when given.
fn rows(ui: &mut Ui, n: usize, reveal: Option<usize>, mut row: impl FnMut(&mut Ui, usize)) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut area = egui::ScrollArea::vertical().auto_shrink(false);
    if let Some(i) = reveal {
        let top = i as f32 * ROW - (ui.available_height() - ROW) / 2.0;
        area = area.vertical_scroll_offset(top.max(0.0));
    }
    area.show_rows(ui, ROW, n, |ui, range| {
        for i in range {
            row(ui, i);
        }
    });
}

/// A quiet line where a list would be: loading, empty, or what went wrong.
fn note(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    let x = rect.left() + MARGIN;
    let g = fit(
        ui,
        text,
        font(BODY, Weight::Regular),
        SECONDARY,
        rect.right() - MARGIN - x,
    );
    put(ui, g, x, rect.center().y);
}

/// One line of text, cut short with an ellipsis past `width`.
fn fit(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping::truncate_at_width(width.max(0.0));
    ui.painter().layout_job(job)
}

/// Paints a line with the ink of its first letter at `x` and its capitals centred on `y`, its baseline on a whole pixel.
fn put(ui: &Ui, g: Arc<Galley>, x: f32, y: f32) -> Rect {
    let size = g
        .job
        .sections
        .first()
        .map_or(BODY, |s| s.format.font_id.size);
    let baseline = g
        .rows
        .first()
        .and_then(|r| r.row.glyphs.first().map(|gl| r.pos.y + gl.pos.y))
        .unwrap_or(g.size().y);
    let ppp = ui.pixels_per_point();
    let line = fastframe_text::snap_to_pixels(y + CAP * size / 2.0, ppp);
    put_top(ui, g, x, line - baseline)
}

/// Paints a line with the ink of its first letter at `x` and its top at `y`, on whole pixels.
/// Starting from the ink rather than the glyph's box lines text up with bars and icons at the same edge.
fn put_top(ui: &Ui, g: Arc<Galley>, x: f32, y: f32) -> Rect {
    let ppp = ui.pixels_per_point();
    let snap = |v| fastframe_text::snap_to_pixels(v, ppp);
    let ink = g
        .rows
        .first()
        .and_then(|r| r.row.glyphs.first())
        .map_or(0.0, |g| g.uv_rect.offset.x);
    let pos = pos2(snap(x - ink), snap(y));
    let rect = Rect::from_min_size(pos, g.size());
    ui.painter().galley(pos, g, TEXT);
    rect
}

/// A chevron with its point at `tip`, pointing right (`dir` 1) or left (-1).
fn chevron(ui: &Ui, tip: Pos2, dir: f32, h: f32, color: Color32) {
    // Pull the point in by half the stroke, so the ink and not the line's centre meets the edge.
    let tip = tip - vec2(dir * 0.75, 0.0);
    let arm = vec2(-dir * h, h);
    ui.painter().line(
        vec![tip + vec2(arm.x, -arm.y), tip, tip + arm],
        egui::Stroke::new(1.5, color),
    );
}

/// Half the drawn width of an icon at `size`, from its glyph in the 24-unit box, stroke included.
fn half(icon: Icon, size: f32) -> f32 {
    let units = match icon {
        Icon::Play => 8.5,
        Icon::Pause => 8.0,
        Icon::Prev | Icon::Next => 10.0,
        Icon::Speaker | Icon::Muted => 11.0,
    };
    units / 24.0 * size
}

/// An icon centred on `c`, answering clicks in a `HIT` square, with a soft circle under the pointer.
fn icon_button(ui: &mut Ui, icon: Icon, c: Pos2, size: f32, enabled: bool) -> Response {
    icon_button_sized(ui, icon, c, size, HIT, enabled)
}

fn icon_button_sized(
    ui: &mut Ui,
    icon: Icon,
    c: Pos2,
    size: f32,
    hit: f32,
    enabled: bool,
) -> Response {
    let hit = Rect::from_center_size(c, vec2(hit, hit));
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let r = ui.interact(hit, ui.id().with(("icon", icon as u8)), sense);
    if enabled && r.hovered() {
        ui.painter().circle_filled(c, hit.width() / 2.0, HOVER);
    }
    let color = if enabled { TEXT } else { DISABLED };
    icon.image(color, size)
        .paint_at(ui, Rect::from_center_size(c, vec2(size, size)));
    r
}

/// Play or pause, or a spinner while the track resolves.
fn play_button(ui: &mut Ui, s: &State, c: Pos2, size: f32, actions: &mut Vec<Action>) {
    play_button_sized(ui, s, c, size, HIT, actions);
}

fn play_button_sized(
    ui: &mut Ui,
    s: &State,
    c: Pos2,
    size: f32,
    hit: f32,
    actions: &mut Vec<Action>,
) {
    if s.loading() {
        egui::Spinner::new()
            .size(size)
            .color(TEXT)
            .paint_at(ui, Rect::from_center_size(c, vec2(size, size)));
        return;
    }
    let (icon, tip) = if s.queue.is_some() && !s.paused {
        (Icon::Pause, "Pause (space)")
    } else {
        (Icon::Play, "Play (space)")
    };
    if icon_button_sized(ui, icon, c, size, hit, true)
        .on_hover_text(tip)
        .clicked()
    {
        actions.push(Action::Toggle);
    }
}

/// The progress of the track, seekable. A `line` thick bar on `hit`'s top edge when `top`, else in its middle,
/// twice as thick under the pointer. Seeks once it is let go, not on every step of a drag.
fn seek(
    ui: &mut Ui,
    hit: Rect,
    top: bool,
    line: f32,
    s: &State,
    scrub: &mut Option<Scrub>,
    actions: &mut Vec<Action>,
) {
    let r = ui.interact(hit, ui.id().with("seek"), Sense::click_and_drag());
    let dur = s.dur.unwrap_or(0.0);
    if dur > 0.0 {
        if let Some(p) = r.interact_pointer_pos() {
            let frac = ((p.x - hit.left()) / hit.width()).clamp(0.0, 1.0) as f64;
            *scrub = Some(Scrub {
                to: frac * dur,
                dragging: true,
                at: Instant::now(),
            });
        }
        if (r.drag_stopped() || r.clicked())
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
    let live = dur > 0.0 && (r.hovered() || r.dragged());
    let thick = if live { 2.0 * line } else { line };
    let y = if top {
        hit.top()
    } else {
        hit.center().y - thick / 2.0
    };
    let rail = Rect::from_min_size(pos2(hit.left(), y), vec2(hit.width(), thick));
    let mut done = rail;
    done.set_right(rail.left() + rail.width() * frac);
    let round = if top { 0.0 } else { thick / 2.0 };
    ui.painter().rect_filled(rail, round, HAIRLINE);
    ui.painter().rect_filled(done, round, ACCENT);
    if dur > 0.0 {
        r.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
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
