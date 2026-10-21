//! The window shown when the app opens without a URL: R4 channels, a channel's tracks, and what's playing.
//! Small first: one view at a time with a transport along the bottom. With room, channels and tracks sit side by side.
use crate::channels::{self, Channel};
use crate::media::{self, NowPlaying};
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
/// The band at the top that the round buttons and the title pill float in; lists start below it and scroll under it.
const TOP: f32 = 12.0 * UNIT;
/// A round button, and the height of the title pill.
const KNOB: f32 = 8.0 * UNIT;
const AVATAR: f32 = 6.0 * UNIT;
/// How long the tracklist takes to glide to the playing track.
const GLIDE: Duration = Duration::from_millis(240);
/// The mini player's height; it floats this far plus a gap above the bottom.
const DOCK: f32 = 12.0 * UNIT;
/// The mini player's corners: superellipses this large, softer than a circle's quarter.
const SQUIRCLE: f32 = 6.0 * UNIT;
const SQUIRCLE_POWER: f32 = 4.0;
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
    let demo = crate::demo::from_env();
    let Some((_running, summons)) = crate::window::claim(demo.is_some()) else {
        return Ok(());
    };
    let memory = crate::window::Memory::load(demo.is_some());
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([280.0, 420.0])
        .with_min_inner_size([180.0, 200.0]);
    let options = eframe::NativeOptions {
        viewport: memory.geometry.apply(viewport),
        ..Default::default()
    };
    eframe::run_native(
        "Radio4000",
        options,
        Box::new(|cc| {
            crate::style::install(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fastframe_icons::install::<Icon>(&cc.egui_ctx);
            summons.attach(&cc.egui_ctx);

            let (tx, rx) = mpsc::channel();
            if demo.is_none() {
                let ctx = cc.egui_ctx.clone();
                std::thread::spawn(move || {
                    // The cached list paints first, the network corrects it a moment later.
                    if let Some(cached) = crate::cache::load() {
                        let _ = tx.send(Ok(cached));
                        ctx.request_repaint();
                    }
                    let fresh = channels::fetch();
                    match &fresh {
                        Ok(list) => crate::cache::save(list),
                        Err(e) => log::warn!("could not load channels: {e}"),
                    }
                    let _ = tx.send(fresh);
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
                media: {
                    let ctx = cc.egui_ctx.clone();
                    NowPlaying::start(media::App::new("radio4000", "Radio4000"), move || {
                        ctx.request_repaint()
                    })
                },
                seek: None,
                scrub: None,
                volume: None,
                scroll: Scroll::default(),
                channel_scroll: Scroll::default(),
                scrolling: fastframe_scroll::Scrolling::default(),
                keyed: false,
                actions: Vec::new(),
                demo: demo.is_some(),
                theme: crate::theme::Theme::new(&cc.egui_ctx, demo.is_some()),
                memory,
                summons,
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
                app.scroll.reveal = Some(Reveal::Jump);
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
    /// Media keys in, and what's on out to the system's now-playing widget.
    media: NowPlaying,
    /// The last seek, until the widget has been told where it landed.
    seek: Option<media::Seek>,
    scrub: Option<Scrub>,
    /// The volume last asked for, shown on the meter until it fades.
    volume: Option<Volume>,
    /// The tracklist's scroll and cursor, and the channel list's.
    scroll: Scroll,
    channel_scroll: Scroll,
    /// Wheel step, touchpad speed and glide, and axis lock for every list.
    scrolling: fastframe_scroll::Scrolling,
    /// The arrow keys are in use: show the cursor until the pointer moves.
    keyed: bool,
    actions: Vec<Action>,
    /// Showing the demo: the player's state stays as the demo set it.
    demo: bool,
    theme: crate::theme::Theme,
    memory: crate::window::Memory,
    /// Another launch asking for the window.
    summons: Arc<crate::window::Summons>,
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

/// How the tracklist brings the playing track to its middle: jumping there when a channel opens,
/// gliding there when the track changes, so you see where it went.
#[derive(Default)]
struct Scroll {
    reveal: Option<Reveal>,
    glide: Option<Glide>,
    /// The row the arrow keys are on.
    cursor: usize,
    /// Where the list was drawn last frame.
    offset: f32,
}

#[derive(Clone, Copy)]
enum Reveal {
    Jump,
    Glide,
    /// Scroll just enough to show this row, as an iPod list does.
    Show(usize),
}

#[derive(Clone, Copy)]
struct Glide {
    from: f32,
    to: f32,
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
    /// Move the list's cursor by this many rows.
    Cursor(isize),
    /// Open or play the row under the cursor.
    Choose,
    ToggleMute,
    Send(Command),
}

impl eframe::App for App {
    // Runs minimized too, where `ui` doesn't.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.memory.remember(ctx);
        self.summons.answer(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.theme.update(ui.ctx());
        let pal = palette(ui);
        self.receive();
        self.keys(ui.ctx());
        self.scrolling.apply(ui.ctx());
        self.media(ui.ctx());
        let wide = ui.ctx().content_rect().width() >= WIDE;
        let state = &self.player.state;
        let playing = state.queue.as_deref();
        let actions = &mut self.actions;

        let flat = egui::Frame::NONE.fill(pal.bg);
        let face_of_playing = playing.and_then(|q| avatar(&self.channels, q));
        if self.now {
            egui::CentralPanel::no_frame().frame(flat).show(ui, |ui| {
                let image = state
                    .queue
                    .as_deref()
                    .and_then(|q| avatar(&self.channels, q));
                now_playing(ui, state, image, &mut self.scrub, self.volume, actions);
            });
        } else if wide {
            let open = self.open.as_ref().map(|o| o.slug.as_str());
            egui::Panel::left("channels")
                .frame(flat)
                .resizable(true)
                .default_size(240.0)
                .show(ui, |ui| {
                    channel_list(
                        ui,
                        &self.channels,
                        open,
                        playing,
                        false,
                        &mut self.channel_scroll,
                        self.keyed && self.open.is_none(),
                        actions,
                    );
                });
            egui::CentralPanel::no_frame().frame(flat).show(ui, |ui| {
                match &self.open {
                    Some(o) => {
                        let image = avatar(&self.channels, &o.slug);
                        tracklist(
                            ui,
                            o,
                            image,
                            &self.tracks,
                            state,
                            false,
                            &mut self.scroll,
                            self.keyed,
                            actions,
                        )
                    }
                    None => note(ui, "Pick a channel."),
                }
                dock(ui, state, face_of_playing, &mut self.scrub, actions);
            });
        } else {
            egui::CentralPanel::no_frame().frame(flat).show(ui, |ui| {
                match &self.open {
                    Some(o) => {
                        let image = avatar(&self.channels, &o.slug);
                        tracklist(
                            ui,
                            o,
                            image,
                            &self.tracks,
                            state,
                            true,
                            &mut self.scroll,
                            self.keyed,
                            actions,
                        )
                    }
                    None => {
                        channel_list(
                            ui,
                            &self.channels,
                            None,
                            playing,
                            true,
                            &mut self.channel_scroll,
                            self.keyed,
                            actions,
                        );
                    }
                }
                dock(ui, state, face_of_playing, &mut self.scrub, actions);
            });
        }

        for a in std::mem::take(&mut self.actions) {
            self.act(ui.ctx(), a);
        }
        self.theme.paint(ui.ctx());
    }
}

impl App {
    fn receive(&mut self) {
        if let Ok(result) = self.rx.try_recv() {
            // A failed refresh is no reason to throw away a list that works.
            let keep = result.is_err() && matches!(self.channels, Some(Ok(_)));
            if !keep {
                self.channels = Some(result);
            }
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
            self.scroll.reveal = Some(Reveal::Glide);
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
            } else {
                key(Key::ArrowUp, Action::Cursor(-1));
                key(Key::ArrowDown, Action::Cursor(1));
                key(Key::PageUp, Action::Cursor(-8));
                key(Key::PageDown, Action::Cursor(8));
                key(Key::Enter, Action::Choose);
            }
            if i.pointer.delta() != egui::Vec2::ZERO {
                self.keyed = false;
            }
            if i.pointer.button_pressed(PointerButton::Extra1) {
                self.actions.push(Action::Back);
            }
        });
    }

    /// Turns what the media keys and the widget asked for into actions, and shows the widget what's on.
    fn media(&mut self, ctx: &egui::Context) {
        use media::Command as M;
        let s = &self.player.state;
        let loaded = s.queue.is_some();
        for c in self.media.commands() {
            let a = match c {
                M::PlayPause => Action::Toggle,
                M::Play if !loaded || s.paused => Action::Toggle,
                M::Pause | M::Stop if loaded && !s.paused => Action::Toggle,
                M::Next => Action::Send(Command::Next),
                M::Previous => Action::Send(Command::Prev),
                M::SeekBy(ms) => Action::Send(Command::SeekBy(ms as f64 / 1000.0)),
                M::SetPosition { track_id, position }
                    if media::track_id(s).as_ref() == Some(&track_id) =>
                {
                    Action::Send(Command::Seek(position.as_secs_f64()))
                }
                M::SetVolume(v) => {
                    Action::Volume(v * 100.0 - self.volume.map_or(s.volume, |v| v.to))
                }
                M::Raise => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    continue;
                }
                M::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    continue;
                }
                _ => continue,
            };
            self.actions.push(a);
        }
        let art = s
            .queue
            .as_deref()
            .and_then(|q| avatar(&self.channels, q))
            .map(|image| channels::avatar_url(image, 512));
        self.media.update(media::shown(s, art));
        if let Some(seek) = &self.seek
            && seek.landed(s.pos)
        {
            self.media.seeked(Duration::from_secs_f64(s.pos.max(0.0)));
            self.seek = None;
        }
    }

    fn act(&mut self, ctx: &egui::Context, a: Action) {
        match a {
            Action::Cursor(by) => {
                // The first press only shows where the cursor is.
                let by = if self.keyed { by } else { 0 };
                self.keyed = true;
                let (len, scroll) = match &self.open {
                    Some(o) => (
                        match self.tracks.get(&o.slug).and_then(|t| t.list.as_ref()) {
                            Some(Ok(l)) => l.len(),
                            _ => 0,
                        },
                        &mut self.scroll,
                    ),
                    None => (
                        match &self.channels {
                            Some(Ok(l)) => l.len(),
                            _ => 0,
                        },
                        &mut self.channel_scroll,
                    ),
                };
                if len > 0 {
                    // The cursor takes over from a touchpad's glide.
                    self.scrolling.stop();
                    scroll.cursor = scroll.cursor.saturating_add_signed(by).min(len - 1);
                    scroll.reveal = Some(Reveal::Show(scroll.cursor));
                }
            }
            Action::Choose if !self.now => match &self.open {
                Some(_) => self.play(self.scroll.cursor),
                None => {
                    if let Some(Ok(list)) = &self.channels
                        && let Some(c) = list.get(self.channel_scroll.cursor)
                    {
                        let a = Action::Open(c.slug.clone(), c.name.clone());
                        self.act(ctx, a);
                    }
                }
            },
            Action::Choose => {}
            Action::Open(slug, name) => {
                if let Some(Ok(list)) = &self.channels
                    && let Some(i) = list.iter().position(|c| c.slug == slug)
                {
                    self.channel_scroll.cursor = i;
                }
                self.scroll.cursor = match self.player.state.queue.as_deref() == Some(&slug) {
                    true => self.player.state.index.unwrap_or(0),
                    false => 0,
                };
                self.scroll.glide = None;
                // A glide on the channel list stays there.
                self.scrolling.stop();
                self.tracks
                    .entry(slug.clone())
                    .or_insert_with(|| Tracks::fetch(ctx, &slug));
                self.scroll.reveal =
                    (self.player.state.queue.as_deref() == Some(&slug)).then_some(Reveal::Jump);
                self.open = Some(Open { slug, name });
                self.now = false;
            }
            Action::Back => {
                self.scrolling.stop();
                if self.now {
                    self.now = false;
                } else {
                    self.open = None;
                }
            }
            Action::ShowNow => self.now = self.player.state.queue.is_some(),
            Action::Play(index) => {
                self.scroll.cursor = index;
                self.play(index);
            }
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
            Action::Send(c) if self.demo => {
                let by = match c {
                    Command::Next => 1,
                    Command::Prev => -1,
                    _ => return,
                };
                let i = self.player.state.index.unwrap_or(0) as isize + by;
                self.play(i.max(0) as usize);
            }
            Action::Send(c) => {
                if let Some(seek) = media::Seek::new(&self.player.state, &c) {
                    self.seek = Some(seek);
                }
                self.player.send(c);
            }
        }
    }

    fn play(&mut self, index: usize) {
        let Some(open) = &self.open else { return };
        // The demo's player stands still, so follow the tap by hand.
        if self.demo {
            self.player.state.index = Some(index);
            self.scroll.reveal = Some(Reveal::Glide);
            return;
        }
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
            let tracks = tracks::fetch(&slug);
            if let Err(e) = &tracks {
                log::warn!("could not load the tracks of {slug}: {e}");
            }
            let _ = tx.send(tracks);
            ctx.request_repaint();
        });
        Tracks { rx, list: None }
    }
}

fn title(t: &Track) -> &str {
    t.title.as_deref().unwrap_or(&t.url)
}

/// The mini player: a capsule floating over the bottom of the list, with the channel's face, what's on, play and
/// next, and the track's progress along its top edge. Touching it opens the now-playing view. Hidden while nothing is loaded.
fn dock(
    ui: &mut Ui,
    s: &State,
    image: Option<&str>,
    scrub: &mut Option<Scrub>,
    actions: &mut Vec<Action>,
) {
    let pal = palette(ui);
    if s.queue.is_none() && s.error.is_none() {
        return;
    }
    let pane = ui.max_rect();
    let width = (pane.width() - 4.0 * UNIT).min(120.0 * UNIT);
    let rect = Rect::from_min_size(
        pos2(
            (pane.center().x - width / 2.0).round(),
            pane.bottom() - 2.0 * UNIT - DOCK,
        ),
        vec2(width, DOCK),
    );
    let p = ui.painter();
    // The list fades out just above it and is gone by its top edge.
    let band = Rect::from_x_y_ranges(pane.x_range(), rect.top() - 4.0 * UNIT..=rect.top());
    p.rect_filled(
        Rect::from_x_y_ranges(pane.x_range(), rect.top()..=pane.bottom()),
        0.0,
        pal.bg,
    );
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(band.left_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(band.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(band.left_bottom(), pal.bg);
    mesh.colored_vertex(band.right_bottom(), pal.bg);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    p.add(mesh);
    let round = SQUIRCLE;
    p.add(
        egui::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: pal.shadow,
        }
        .as_shape(rect, round * 0.6),
    );
    let open = ui.interact(rect, ui.id().with("dock"), Sense::click());
    let fill = if open.hovered() {
        pal.selected
    } else {
        pal.surface
    };
    squircle(ui, rect, round, fill, pal.edge);
    if open.clicked() && s.queue.is_some() {
        actions.push(Action::ShowNow);
    }

    // The top edge between the corners is the seek line: it fills with the track and thickens to be dragged.
    let edge = Rect::from_min_size(
        pos2(rect.left() + round, rect.top()),
        vec2(rect.width() - 2.0 * round, 2.0 * UNIT),
    );
    seek(ui, edge, true, LINE, s, scrub, actions);

    // The face sits as far in from the left as from the top and bottom; next mirrors it at the right.
    let cy = rect.center().y;
    let inset = 2.0 * UNIT;
    let size = DOCK - 2.0 * inset;
    let side = inset + size / 2.0;
    face(
        ui,
        Rect::from_center_size(pos2(rect.left() + side, cy), vec2(size, size)),
        image,
        &s.channel,
    );

    let next = pos2(rect.right() - side, cy);
    let play = next - vec2(HIT, 0.0);
    if icon_button(ui, Icon::Next, next, ICON, s.queue.is_some()).clicked() {
        actions.push(Action::Send(Command::Next));
    }
    play_button(ui, s, play, ICON, actions);

    let x = rect.left() + inset + size + inset;
    let width = play.x - HIT / 2.0 - x;
    if let Some(e) = &s.error {
        let g = fit(
            ui,
            &format!("Can't play: {e}"),
            font(SMALL, Weight::Regular),
            pal.secondary,
            width,
        );
        put(ui, g, x, cy);
        return;
    }
    let time = s.dur.map_or_else(String::new, |d| {
        format!(" · {} / {}", clock(shown(s, scrub)), clock(d))
    });
    put(
        ui,
        fit(ui, &s.title, font(BODY, Weight::SemiBold), pal.text, width),
        x,
        cy - 2.0 * UNIT,
    );
    let sub = format!("{}{time}", s.channel);
    put(
        ui,
        fit(ui, &sub, font(SMALL, Weight::Regular), pal.secondary, width),
        x,
        cy + 2.0 * UNIT,
    );
}

/// A rectangle with superellipse corners, filled, with a hairline edge just inside it.
/// The edge is the outer shape showing round a 1px smaller inner one: two fills, no stroke joins to spike.
fn squircle(ui: &Ui, rect: Rect, r: f32, fill: Color32, edge: Color32) {
    let p = ui.painter();
    p.add(egui::Shape::convex_polygon(
        squircle_outline(rect, r),
        edge,
        egui::Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        squircle_outline(rect.shrink(1.0), r - 1.0),
        fill,
        egui::Stroke::NONE,
    ));
}

/// The outline clockwise from the top edge's right end, each corner swept from one straight edge to the next.
fn squircle_outline(rect: Rect, r: f32) -> Vec<Pos2> {
    let e = 2.0 / SQUIRCLE_POWER;
    let mut points = Vec::new();
    for (c, sx, sy, reverse) in [
        (pos2(rect.right() - r, rect.top() + r), 1.0, -1.0, true),
        (pos2(rect.right() - r, rect.bottom() - r), 1.0, 1.0, false),
        (pos2(rect.left() + r, rect.bottom() - r), -1.0, 1.0, true),
        (pos2(rect.left() + r, rect.top() + r), -1.0, -1.0, false),
    ] {
        for k in 0..=16 {
            let t = k as f32 / 16.0;
            let a = if reverse { 1.0 - t } else { t } * std::f32::consts::FRAC_PI_2;
            // cos(π/2) comes out a hair below zero, and its root would be NaN.
            let (x, y) = (a.cos().max(0.0).powf(e), a.sin().max(0.0).powf(e));
            points.push(c + vec2(sx * r * x, sy * r * y));
        }
    }
    // Corners as tall as half the rectangle meet at a shared point.
    points.dedup_by(|a, b| a.distance(*b) < 0.01);
    points
}

/// A stroke clockwise from twelve o'clock, `frac` of the way round.
fn arc(ui: &Ui, c: Pos2, radius: f32, frac: f32, stroke: egui::Stroke) {
    let steps = (64.0 * frac).ceil() as usize;
    if steps == 0 {
        return;
    }
    let points = (0..=steps)
        .map(|k| {
            let a = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * frac * k as f32 / steps as f32;
            c + radius * vec2(a.cos(), a.sin())
        })
        .collect();
    ui.painter().line(points, stroke);
}

/// The now-playing view, laid out like Apple's: title and channel at the margin, the seek bar with its times,
/// then previous, play and next, the whole group centred in the window.
fn now_playing(
    ui: &mut Ui,
    s: &State,
    image: Option<&str>,
    scrub: &mut Option<Scrub>,
    volume: Option<Volume>,
    actions: &mut Vec<Action>,
) {
    let pal = palette(ui);
    let rect = ui.max_rect();
    let cy = rect.top() + 2.0 * UNIT + KNOB / 2.0;
    back(ui, pos2(rect.left() + 2.0 * UNIT + KNOB / 2.0, cy), actions);
    speaker(ui, rect, s, volume, actions);
    // As much room under the buttons as the knobs leave above the group.
    let area = Rect::from_x_y_ranges(
        rect.x_range(),
        rect.top() + TOP..=rect.bottom() - 6.0 * UNIT,
    );
    let x = area.left() + MARGIN;
    let width = area.width() - 2.0 * MARGIN;

    let channel = fit(
        ui,
        &s.channel,
        font(BODY, Weight::Regular),
        pal.accent,
        width,
    );
    let big = 2.0 * ICON;
    // Each step below is a gap on the grid, from the title's top to the buttons' bottom.
    let below = UNIT + channel.size().y + 6.0 * UNIT + 8.0 * UNIT + 6.0 * UNIT + big;
    // The title takes two lines where they fit, one in a small window.
    let title = |rows| {
        let mut job = LayoutJob::simple(
            s.title.clone(),
            font(LARGE, Weight::SemiBold),
            pal.text,
            width,
        );
        job.wrap.max_rows = rows;
        ui.painter().layout_job(job)
    };
    let mut title = title(2);
    if title.size().y + below > area.height() {
        title = self::title_line(ui, &s.title, width);
    }
    let height = title.size().y + below;
    // The channel's avatar as artwork, as large as the room above the group allows, on 16s; none when that's tiny.
    let gap = 6.0 * UNIT;
    let room = area.height() - height - 2.0 * gap;
    let side = (room.min(width).min(60.0 * UNIT) / (4.0 * UNIT)).floor() * 4.0 * UNIT;
    let art = image.filter(|_| side >= 24.0 * UNIT);
    let total = height + art.map_or(0.0, |_| side + gap);
    let mut y = (area.top() + (area.height() - total) / 2.0).max(area.top());
    y = (y / UNIT).round() * UNIT;
    if let Some(id) = art {
        let at = Rect::from_min_size(
            pos2((area.center().x - side / 2.0).round(), y),
            vec2(side, side),
        );
        let px = (side * ui.pixels_per_point()).round() as u32;
        ui.painter().rect_filled(at, 0.0, pal.surface);
        egui::Image::new(channels::avatar_url(id, px))
            .corner_radius(0.0)
            .show_loading_spinner(false)
            .paint_at(ui, at);
        y += side + gap;
    }

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
        fit(
            ui,
            &clock(shown(s, scrub)),
            small.clone(),
            pal.secondary,
            width,
        ),
        x,
        y,
    );
    let left = s.dur.map_or_else(
        || "-:--".into(),
        |d| format!("−{}", clock(d - shown(s, scrub))),
    );
    let g = fit(ui, &left, small, pal.secondary, width);
    put(ui, g.clone(), x + width - g.size().x, y);
    y += 6.0 * UNIT + big / 2.0;

    let loaded = s.queue.is_some();
    let step = (width / 3.0).min(16.0 * UNIT);
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

fn title_line(ui: &Ui, title: &str, width: f32) -> Arc<Galley> {
    let pal = palette(ui);
    fit(ui, title, font(LARGE, Weight::SemiBold), pal.text, width)
}

/// Where the track is, or where it is being dragged to.
fn shown(s: &State, scrub: &Option<Scrub>) -> f64 {
    scrub.as_ref().map_or(s.pos, |sc| sc.to)
}

#[allow(clippy::too_many_arguments)]
fn channel_list(
    ui: &mut Ui,
    channels: &Option<Result<Vec<Channel>, String>>,
    open: Option<&str>,
    playing: Option<&str>,
    opens: bool,
    scroll: &mut Scroll,
    keyed: bool,
    actions: &mut Vec<Action>,
) {
    let list = match channels {
        None => return note(ui, "Loading…"),
        Some(Err(e)) => return note(ui, &format!("Could not load channels: {e}")),
        Some(Ok(list)) => list,
    };
    let cursor = keyed.then_some(scroll.cursor);
    rows(ui, list.len(), 2.0 * UNIT, None, Some(scroll), |ui, i| {
        let c = &list[i];
        let selected = open == Some(c.slug.as_str()) || cursor == Some(i);
        if row(
            ui,
            &c.name,
            Some(c.image.as_deref()),
            selected,
            playing == Some(&c.slug),
            opens,
        )
        .clicked()
        {
            actions.push(Action::Open(c.slug.clone(), c.name.clone()));
        }
    });
}

/// The channel's avatar id, if it has one.
fn avatar<'a>(channels: &'a Option<Result<Vec<Channel>, String>>, slug: &str) -> Option<&'a str> {
    let Some(Ok(list)) = channels else {
        return None;
    };
    list.iter()
        .find(|c| c.slug == slug)
        .and_then(|c| c.image.as_deref())
}

#[allow(clippy::too_many_arguments)]
fn tracklist(
    ui: &mut Ui,
    open: &Open,
    image: Option<&str>,
    tracks: &HashMap<String, Tracks>,
    s: &State,
    back: bool,
    scroll: &mut Scroll,
    keyed: bool,
    actions: &mut Vec<Action>,
) {
    let rect = ui.max_rect();
    match tracks.get(&open.slug).and_then(|t| t.list.as_ref()) {
        None => note(ui, "Loading…"),
        Some(Err(e)) => note(ui, &format!("Could not load tracks: {e}")),
        Some(Ok(list)) => {
            let current = s.index.filter(|_| s.queue.as_deref() == Some(&open.slug));
            let cursor = keyed.then_some(scroll.cursor);
            rows(ui, list.len(), TOP, current, Some(scroll), |ui, i| {
                let selected = cursor == Some(i);
                if row(
                    ui,
                    title(&list[i]),
                    None,
                    selected,
                    current == Some(i),
                    false,
                )
                .clicked()
                {
                    actions.push(Action::Play(i));
                }
            });
        }
    }
    chrome(ui, rect, &open.name, image, back, actions);
}

/// What floats over the top of a list: a fade the rows scroll under, a round back button,
/// and the title in a pill with the channel's avatar.
fn chrome(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    image: Option<&str>,
    back_button: bool,
    actions: &mut Vec<Action>,
) {
    let pal = palette(ui);
    let band = Rect::from_x_y_ranges(rect.x_range(), rect.top()..=rect.top() + TOP);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(band.left_top(), pal.bg);
    mesh.colored_vertex(band.right_top(), pal.bg);
    mesh.colored_vertex(band.left_bottom(), Color32::TRANSPARENT);
    mesh.colored_vertex(band.right_bottom(), Color32::TRANSPARENT);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(mesh);

    let side = 2.0 * UNIT + KNOB / 2.0;
    let cy = rect.top() + side;
    if back_button {
        back(ui, pos2(rect.left() + side, cy), actions);
    }
    pill(
        ui,
        pos2(rect.center().x, cy),
        title,
        image,
        rect.width() - 4.0 * side,
    );
}

/// A round button that floats over what's under it.
fn knob(ui: &mut Ui, c: Pos2, id: &str) -> Response {
    let pal = palette(ui);
    let r = ui.interact(
        Rect::from_center_size(c, vec2(KNOB, KNOB)),
        ui.id().with(id),
        Sense::click(),
    );
    let fill = if r.hovered() || r.has_focus() {
        pal.selected
    } else {
        pal.surface
    };
    ui.painter()
        .circle(c, KNOB / 2.0 - 0.5, fill, egui::Stroke::new(1.0, pal.edge));
    r
}

fn back(ui: &mut Ui, c: Pos2, actions: &mut Vec<Action>) {
    let pal = palette(ui);
    let r = knob(ui, c, "back");
    // The chevron's ink is 6.5 wide from its tip; centre that.
    chevron(ui, pos2(c.x - 3.25, c.y), -1.0, 5.0, pal.text);
    if r.on_hover_text("Back (esc)").clicked() {
        actions.push(Action::Back);
    }
}

/// The title in a pill centred on `c`, led by the avatar when there is one.
fn pill(ui: &mut Ui, c: Pos2, title: &str, image: Option<&str>, max: f32) {
    let pal = palette(ui);
    if title.is_empty() {
        return;
    }
    let lead = if image.is_some() {
        UNIT + AVATAR + 2.0 * UNIT
    } else {
        3.0 * UNIT
    };
    let trail = 3.0 * UNIT;
    let g = fit(
        ui,
        title,
        font(BODY, Weight::SemiBold),
        pal.text,
        max - lead - trail,
    );
    // Whole pixels from the ink's last soft edge, so both sides read the same.
    let left = (c.x - (lead + ink_width(&g) + trail) / 2.0).round();
    let width = (left + lead + ink_width(&g)).ceil() + trail - left;
    let rect = Rect::from_min_size(pos2(left, c.y - KNOB / 2.0), vec2(width, KNOB));
    ui.painter().rect(
        rect,
        KNOB / 2.0,
        pal.surface,
        egui::Stroke::new(1.0, pal.edge),
        egui::StrokeKind::Inside,
    );
    if image.is_some() {
        let at = Rect::from_min_size(pos2(left + UNIT, c.y - AVATAR / 2.0), vec2(AVATAR, AVATAR));
        face(ui, at, image, title);
    }
    put(ui, g, left + lead, c.y);
}

/// A round speaker opposite back: click to mute. Scrolling anywhere on the view, or up and down, turns the volume
/// like the Watch's crown, and the button's rim fills to show it, then fades.
fn speaker(ui: &mut Ui, view: Rect, s: &State, volume: Option<Volume>, actions: &mut Vec<Action>) {
    let pal = palette(ui);
    let side = 2.0 * UNIT + KNOB / 2.0;
    let c = pos2(view.right() - side, view.top() + side);
    let r = knob(ui, c, "speaker");
    let (icon, tip) = if s.muted {
        (Icon::Muted, "Unmute (m)")
    } else {
        (Icon::Speaker, "Mute (m)")
    };
    icon.image(pal.text, ICON)
        .paint_at(ui, Rect::from_center_size(c, vec2(ICON, ICON)));
    if r.on_hover_text(tip).clicked() {
        actions.push(Action::ToggleMute);
    }

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
    // The button's own rim is the meter, as the Watch's ring hugs its play button.
    let radius = KNOB / 2.0 - 1.0;
    ui.painter().circle_stroke(
        c,
        radius,
        egui::Stroke::new(2.0, pal.hairline.gamma_multiply(fade)),
    );
    arc(
        ui,
        c,
        radius,
        level,
        egui::Stroke::new(2.0, pal.text.gamma_multiply(fade)),
    );
    ui.ctx().request_repaint();
}

/// One row of a list: the name at the margin, then a playing dot or, for rows that open something, a chevron.
/// `avatar` gives the row an avatar slot at the margin: the channel's image, or its initial when it has none.
fn row(
    ui: &mut Ui,
    label: &str,
    avatar: Option<Option<&str>>,
    selected: bool,
    playing: bool,
    opens: bool,
) -> Response {
    let pal = palette(ui);
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    if !ui.is_rect_visible(rect) {
        return r;
    }
    let fill = if selected {
        pal.selected
    } else if r.hovered() || r.has_focus() {
        pal.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    let cy = rect.center().y;
    let mut right = rect.right() - MARGIN;
    // The playing dot takes the chevron's place rather than sitting beside it.
    if playing {
        ui.painter()
            .circle_filled(pos2(right - 3.0, cy), 3.0, pal.accent);
        right -= 6.0 + 2.0 * UNIT;
    } else if opens {
        chevron(ui, pos2(right, cy), 1.0, 4.0, pal.disabled);
        right -= 4.0 + 2.0 * UNIT;
    }
    let mut x = rect.left() + MARGIN;
    if let Some(image) = avatar {
        let at = Rect::from_min_size(pos2(x, cy - AVATAR / 2.0), vec2(AVATAR, AVATAR));
        face(ui, at, image, label);
        x += AVATAR + 2.0 * UNIT;
    }
    put(
        ui,
        fit(ui, label, font(BODY, Weight::Regular), pal.text, right - x),
        x,
        cy,
    );
    r
}

/// A channel's avatar in a circle, or its first letter while there's no picture.
fn face(ui: &Ui, at: Rect, image: Option<&str>, name: &str) {
    let pal = palette(ui);
    let r = at.width() / 2.0;
    ui.painter().circle_filled(at.center(), r, pal.surface);
    let initial: String = name.chars().take(1).flat_map(char::to_uppercase).collect();
    let g = fit(
        ui,
        &initial,
        font(SMALL, Weight::SemiBold),
        pal.secondary,
        at.width(),
    );
    put(
        ui,
        g.clone(),
        at.center().x - ink_width(&g) / 2.0,
        at.center().y,
    );
    if let Some(id) = image {
        let px = (at.width() * ui.pixels_per_point()).round() as u32;
        egui::Image::new(channels::avatar_url(id, px))
            .corner_radius(r)
            .show_loading_spinner(false)
            .paint_at(ui, at);
    }
}

/// A list of `n` rows starting `inset` down, which is room for what floats at the top, drawing only the visible ones. With a `scroll`, it brings `target` to
/// its middle as the scroll asks.
fn rows(
    ui: &mut Ui,
    n: usize,
    inset: f32,
    target: Option<usize>,
    scroll: Option<&mut Scroll>,
    mut row: impl FnMut(&mut Ui, usize),
) {
    let view = ui.available_height();
    let height = inset + n as f32 * ROW + DOCK + 6.0 * UNIT;
    let mut area = egui::ScrollArea::vertical().auto_shrink(false);
    let mut draw = |ui: &mut Ui, vp: Rect| {
        ui.set_height(height);
        let top = ui.max_rect().top();
        let first = ((vp.min.y - inset) / ROW).floor().max(0.0) as usize;
        let last = (((vp.max.y - inset) / ROW).ceil().max(0.0) as usize).min(n);
        for i in first..last {
            let at = Rect::from_min_size(
                pos2(ui.max_rect().left(), top + inset + i as f32 * ROW),
                vec2(ui.max_rect().width(), ROW),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(at).id_salt(i), |ui| {
                row(ui, i)
            });
        }
    };
    let Some(sc) = scroll else {
        area.show_viewport(ui, |ui, vp| draw(ui, vp));
        return;
    };
    let room = DOCK + 6.0 * UNIT;
    match sc.reveal.take() {
        Some(Reveal::Show(i)) => {
            // Just enough to bring the row out from under the top band or the mini player.
            let top = inset + i as f32 * ROW;
            let mut to = sc.offset;
            if top < to + inset {
                to = top - inset;
            } else if top + ROW > to + view - room {
                to = top + ROW - (view - room);
            }
            sc.glide = None;
            area = area.vertical_scroll_offset(to.clamp(0.0, (height - view).max(0.0)));
        }
        Some(how) => {
            if let Some(i) = target {
                // Centre it in what's left between the top band and the mini player.
                let middle = (inset + view - room + 2.0 * UNIT) / 2.0;
                let to = (inset + i as f32 * ROW + ROW / 2.0 - middle)
                    .clamp(0.0, (height - view).max(0.0));
                if matches!(how, Reveal::Jump) {
                    sc.glide = None;
                    area = area.vertical_scroll_offset(to);
                } else {
                    sc.glide = Some(Glide {
                        from: sc.offset,
                        to,
                        at: Instant::now(),
                    });
                }
            } else {
                // Not this list's turn yet; keep it for when the track is here.
                sc.reveal = Some(how);
            }
        }
        None => {}
    }
    // Scrolling by hand takes over from a glide.
    if ui.input(|i| i.smooth_scroll_delta.y != 0.0) {
        sc.glide = None;
    }
    if let Some(g) = sc.glide {
        let t = (g.at.elapsed().as_secs_f32() / GLIDE.as_secs_f32()).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        area = area.vertical_scroll_offset(g.from + (g.to - g.from) * eased);
        if t < 1.0 {
            ui.ctx().request_repaint();
        } else {
            sc.glide = None;
        }
    }
    sc.offset = area.show_viewport(ui, |ui, vp| draw(ui, vp)).state.offset.y;
}

/// A quiet line where a list would be: loading, empty, or what went wrong.
fn note(ui: &mut Ui, text: &str) {
    let pal = palette(ui);
    ui.add_space(TOP);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    let x = rect.left() + MARGIN;
    let g = fit(
        ui,
        text,
        font(BODY, Weight::Regular),
        pal.secondary,
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
    let pal = palette(ui);
    let ppp = ui.pixels_per_point();
    let snap = |v| fastframe_text::snap_to_pixels(v, ppp);
    let ink = g
        .rows
        .first()
        .and_then(|r| r.row.glyphs.first())
        .map_or(0.0, |g| g.uv_rect.offset.x);
    let pos = pos2(snap(x - ink), snap(y));
    let rect = Rect::from_min_size(pos, g.size());
    ui.painter().galley(pos, g, pal.text);
    rect
}

/// How wide a line's ink is, first letter to last, without the space either side of the glyphs.
fn ink_width(g: &Galley) -> f32 {
    let Some(row) = g.rows.first() else {
        return 0.0;
    };
    let (Some(first), Some(last)) = (row.row.glyphs.first(), row.row.glyphs.last()) else {
        return g.size().x;
    };
    let left = first.pos.x + first.uv_rect.offset.x;
    let right = last.pos.x + last.uv_rect.offset.x + last.uv_rect.size.x;
    right - left
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
    let pal = palette(ui);
    let hit = Rect::from_center_size(c, vec2(hit, hit));
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let r = ui.interact(hit, ui.id().with(("icon", icon as u8)), sense);
    if enabled && r.hovered() {
        ui.painter().circle_filled(c, hit.width() / 2.0, pal.hover);
    }
    let color = if enabled { pal.text } else { pal.disabled };
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
    let pal = palette(ui);
    if s.loading() {
        egui::Spinner::new()
            .size(size)
            .color(pal.text)
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
    let pal = palette(ui);
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
    ui.painter().rect_filled(rail, round, pal.hairline);
    ui.painter().rect_filled(done, round, pal.accent);
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
