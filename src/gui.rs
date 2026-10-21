//! The window shown when the app opens without a URL: R4 channels, newest activity first.
use crate::channels::{self, Channel};
use crate::player::Player;
use crate::tracks::{self, Track};
use std::sync::mpsc;

pub fn run() -> eframe::Result {
    eframe::run_native(
        "Radio4000",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            let mut fonts = fastframe_fonts::FontSetup::default().definitions();
            let rendering = fastframe_text::detect();
            rendering.apply_to(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx
                .all_styles_mut(|style| rendering.apply_to_visuals(&mut style.visuals));

            let (tx, rx) = mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(channels::fetch());
                ctx.request_repaint();
            });
            Ok(Box::new(App {
                rx,
                channels: None,
                open: None,
                player: None,
            }))
        }),
    )
}

struct App {
    rx: mpsc::Receiver<Result<Vec<Channel>, String>>,
    channels: Option<Result<Vec<Channel>, String>>,
    open: Option<Open>,
    /// Created on the first play, so browsing works even without libmpv.
    player: Option<Result<Player, String>>,
}

/// The channel being looked at, with its tracks once they arrive.
struct Open {
    name: String,
    slug: String,
    rx: mpsc::Receiver<Result<Vec<Track>, String>>,
    tracks: Option<Result<Vec<Track>, String>>,
}

impl Open {
    fn new(ctx: &egui::Context, c: &Channel) -> Self {
        let (tx, rx) = mpsc::channel();
        let (ctx, slug) = (ctx.clone(), c.slug.clone());
        std::thread::spawn(move || {
            let _ = tx.send(tracks::fetch(&slug));
            ctx.request_repaint();
        });
        Open {
            name: c.name.clone(),
            slug: c.slug.clone(),
            rx,
            tracks: None,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Ok(result) = self.rx.try_recv() {
            self.channels = Some(result);
        }
        if let Some(open) = &mut self.open
            && let Ok(result) = open.rx.try_recv()
        {
            open.tracks = Some(result);
        }
        let mut back = false;
        let mut clicked = None;
        let mut play = None;
        let player = match &self.player {
            Some(Ok(p)) => Some(p),
            _ => None,
        };
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(open) = &self.open {
                ui.horizontal(|ui| {
                    back = ui.button("< Channels").clicked();
                    ui.heading(&open.name);
                });
                if let Some(Err(e)) = &self.player {
                    ui.label(format!("Could not start playback: {e}"));
                }
                let current = player
                    .filter(|p| p.queue.as_deref() == Some(&open.slug))
                    .and_then(|p| p.current());
                if let (Some(p), Some(_)) = (player, current) {
                    ui.horizontal(|ui| {
                        if ui.button("Prev").clicked() {
                            p.prev();
                        }
                        if ui
                            .button(if p.paused() { "Play" } else { "Pause" })
                            .clicked()
                        {
                            p.toggle_pause();
                        }
                        if ui.button("Next").clicked() {
                            p.next();
                        }
                    });
                    if let Some((mut pos, dur)) = p.progress() {
                        ui.horizontal(|ui| {
                            ui.label(clock(pos));
                            ui.spacing_mut().slider_width = ui.available_width() - 50.0;
                            let bar = egui::Slider::new(&mut pos, 0.0..=dur).show_value(false);
                            if ui.add(bar).changed() {
                                p.seek(pos);
                            }
                            ui.label(clock(dur));
                        });
                    }
                }
                match &open.tracks {
                    None => {
                        ui.spinner();
                    }
                    Some(Err(e)) => {
                        ui.label(format!("Could not load tracks: {e}"));
                    }
                    Some(Ok(list)) => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (i, t) in list.iter().enumerate() {
                                let title = t.title.as_deref().unwrap_or(&t.url);
                                if ui.selectable_label(current == Some(i), title).clicked() {
                                    play = Some(i);
                                }
                            }
                        });
                    }
                }
                return;
            }
            ui.heading("Radio4000");
            match &self.channels {
                None => {
                    ui.spinner();
                }
                Some(Err(e)) => {
                    ui.label(format!("Could not load channels: {e}"));
                }
                Some(Ok(list)) => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for c in list {
                            ui.horizontal(|ui| {
                                if ui.link(&c.name).clicked() {
                                    clicked = Some(c);
                                }
                                ui.weak(format!("@{}", c.slug));
                                if let Some(at) = &c.latest_track_at {
                                    ui.weak(at.get(..10).unwrap_or(at));
                                }
                            });
                        }
                    });
                }
            }
        });
        if let Some(i) = play
            && let Some(open) = &self.open
            && let Some(Ok(list)) = &open.tracks
        {
            let player = self.player.get_or_insert_with(Player::new);
            if let Ok(p) = player {
                let urls: Vec<&str> = list.iter().map(|t| t.url.as_str()).collect();
                p.play(&open.slug, &urls, i);
            }
        }
        // mpv moves on by itself; look again so the highlight follows it.
        if self.player.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
        }
        if back {
            self.open = None;
        }
        if let Some(c) = clicked {
            self.open = Some(Open::new(ui.ctx(), c));
        }
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
