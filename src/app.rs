//! The window shown when the app opens without a URL: R4 channels, a channel's tracks, and what's playing.
//! Small first: one view at a time with a transport along the bottom. With room, channels and tracks sit side by side.
mod browse;
mod list;
mod navigation;
mod transport;
mod widgets;

use self::browse::{
    Search, SearchTarget, channel_info, channel_list, matching, matching_tags, matching_tracks,
    open_page, search_panel, shown_tracks, title, toggle_tag, tracklist,
};
use self::list::{ListScroll, Reveal, note};
use self::navigation::{ChannelRef, Nav, Slide, snap};
use self::transport::{STEP, Scrub, Volume, dock, now_playing};
use self::widgets::Icon;
use crate::channels::{self, Channel};
use crate::library::Library;
use crate::media::{self, NowPlaying};
use crate::player::{self, Command, Player};
use crate::style::palette;
use egui::{Key, Modifiers, PointerButton, Sense, Ui, vec2};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// From this width channels and tracks sit side by side.
const WIDE: f32 = 560.0;
/// How far the view underneath drifts aside meanwhile, as a share of the width.
const DRIFT: f32 = 0.3;
/// How much it fades meanwhile.
const DIM: f32 = 0.4;
/// How dark the tracklist goes under the channel's details.
const SCRIM: f32 = 0.4;

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

            let library = if demo.is_some() {
                Library::fixed(Vec::new(), HashMap::new())
            } else {
                let ctx = cc.egui_ctx.clone();
                Library::start(move || ctx.request_repaint())
            };
            let ctx = cc.egui_ctx.clone();
            let mut app = App {
                library,
                nav: Nav::default(),
                pending_play: None,
                player: Player::spawn(&[], move || ctx.request_repaint()),
                media: {
                    let ctx = cc.egui_ctx.clone();
                    NowPlaying::start(media::app(), move || ctx.request_repaint())
                },
                seek: None,
                scrub: None,
                volume: None,
                track_scroll: ListScroll::default(),
                channel_scroll: ListScroll::default(),
                scrolling: fastframe_scroll::Scrolling::default(),
                keyboard_cursor: false,
                actions: Vec::new(),
                demo: demo.is_some(),
                theme: crate::theme::Theme::new(&cc.egui_ctx, demo.is_some()),
                memory,
                summons,
            };
            if let Some(d) = demo {
                let open = ChannelRef {
                    slug: "oskar".into(),
                    name: "Radio Oskar".into(),
                };
                app.library =
                    Library::fixed(d.channels, HashMap::from([(open.slug.clone(), d.tracks)]));
                app.player.state = d.state;
                app.nav.open_channel = d.open.then_some(open);
                app.nav.show_channel_info = d.info;
                app.nav.show_now_playing = d.now;
                app.nav.search = d.search.then(|| Search::new(SearchTarget::Channels));
                app.track_scroll.reveal = Some(Reveal::Jump);
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
    library: Library,
    nav: Nav,
    /// A channel asked to play from the list, waiting on its tracks.
    pending_play: Option<ChannelRef>,
    player: Player,
    /// Media keys in, and what's on out to the system's now-playing widget.
    media: NowPlaying,
    /// The last seek, until the widget has been told where it landed.
    seek: Option<media::Seek>,
    scrub: Option<Scrub>,
    /// The volume last asked for, shown on the meter until it fades.
    volume: Option<Volume>,
    /// The tracklist's scroll and cursor, and the channel list's.
    track_scroll: ListScroll,
    channel_scroll: ListScroll,
    /// Wheel step, touchpad speed and glide, and axis lock for every list.
    scrolling: fastframe_scroll::Scrolling,
    /// The arrow keys are in use: show the cursor until the pointer moves.
    keyboard_cursor: bool,
    actions: Vec<Action>,
    /// Showing the demo: the player's state stays as the demo set it.
    demo: bool,
    theme: crate::theme::Theme,
    memory: crate::window::Memory,
    /// Another launch asking for the window.
    summons: Arc<crate::window::Summons>,
}

/// What the views asked for this frame, done once they are drawn.
enum Action {
    Open(String, String),
    /// Play this channel from a random track, without opening it.
    PlayChannel(String, String),
    Back,
    /// Show or hide the open channel's details.
    Info,
    ShowNow,
    /// Open the search panel.
    Search(SearchTarget),
    /// Play this track of the open channel, within its filter.
    Play(usize),
    /// Toggle this tag in the open channel's filter.
    Tag(String),
    /// Drop the open channel's filter.
    ClearFilter,
    /// Play an original queue index from filtered results, keeping the query and list position.
    PlayTrack(ChannelRef, usize),
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
        self.poll_updates();
        self.read_keyboard(ui.ctx());
        self.scrolling.apply(ui.ctx());
        self.sync_media(ui.ctx());
        let wide = ui.ctx().content_rect().width() >= WIDE;
        let state = &self.player.state;
        let playing = state.queue.as_deref();
        let actions = &mut self.actions;

        let flat = egui::Frame::NONE.fill(pal.bg);
        let face_of_playing = playing.and_then(|q| self.library.avatar(q));
        // What's under now playing is drawn only while it slides.
        if !self.nav.show_now_playing || self.nav.now_playing_transition.is_some() {
            if let Some(search) = &mut self.nav.search {
                egui::CentralPanel::no_frame().frame(flat).show(ui, |ui| {
                    let tracks = match &search.target {
                        SearchTarget::Channels => None,
                        SearchTarget::Tracks(open) | SearchTarget::Tags(open) => {
                            self.library.tracks(&open.slug)
                        }
                    };
                    search_panel(
                        ui,
                        search,
                        self.library.channels(),
                        tracks,
                        self.nav.filter.as_deref(),
                        state,
                        actions,
                    );
                    dock(ui, state, face_of_playing, &mut self.scrub, actions);
                });
            } else if wide {
                let open = self.nav.open_channel.as_ref().map(|o| o.slug.as_str());
                egui::Panel::left("channels")
                    .frame(flat)
                    .resizable(true)
                    .default_size(240.0)
                    .show(ui, |ui| {
                        channel_list(
                            ui,
                            self.library.channels(),
                            open,
                            playing,
                            false,
                            true,
                            &mut self.channel_scroll,
                            self.keyboard_cursor && self.nav.open_channel.is_none(),
                            actions,
                        );
                    });
                egui::CentralPanel::no_frame().frame(flat).show(ui, |ui| {
                    let rect = ui.max_rect();
                    match &self.nav.open_channel {
                        Some(o) => channel_pane(
                            ui,
                            o,
                            self.library.find(&o.slug),
                            self.library.tracks(&o.slug),
                            self.nav.filter.as_deref(),
                            state,
                            self.nav.show_channel_info,
                            self.nav.info_transition,
                            None,
                            &mut self.track_scroll,
                            self.keyboard_cursor,
                            actions,
                        ),
                        None => note(ui, "Pick a channel."),
                    }
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        dock(ui, state, face_of_playing, &mut self.scrub, actions)
                    });
                });
            } else {
                egui::CentralPanel::no_frame().frame(flat).show(ui, |ui| {
                    let rect = ui.max_rect();
                    let shown = self.nav.channel_transition.map_or(
                        if self.nav.open_channel.is_some() {
                            1.0
                        } else {
                            0.0
                        },
                        |s| s.shown(),
                    );
                    // Only the view being slid to answers clicks and keys.
                    let mut ignored = Vec::new();
                    if self.nav.open_channel.is_none() || self.nav.channel_transition.is_some() {
                        let at = rect.translate(snap(ui, vec2(-DRIFT * shown * rect.width(), 0.0)));
                        let live = self.nav.open_channel.is_none();
                        let acts = if live { &mut *actions } else { &mut ignored };
                        ui.scope_builder(
                            egui::UiBuilder::new().max_rect(at).id_salt("channels"),
                            |ui| {
                                ui.shrink_clip_rect(rect);
                                if !live {
                                    let opacity = ui.painter().opacity();
                                    ui.disable();
                                    ui.set_opacity(opacity);
                                }
                                // Receding as the channel covers it.
                                ui.multiply_opacity(1.0 - DIM * shown);
                                channel_list(
                                    ui,
                                    self.library.channels(),
                                    None,
                                    playing,
                                    true,
                                    self.nav.channel_transition.is_none(),
                                    &mut self.channel_scroll,
                                    self.keyboard_cursor && live,
                                    acts,
                                );
                            },
                        );
                    }
                    if let Some(o) = self
                        .nav
                        .open_channel
                        .as_ref()
                        .or(self.nav.departing_channel.as_ref())
                    {
                        let at = rect.translate(snap(ui, vec2((1.0 - shown) * rect.width(), 0.0)));
                        let live = self.nav.open_channel.is_some();
                        let acts = if live { &mut *actions } else { &mut ignored };
                        ui.scope_builder(
                            egui::UiBuilder::new().max_rect(at).id_salt("tracks"),
                            |ui| {
                                ui.shrink_clip_rect(rect);
                                // Always, as the channel list may still be drawn under it on the slide's last frame.
                                ui.painter().rect_filled(at, 0.0, pal.bg);
                                if shown < 1.0 {
                                    ui.painter().vline(
                                        at.left(),
                                        at.y_range(),
                                        egui::Stroke::new(1.0, pal.hairline),
                                    );
                                }
                                if !live {
                                    let opacity = ui.painter().opacity();
                                    ui.disable();
                                    ui.set_opacity(opacity);
                                }
                                channel_pane(
                                    ui,
                                    o,
                                    self.library.find(&o.slug),
                                    self.library.tracks(&o.slug),
                                    self.nav.filter.as_deref(),
                                    state,
                                    self.nav.show_channel_info,
                                    self.nav.info_transition,
                                    Some(shown),
                                    &mut self.track_scroll,
                                    self.keyboard_cursor && live,
                                    acts,
                                );
                            },
                        );
                        if !live {
                            ui.interact(
                                at.intersect(rect).intersect(ui.clip_rect()),
                                ui.id().with("departing-channel-shield"),
                                Sense::click_and_drag(),
                            );
                        }
                    }
                    // The mini player stays put while the lists slide under it,
                    // measured from the pane, which a view sliding off has stretched.
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        dock(ui, state, face_of_playing, &mut self.scrub, actions)
                    });
                });
            }
        }
        if self.nav.show_now_playing || self.nav.now_playing_transition.is_some() {
            let screen = ui.ctx().content_rect();
            let shown = self.nav.now_playing_transition.map_or(1.0, |s| s.shown());
            let rect = screen.translate(snap(ui, vec2(0.0, (1.0 - shown) * screen.height())));
            egui::Area::new(egui::Id::new("now"))
                .order(egui::Order::Foreground)
                .constrain(false)
                .fixed_pos(rect.min)
                .show(ui.ctx(), |ui| {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        ui.painter().rect_filled(rect, 0.0, pal.bg);
                        if shown < 1.0 {
                            ui.painter().hline(
                                rect.x_range(),
                                rect.top(),
                                egui::Stroke::new(1.0, pal.hairline),
                            );
                        }
                        // Clicks only count once it's coming up, not while it goes.
                        let mut ignored = Vec::new();
                        let acts = if self.nav.show_now_playing {
                            &mut *actions
                        } else {
                            &mut ignored
                        };
                        now_playing(
                            ui,
                            state,
                            face_of_playing,
                            &mut self.scrub,
                            self.volume,
                            acts,
                        );
                        ui.expand_to_include_rect(rect);
                    });
                });
        }

        self.nav.finish_transitions(ui.ctx());

        for a in std::mem::take(&mut self.actions) {
            self.dispatch(ui.ctx(), a);
        }
        self.theme.paint(ui.ctx());
    }
}

/// Details rise over the tracks; the caller draws the dock against the original pane afterwards.
#[allow(clippy::too_many_arguments)]
fn channel_pane(
    ui: &mut Ui,
    open: &ChannelRef,
    channel: Option<&Channel>,
    tracks: Option<Result<&[crate::tracks::Track], &str>>,
    filter: Option<&str>,
    state: &player::State,
    show_info: bool,
    transition: Option<Slide>,
    back: Option<f32>,
    scroll: &mut ListScroll,
    keyed: bool,
    actions: &mut Vec<Action>,
) {
    let rect = ui.max_rect();
    let shown = transition.map_or(if show_info { 1.0 } else { 0.0 }, |s| s.shown());
    let mut ignored = Vec::new();
    let acts = if show_info {
        &mut ignored
    } else {
        &mut *actions
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .id_salt(("tracklist", &open.slug)),
        |ui| {
            if show_info {
                let opacity = ui.painter().opacity();
                ui.disable();
                ui.set_opacity(opacity);
            }
            tracklist(
                ui,
                open,
                channel.and_then(|c| c.image.as_deref()),
                tracks,
                filter,
                state,
                back,
                scroll,
                keyed && !show_info,
                acts,
            );
        },
    );
    if show_info || transition.is_some() {
        // The tracklist darkens under the sheet; a click on it closes the sheet, and does nothing while it goes.
        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_black_alpha((SCRIM * shown * 255.0) as u8),
        );
        let scrim = ui.interact(
            rect,
            ui.id().with(("info-scrim", &open.slug)),
            Sense::click_and_drag(),
        );
        if show_info && scrim.clicked() {
            actions.push(Action::Info);
        }
        let acts = if show_info { actions } else { &mut ignored };
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .id_salt(("info", &open.slug)),
            |ui| {
                ui.shrink_clip_rect(rect);
                if !show_info {
                    let opacity = ui.painter().opacity();
                    ui.disable();
                    ui.set_opacity(opacity);
                }
                channel_info(ui, open, channel, shown, acts);
            },
        );
    }
}

impl App {
    fn poll_updates(&mut self) {
        self.library.poll_updates();
        if let Some(p) = self.pending_play.take() {
            match self.library.tracks(&p.slug) {
                None => self.pending_play = Some(p),
                Some(_) => self.shuffle(&p.slug, &p.name, None),
            }
        }
        let before = (self.player.state.queue.clone(), self.player.state.index);
        if !self.demo {
            self.player.update();
        }
        let s = &self.player.state;
        if s.index.is_some() && (s.queue.clone(), s.index) != before && !self.searching_in_channel()
        {
            self.track_scroll.reveal = Some(Reveal::Glide);
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

    fn read_keyboard(&mut self, ctx: &egui::Context) {
        let typing = self.nav.typing();
        let tabbed = ctx.memory(|m| m.focused()).is_some();
        ctx.input_mut(|i| {
            let mut key = |k, a| {
                if i.consume_key(Modifiers::NONE, k) {
                    self.actions.push(a);
                }
            };
            // Typing in the search field keeps its letters, space, backspace and arrows.
            if !typing {
                key(Key::Space, Action::Toggle);
                key(Key::N, Action::Send(Command::Next));
                key(Key::P, Action::Send(Command::Prev));
                key(Key::ArrowLeft, Action::Send(Command::SeekBy(-10.0)));
                key(Key::ArrowRight, Action::Send(Command::SeekBy(10.0)));
                key(Key::Backspace, Action::Back);
                key(Key::M, Action::ToggleMute);
            }
            if !self.nav.show_now_playing && !typing {
                let target = self
                    .nav
                    .open_channel
                    .as_ref()
                    .map_or(SearchTarget::Channels, |open| {
                        SearchTarget::Tracks(open.clone())
                    });
                if let SearchTarget::Tracks(open) = &target {
                    key(Key::T, Action::Search(SearchTarget::Tags(open.clone())));
                }
                key(Key::Slash, Action::Search(target));
            }
            key(Key::Escape, Action::Back);
            // Up and down are the iPod's wheel: volume, while the now-playing view is up.
            if self.nav.show_now_playing {
                key(Key::ArrowUp, Action::Volume(STEP));
                key(Key::ArrowDown, Action::Volume(-STEP));
            } else {
                key(Key::ArrowUp, Action::Cursor(-1));
                key(Key::ArrowDown, Action::Cursor(1));
                key(Key::PageUp, Action::Cursor(-8));
                key(Key::PageDown, Action::Cursor(8));
                // A button reached with Tab takes its own Return; the search field passes it on.
                if typing || !tabbed {
                    key(Key::Enter, Action::Choose);
                }
                if !typing {
                    key(Key::I, Action::Info);
                }
            }
            if i.pointer.delta() != egui::Vec2::ZERO {
                self.keyboard_cursor = false;
            }
            if i.pointer.button_pressed(PointerButton::Extra1) {
                self.actions.push(Action::Back);
            }
        });
    }

    /// Turns what the media keys and the widget asked for into actions, and shows the widget what's on.
    fn sync_media(&mut self, ctx: &egui::Context) {
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
            .and_then(|q| self.library.avatar(q))
            .map(|image| channels::avatar_url(image, 512));
        self.media.update(media::shown(s, art));
        if let Some(seek) = &self.seek
            && seek.landed(s.pos)
        {
            self.media.seeked(Duration::from_secs_f64(s.pos.max(0.0)));
            self.seek = None;
        }
    }

    fn dispatch(&mut self, ctx: &egui::Context, a: Action) {
        match a {
            Action::Cursor(by) => {
                // The arrows take over from Tab, so Return chooses the cursor's row again.
                // The search field keeps the keyboard.
                if self.nav.search.is_none()
                    && let Some(id) = ctx.memory(|m| m.focused())
                {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
                // The first press only shows where the cursor is; search always shows it.
                let by = if self.keyboard_cursor || self.nav.search.is_some() {
                    by
                } else {
                    0
                };
                self.keyboard_cursor = true;
                let (len, scroll) = match (&mut self.nav.search, &self.nav.open_channel) {
                    (Some(search), _) => (
                        match &search.target {
                            SearchTarget::Channels => match self.library.channels() {
                                Some(Ok(l)) => matching(&search.query, l).len(),
                                _ => 0,
                            },
                            SearchTarget::Tracks(open) => match self.library.tracks(&open.slug) {
                                Some(Ok(l)) => matching_tracks(&search.query, l).len(),
                                _ => 0,
                            },
                            SearchTarget::Tags(open) => match self.library.tracks(&open.slug) {
                                Some(Ok(l)) => {
                                    matching_tags(&search.query, l, self.nav.filter.as_deref())
                                        .len()
                                }
                                _ => 0,
                            },
                        },
                        &mut search.scroll,
                    ),
                    (None, Some(o)) => (
                        match self.library.tracks(&o.slug) {
                            Some(Ok(l)) => shown_tracks(l, self.nav.filter.as_deref()).len(),
                            _ => 0,
                        },
                        &mut self.track_scroll,
                    ),
                    (None, None) => (
                        match self.library.channels() {
                            Some(Ok(l)) => l.len(),
                            _ => 0,
                        },
                        &mut self.channel_scroll,
                    ),
                };
                if len > 0 {
                    // The cursor takes over from a touchpad's glide.
                    self.scrolling.stop();
                    scroll.move_cursor(by, len);
                }
            }
            Action::Choose if !self.nav.show_now_playing && self.nav.search.is_some() => {
                let search = self.nav.search.as_ref().unwrap();
                let action = match &search.target {
                    SearchTarget::Channels => self
                        .library
                        .channels()
                        .and_then(Result::ok)
                        .and_then(|list| {
                            let i = *matching(&search.query, list).get(search.scroll.cursor)?;
                            Some(Action::Open(list[i].slug.clone(), list[i].name.clone()))
                        }),
                    SearchTarget::Tracks(open) => self
                        .library
                        .tracks(&open.slug)
                        .and_then(Result::ok)
                        .and_then(|list| {
                            let i =
                                *matching_tracks(&search.query, list).get(search.scroll.cursor)?;
                            Some(Action::PlayTrack(open.clone(), i))
                        }),
                    SearchTarget::Tags(open) => self
                        .library
                        .tracks(&open.slug)
                        .and_then(Result::ok)
                        .and_then(|list| {
                            let (tag, _, _) =
                                *matching_tags(&search.query, list, self.nav.filter.as_deref())
                                    .get(search.scroll.cursor)?;
                            Some(Action::Tag(tag.to_owned()))
                        }),
                };
                if let Some(action) = action {
                    self.dispatch(ctx, action);
                }
            }
            Action::Choose if !self.nav.show_now_playing => match &self.nav.open_channel {
                Some(o) if self.nav.show_channel_info => open_page(ctx, &o.slug),
                Some(o) => {
                    if let Some(Ok(list)) = self.library.tracks(&o.slug)
                        && let Some(&i) = shown_tracks(list, self.nav.filter.as_deref())
                            .get(self.track_scroll.cursor)
                    {
                        self.dispatch(ctx, Action::Play(i));
                    }
                }
                None => {
                    if let Some(Ok(list)) = self.library.channels()
                        && let Some(c) = list.get(self.channel_scroll.cursor)
                    {
                        let a = Action::Open(c.slug.clone(), c.name.clone());
                        self.dispatch(ctx, a);
                    }
                }
            },
            Action::Choose => {}
            Action::Open(slug, name) => {
                if let Some(Ok(list)) = self.library.channels()
                    && let Some(i) = list.iter().position(|c| c.slug == slug)
                {
                    self.channel_scroll.cursor = i;
                }
                // A glide on the channel list stays there.
                self.scrolling.stop();
                self.library.request_tracks(&slug);
                self.nav.open(ChannelRef { slug, name });
                self.find_playing();
            }
            Action::Back => {
                self.scrolling.stop();
                let narrowed = self.nav.filter.is_some();
                self.nav.back();
                if narrowed && self.nav.filter.is_none() {
                    self.find_playing();
                }
            }
            // The tags stay open; the cursor follows the toggled tag.
            Action::Tag(tag) => {
                self.nav.filter = toggle_tag(self.nav.filter.as_deref(), &tag);
                if let Some(search) = &mut self.nav.search
                    && let SearchTarget::Tags(open) = &search.target
                    && let Some(Ok(list)) = self.library.tracks(&open.slug)
                {
                    search.query.clear();
                    let at = matching_tags("", list, self.nav.filter.as_deref())
                        .iter()
                        .position(|&(t, _, _)| t == tag)
                        .unwrap_or(0);
                    search.scroll.cursor = at;
                    search.scroll.reveal = Some(Reveal::Show(at));
                }
                self.find_playing();
            }
            Action::ClearFilter => {
                self.nav.filter = None;
                self.find_playing();
            }
            Action::PlayChannel(slug, name) => {
                if self.demo {
                    // The demo has one tracklist; borrow it so the playing dot moves to this channel.
                    let s = &mut self.player.state;
                    s.index = Some(player::pick(6, s.index, player::random()));
                    (s.queue, s.channel) = (Some(slug), name);
                    return;
                }
                self.library.request_tracks(&slug);
                self.pending_play = Some(ChannelRef { slug, name });
            }
            Action::Info => self.nav.toggle_info(),
            Action::ShowNow => self.nav.set_now_playing(self.player.state.queue.is_some()),
            Action::Search(target) => {
                self.scrolling.stop();
                let mut search = Search::new(target);
                // Start from the current filter.
                if let (SearchTarget::Tracks(_), Some(filter)) = (&search.target, &self.nav.filter)
                {
                    search.query = format!("{filter} ");
                }
                self.nav.search = Some(search);
            }
            // The query becomes the filter: what you see is what plays.
            Action::PlayTrack(open, index) => {
                let filter = self
                    .nav
                    .search
                    .as_ref()
                    .map(|s| s.query.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|q| !q.is_empty());
                if filter != self.nav.filter
                    && let Some(Ok(list)) = self.library.tracks(&open.slug)
                {
                    let shown = shown_tracks(list, filter.as_deref());
                    self.track_scroll.cursor = shown.iter().position(|&i| i == index).unwrap_or(0);
                    self.track_scroll.reveal = Some(Reveal::Jump);
                    self.nav.filter = filter.clone();
                }
                self.play(&open.slug, &open.name, index, filter);
            }
            Action::Play(index) => {
                if let Some(o) = &self.nav.open_channel {
                    let (slug, name, filter) =
                        (o.slug.clone(), o.name.clone(), self.nav.filter.clone());
                    if let Some(Ok(list)) = self.library.tracks(&slug)
                        && let Some(at) = shown_tracks(list, filter.as_deref())
                            .iter()
                            .position(|&i| i == index)
                    {
                        self.track_scroll.cursor = at;
                    }
                    self.play(&slug, &name, index, filter);
                }
            }
            // Like the iPod's play button on a highlighted playlist: Space plays the channel under the cursor.
            Action::Toggle if let Some(c) = self.cued() => {
                let a = Action::PlayChannel(c.slug.clone(), c.name.clone());
                self.dispatch(ctx, a);
            }
            // Space with nothing loaded starts the open channel at a random track.
            Action::Toggle if self.player.state.queue.is_none() => {
                if let Some(o) = &self.nav.open_channel {
                    let (slug, name, filter) =
                        (o.slug.clone(), o.name.clone(), self.nav.filter.clone());
                    self.shuffle(&slug, &name, filter);
                }
            }
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
                self.player.state.index = Some(i.max(0) as usize);
                self.track_scroll.reveal = Some(Reveal::Glide);
            }
            Action::Send(c) => {
                if let Some(seek) = media::Seek::new(&self.player.state, &c) {
                    self.seek = Some(seek);
                }
                self.player.send(c);
            }
        }
    }

    /// The channel under the list's cursor, while the arrows are in use and it isn't the one playing.
    fn cued(&self) -> Option<&Channel> {
        let Some(Ok(list)) = self.library.channels() else {
            return None;
        };
        let c = list.get(self.channel_scroll.cursor)?;
        let listed = self.keyboard_cursor && self.nav.channel_list_focused();
        (listed && self.player.state.queue.as_deref() != Some(&c.slug)).then_some(c)
    }

    /// A search or tags panel over the open channel.
    fn searching_in_channel(&self) -> bool {
        self.nav
            .search
            .as_ref()
            .is_some_and(|s| matches!(&s.target, SearchTarget::Tracks(_) | SearchTarget::Tags(_)))
    }

    /// Moves the tracklist's cursor to the playing track, or the first.
    fn find_playing(&mut self) {
        let Some(o) = &self.nav.open_channel else {
            return;
        };
        let s = &self.player.state;
        let at = match self.library.tracks(&o.slug) {
            Some(Ok(list)) => s
                .index
                .filter(|_| s.queue.as_deref() == Some(&o.slug))
                .and_then(|i| {
                    shown_tracks(list, self.nav.filter.as_deref())
                        .iter()
                        .position(|&j| j == i)
                }),
            _ => s.index.filter(|_| s.queue.as_deref() == Some(&o.slug)),
        };
        self.track_scroll.reset_glide();
        self.track_scroll.cursor = at.unwrap_or(0);
        self.track_scroll.reveal = Some(match at {
            Some(_) => Reveal::Jump,
            None => Reveal::Show(0),
        });
    }

    /// Plays a random track of the channel, within `filter`, not the one playing.
    fn shuffle(&mut self, slug: &str, name: &str, filter: Option<String>) {
        let Some(Ok(list)) = self.library.tracks(slug) else {
            return;
        };
        let pool = shown_tracks(list, filter.as_deref());
        let s = &self.player.state;
        let now = s
            .index
            .filter(|_| s.queue.as_deref() == Some(slug))
            .and_then(|i| pool.iter().position(|&j| j == i));
        let Some(&index) = pool.get(player::pick(pool.len(), now, player::random())) else {
            return;
        };
        self.play(slug, name, index, filter);
    }

    fn play(&mut self, slug: &str, name: &str, index: usize, filter: Option<String>) {
        // Whatever was asked for last wins over a channel still waiting on its tracks.
        self.pending_play = None;
        // The demo's player stands still, so follow the tap by hand.
        if self.demo {
            self.player.state.index = Some(index);
            self.player.state.filter = filter;
            if !self.searching_in_channel() {
                self.track_scroll.reveal = Some(Reveal::Glide);
            }
            return;
        }
        let Some(Ok(list)) = self.library.tracks(slug) else {
            return;
        };
        if index >= list.len() {
            return;
        }
        let pool = shown_tracks(list, filter.as_deref());
        self.player.send(Command::Play {
            queue: slug.to_string(),
            filter,
            pool,
            channel: name.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::Track;
    use std::sync::mpsc;
    #[test]
    fn later_track_selection_cancels_earlier_overview_play() {
        let ctx = egui::Context::default();
        let (ready, loading) = mpsc::channel();
        let mut app = App {
            library: Library::fixed(
                Vec::new(),
                HashMap::from([(
                    "b".into(),
                    vec![Track {
                        title: Some("B track".into()),
                        url: "/tmp/missing-b.wav".into(),
                        provider: None,
                        ..Default::default()
                    }],
                )]),
            ),
            nav: Nav {
                open_channel: Some(ChannelRef {
                    slug: "b".into(),
                    name: "B".into(),
                }),
                ..Nav::default()
            },
            pending_play: None,
            player: Player::spawn(&[("ao", "null")], || {}),
            media: NowPlaying::start(
                media::App::new(&format!("radio4000.test_{}", std::process::id()), "test"),
                || {},
            ),
            seek: None,
            scrub: None,
            volume: None,
            track_scroll: ListScroll::default(),
            channel_scroll: ListScroll::default(),
            scrolling: fastframe_scroll::Scrolling::default(),
            keyboard_cursor: false,
            actions: Vec::new(),
            demo: false,
            theme: crate::theme::Theme::new(&ctx, true),
            memory: crate::window::Memory::load(true),
            summons: Arc::default(),
        };
        app.library.insert_pending_tracks("a", loading);
        app.dispatch(&ctx, Action::PlayChannel("a".into(), "A".into()));
        app.dispatch(&ctx, Action::Play(0));
        ready
            .send(Ok(vec![Track {
                title: Some("A track".into()),
                url: "/tmp/missing-a.wav".into(),
                provider: None,
                ..Default::default()
            }]))
            .unwrap();
        app.poll_updates();
        // The player runs commands in order, so once it reports this volume, every play sent before it has landed.
        app.player.send(Command::Volume(42.0));
        let start = Instant::now();
        while app.player.state.volume != 42.0 {
            assert!(start.elapsed().as_secs() < 10, "the player never answered");
            std::thread::sleep(Duration::from_millis(10));
            app.poll_updates();
        }
        assert_eq!(
            app.player.state.queue.as_deref(),
            Some("b"),
            "an old overview request replaced the track selected afterwards"
        );
    }
}
