//! The window shown when the app opens without a URL: R4 channels, newest activity first.
use crate::channels::{self, Channel};
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
            }))
        }),
    )
}

struct App {
    rx: mpsc::Receiver<Result<Vec<Channel>, String>>,
    channels: Option<Result<Vec<Channel>, String>>,
    open: Option<Open>,
}

/// The channel being looked at, with its tracks once they arrive.
struct Open {
    name: String,
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
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(open) = &self.open {
                ui.horizontal(|ui| {
                    back = ui.button("< Channels").clicked();
                    ui.heading(&open.name);
                });
                match &open.tracks {
                    None => {
                        ui.spinner();
                    }
                    Some(Err(e)) => {
                        ui.label(format!("Could not load tracks: {e}"));
                    }
                    Some(Ok(list)) => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for t in list {
                                ui.label(t.title.as_deref().unwrap_or(&t.url));
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
        if back {
            self.open = None;
        }
        if let Some(c) = clicked {
            self.open = Some(Open::new(ui.ctx(), c));
        }
    }
}
