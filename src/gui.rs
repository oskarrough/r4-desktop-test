//! The window shown when the app opens without a URL: R4 channels, newest activity first.
use crate::channels::{self, Channel};
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
            Ok(Box::new(App { rx, channels: None }))
        }),
    )
}

struct App {
    rx: mpsc::Receiver<Result<Vec<Channel>, String>>,
    channels: Option<Result<Vec<Channel>, String>>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Ok(result) = self.rx.try_recv() {
            self.channels = Some(result);
        }
        egui::CentralPanel::default().show(ui, |ui| {
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
                                ui.strong(&c.name);
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
    }
}
