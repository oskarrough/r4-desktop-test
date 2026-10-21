//! Virtualized lists, empty-state notes, and keyboard cursor and reveal animation.
use super::navigation::ease;
use super::transport::DOCK;
use super::widgets::{AVATAR, ICON, Icon, TOP, chevron, face, fit, put};
use crate::style::{BODY, MARGIN, Palette, UNIT, font, palette};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, Rect, Response, Sense, Ui, pos2, vec2};
use fastframe_fonts::Weight;
use std::time::{Duration, Instant};

/// A list row.
const ROW: f32 = 8.0 * UNIT;
/// How long the tracklist takes to glide to the playing track.
const GLIDE: Duration = Duration::from_millis(240);

/// How the tracklist brings the playing track to its middle: jumping there when a channel opens,
/// gliding there when the track changes, so you see where it went.
#[derive(Default)]
pub(super) struct ListScroll {
    pub(super) reveal: Option<Reveal>,
    glide: Option<Glide>,
    /// The row the arrow keys are on.
    pub(super) cursor: usize,
    /// Where the list was drawn last frame.
    offset: f32,
}

#[derive(Clone, Copy)]
pub(super) enum Reveal {
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

/// The mark at a row's right end.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Trail {
    None,
    Opens,
    On,
}

/// One row of a list: the name, a quieter `note` after it, then a playing dot or the `trail`.
/// `avatar` gives the row an avatar slot at the margin: the channel's image, or its initial when it has none.
/// The avatar is a play button of its own, showing so while the row is pointed at or focused; its response comes second.
pub(super) fn row(
    ui: &mut Ui,
    label: &str,
    note: Option<&str>,
    avatar: Option<Option<&str>>,
    selected: bool,
    playing: bool,
    trail: Trail,
) -> (Response, Option<Response>) {
    let pal = palette(ui);
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    if !ui.is_rect_visible(rect) {
        return (r, None);
    }
    // The playing track is white.
    let lit = playing && avatar.is_none();
    let fill = if lit {
        Color32::WHITE
    } else if selected || (avatar.is_some() && r.has_focus()) {
        pal.selected
    } else if avatar.is_none() && r.has_focus() {
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
    } else if trail == Trail::Opens {
        chevron(ui, pos2(right, cy), 1.0, 4.0, pal.disabled);
        right -= 4.0 + 2.0 * UNIT;
    } else if trail == Trail::On {
        Icon::Check.image(pal.text, ICON).paint_at(
            ui,
            Rect::from_center_size(pos2(right - ICON / 2.0, cy), vec2(ICON, ICON)),
        );
        right -= ICON + 2.0 * UNIT;
    }
    let mut x = rect.left() + MARGIN;
    let mut play = None;
    if let Some(image) = avatar {
        let at = Rect::from_min_size(pos2(x, cy - AVATAR / 2.0), vec2(AVATAR, AVATAR));
        face(ui, at, image, label);
        // The hit is the row's full height, so the button is as easy to reach as the row.
        let hit = Rect::from_center_size(at.center(), vec2(ROW, ROW));
        let p = ui.interact(hit, r.id.with("play"), Sense::click());
        if ui.rect_contains_pointer(rect) || r.has_focus() || p.has_focus() {
            let lit = p.hovered() || p.has_focus();
            let scrim = if lit { 0.8 } else { 0.6 };
            ui.painter()
                .circle_filled(at.center(), AVATAR / 2.0, pal.bg.gamma_multiply(scrim));
            let size = AVATAR / 2.0;
            Icon::Play
                .image(if lit { pal.text } else { pal.secondary }, size)
                .paint_at(ui, Rect::from_center_size(at.center(), vec2(size, size)));
        }
        play = Some(p);
        x += AVATAR + 2.0 * UNIT;
    }
    let (ink, quiet) = if lit {
        (Palette::dark().bg, Palette::dark().secondary)
    } else {
        (pal.text, pal.secondary)
    };
    let line = match note {
        Some(note) => {
            let body = font(BODY, Weight::Regular);
            let mut job = LayoutJob::default();
            job.append(label, 0.0, TextFormat::simple(body.clone(), ink));
            job.append(note, 2.0 * UNIT, TextFormat::simple(body, quiet));
            job.wrap = TextWrapping::truncate_at_width((right - x).max(0.0));
            ui.painter().layout_job(job)
        }
        None => fit(ui, label, font(BODY, Weight::Regular), ink, right - x),
    };
    put(ui, line, x, cy);
    (r, play)
}

/// Draws the visible rows below the top band, bringing `target` to the middle when requested.
pub(super) fn rows(
    ui: &mut Ui,
    n: usize,
    target: Option<usize>,
    sc: &mut ListScroll,
    mut row: impl FnMut(&mut Ui, usize),
) {
    let inset = TOP;
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
        let eased = ease(t);
        area = area.vertical_scroll_offset(g.from + (g.to - g.from) * eased);
        if t < 1.0 {
            ui.ctx().request_repaint();
        } else {
            sc.glide = None;
        }
    }
    sc.offset = area.show_viewport(ui, |ui, vp| draw(ui, vp)).state.offset.y;
}

impl ListScroll {
    pub(super) fn reset_glide(&mut self) {
        self.glide = None;
    }
    pub(super) fn move_cursor(&mut self, by: isize, len: usize) {
        if len == 0 {
            return;
        }
        self.cursor = self.cursor.saturating_add_signed(by).min(len - 1);
        self.reveal = Some(Reveal::Show(self.cursor));
    }
}

/// A quiet line where a list would be: loading, empty, or what went wrong.
pub(super) fn note(ui: &mut Ui, text: &str) {
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
