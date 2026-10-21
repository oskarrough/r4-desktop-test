//! The dock, now-playing view, seeking, and volume feedback.
use super::Action;
use super::widgets::{
    HIT, ICON, Icon, KNOB, TOP, arc, channel_image, close, face, fit, icon_button,
    icon_button_sized, knob, put, put_top, squircle, squircle_top,
};
use crate::player::{Command, State};
use crate::style::{BODY, LARGE, MARGIN, SMALL, UNIT, font, palette};
use egui::text::LayoutJob;
use egui::{Color32, Galley, Pos2, Rect, Sense, Ui, pos2, vec2};
use fastframe_fonts::Weight;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The mini player's height; it floats this far plus a gap above the bottom.
pub(super) const DOCK: f32 = 12.0 * UNIT;
/// The mini player's corners: superellipses this large, softer than a circle's quarter.
const SQUIRCLE: f32 = 6.0 * UNIT;
/// How long the volume meter stays up after a change, and how long it then takes to fade.
const METER: Duration = Duration::from_millis(1200);
const FADE: Duration = Duration::from_millis(300);
/// Volume per arrow press; a notch of the wheel moves about as much.
pub(super) const STEP: f64 = 5.0;
/// The progress line on the transport's top edge, thicker under the pointer.
const LINE: f32 = 2.0;

/// Where the seek bar points while it is dragged, and after, until mpv has caught up.
pub(super) struct Scrub {
    pub(super) to: f64,
    pub(super) dragging: bool,
    pub(super) at: Instant,
}

/// A change of volume, ahead of mpv reporting it.
#[derive(Clone, Copy)]
pub(super) struct Volume {
    pub(super) to: f64,
    pub(super) at: Instant,
}

impl Volume {
    /// How visible the meter is: fully for `METER`, then fading out over `FADE`.
    pub(super) fn opacity(&self) -> f32 {
        let t = self.at.elapsed().saturating_sub(METER);
        1.0 - (t.as_secs_f32() / FADE.as_secs_f32()).min(1.0)
    }
}

/// The mini player: a capsule floating over the bottom of the list, with the channel's face, what's on, play and
/// next, and the track's progress along its top edge. Touching it opens the now-playing view. Hidden while nothing is loaded.
pub(super) fn dock(
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

    // The top edge is the seek line, from halfway round one corner to halfway round the other: it fills with the
    // track and thickens to be dragged.
    let lip = round * (1.0 - std::f32::consts::FRAC_1_SQRT_2.sqrt());
    let edge = Rect::from_x_y_ranges(
        rect.left() + lip..=rect.right() - lip,
        rect.top()..=rect.top() + 2.0 * UNIT,
    );
    let (frac, live) = seek(ui, edge, s, scrub, actions);
    edge_line(ui, rect, round, frac, if live { 2.0 * LINE } else { LINE });

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
    let sub = format!("{}{time}", s.station());
    put(
        ui,
        fit(ui, &sub, font(SMALL, Weight::Regular), pal.secondary, width),
        x,
        cy + 2.0 * UNIT,
    );
}

/// The now-playing view, laid out like Apple's: title and channel at the margin, the seek bar with its times,
/// then previous, play and next, the whole group centred in the window.
pub(super) fn now_playing(
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
    close(ui, pos2(rect.left() + 2.0 * UNIT + KNOB / 2.0, cy), actions);
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
        &s.station(),
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
        ui.painter().rect_filled(at, 0.0, pal.surface);
        channel_image(ui, at, id, 0.0);
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
    let (frac, live) = seek(ui, hit, s, scrub, actions);
    bar(ui, hit, frac, if live { 4.0 * LINE } else { 2.0 * LINE });
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

/// The progress of the track, seekable anywhere in `hit`, left to right. How far along it is and whether the pointer
/// is on it, for the caller to draw. Seeks once it is let go, not on every step of a drag.
fn seek(
    ui: &mut Ui,
    hit: Rect,
    s: &State,
    scrub: &mut Option<Scrub>,
    actions: &mut Vec<Action>,
) -> (f32, bool) {
    let r = ui.interact(hit, ui.id().with("seek"), Sense::click_and_drag());
    let dur = s.dur.unwrap_or(0.0);
    if dur <= 0.0 {
        return (0.0, false);
    }
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
    let frac = (shown(s, scrub) / dur).clamp(0.0, 1.0) as f32;
    (frac, r.hovered() || r.dragged())
}

/// The dock's seek line: its top edge, curving down into both corners, filled `frac` of the way across.
fn edge_line(ui: &Ui, rect: Rect, round: f32, frac: f32, thick: f32) {
    let pal = palette(ui);
    let path = squircle_top(rect.shrink(thick / 2.0), round - thick / 2.0);
    let (first, last) = (path[0], path[path.len() - 1]);
    let to = first.x + (last.x - first.x) * frac;
    let mut done: Vec<Pos2> = path.iter().copied().take_while(|p| p.x < to).collect();
    if let Some(i) = path.iter().position(|p| p.x >= to)
        && i > 0
    {
        let (a, b) = (path[i - 1], path[i]);
        done.push(a.lerp(b, (to - a.x) / (b.x - a.x)));
    }
    ui.painter()
        .line(path, egui::Stroke::new(thick, pal.hairline));
    if done.len() > 1 {
        ui.painter()
            .line(done, egui::Stroke::new(thick, pal.accent));
    }
}

/// The now-playing view's seek bar: a rounded rail across the middle of `hit`, filled `frac` of the way.
fn bar(ui: &Ui, hit: Rect, frac: f32, thick: f32) {
    let pal = palette(ui);
    let rail = Rect::from_center_size(hit.center(), vec2(hit.width(), thick));
    let mut done = rail;
    done.set_right(rail.left() + rail.width() * frac);
    ui.painter().rect_filled(rail, thick / 2.0, pal.hairline);
    ui.painter().rect_filled(done, thick / 2.0, pal.accent);
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
