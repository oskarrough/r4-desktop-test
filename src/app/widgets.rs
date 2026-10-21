//! Shared text, artwork, headers, and drawing primitives.
use super::Action;
use super::browse::SearchTarget;
use super::navigation::{ChannelRef, snap};
use crate::channels;
use crate::style::{BODY, CAP, SMALL, UNIT, font, palette};
use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, FontId, Galley, Pos2, Rect, Response, Sense, Ui, pos2, vec2};
use fastframe_fonts::Weight;
use std::sync::Arc;

/// The band at the top that the round buttons and the title pill float in; lists start below it and scroll under it.
pub(super) const TOP: f32 = 12.0 * UNIT;
/// A round button, and the height of the title pill.
pub(super) const KNOB: f32 = 8.0 * UNIT;
pub(super) const AVATAR: f32 = 6.0 * UNIT;
/// Icons, and the square each one answers clicks in.
pub(super) const ICON: f32 = 16.0;
pub(super) const HIT: f32 = 8.0 * UNIT;
const SQUIRCLE_POWER: f32 = 4.0;

fastframe_icons::icons! {
    /// Every icon the window draws.
    pub(super) enum Icon {
        prefix: "radio4000-icon-",
        directory: "../../assets/icons/",
        Play => "play-filled",
        Pause => "pause-filled",
        Prev => "skip-back-filled",
        Next => "skip-forward-filled",
        Speaker => lucide "volume-2",
        Muted => lucide "volume-x",
        Search => lucide "search",
        Back => lucide "chevron-left",
    }
}

/// A rectangle with superellipse corners, filled, with a hairline edge just inside it.
/// The edge is the outer shape showing round a 1px smaller inner one: two fills, no stroke joins to spike.
pub(super) fn squircle(ui: &Ui, rect: Rect, r: f32, fill: Color32, edge: Color32) {
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
pub(super) fn arc(ui: &Ui, c: Pos2, radius: f32, frac: f32, stroke: egui::Stroke) {
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

/// What floats over the top of a list: a fade, back and search buttons,
/// and the title in a pill with the channel's avatar, which opens the channel's details.
/// `back` is how far the channel has slid over the channel list, when it does: the buttons hold still meanwhile,
/// search glides aside, back shows in the room it leaves, then the title drifts in; leaving runs it in reverse.
pub(super) fn chrome(
    ui: &mut Ui,
    rect: Rect,
    open: &ChannelRef,
    image: Option<&str>,
    back_button: Option<f32>,
    actions: &mut Vec<Action>,
) {
    fade(ui, rect);
    let shown = back_button.unwrap_or(1.0);
    let rect = rect.translate(-snap(ui, vec2((1.0 - shown) * rect.width(), 0.0)));
    // Over the whole pane, not just the part slid in so far.
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        if back_button.is_some() {
            ui.set_clip_rect(rect);
        }
        let side = 2.0 * UNIT + KNOB / 2.0;
        let cy = rect.top() + side;
        let home = rect.left() + side;
        let room = if back_button.is_some() {
            KNOB + UNIT
        } else {
            0.0
        };
        if back_button.is_some() {
            faded(ui, stage(shown, 0.15, 0.75), |ui| {
                back(ui, pos2(home, cy), actions)
            });
        }
        let c = pos2(home + snap(ui, vec2(shown * room, 0.0)).x, cy);
        let pal = palette(ui);
        let search = knob(ui, c, "search");
        Icon::Search
            .image(pal.text, ICON)
            .paint_at(ui, Rect::from_center_size(c, vec2(ICON, ICON)));
        if search.on_hover_text("Search tracks (/)").clicked() {
            actions.push(Action::Search(SearchTarget::Tracks(open.clone())));
        }
        let left = home + room + KNOB / 2.0 + 2.0 * UNIT;
        let right = rect.right() - 2.0 * UNIT;
        let arrived = stage(shown, 0.35, 1.0);
        let drift = snap(ui, vec2((1.0 - arrived) * 4.0 * UNIT, 0.0)).x;
        let r = faded(ui, arrived, |ui| {
            pill(
                ui,
                pos2((left + right) / 2.0 + drift, cy),
                &open.name,
                image,
                (right - left).max(0.0),
            )
        })
        .flatten();
        if r.is_some_and(|r| r.on_hover_text("About this channel (i)").clicked()) {
            actions.push(Action::Info);
        }
    });
}

/// How far along from `a` to `b` a transition at `t` is, from 0 to 1, so parts of a view can follow one another.
fn stage(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

/// Draws with `f` at this opacity; nothing at all, nor anything to click, while fully faded.
fn faded<R>(ui: &mut Ui, opacity: f32, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    if opacity <= 0.0 {
        return None;
    }
    let inner = ui.scope_builder(egui::UiBuilder::new().max_rect(ui.max_rect()), |ui| {
        ui.multiply_opacity(opacity);
        f(ui)
    });
    Some(inner.inner)
}

/// The band at the top that what's below scrolls under, fading to the background.
pub(super) fn fade(ui: &Ui, rect: Rect) {
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
}

/// A round button that floats over what's under it.
pub(super) fn knob(ui: &mut Ui, c: Pos2, id: &str) -> Response {
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

pub(super) fn back(ui: &mut Ui, c: Pos2, actions: &mut Vec<Action>) {
    let pal = palette(ui);
    let r = knob(ui, c, "back");
    Icon::Back
        .image(pal.text, 20.0)
        .paint_at(ui, Rect::from_center_size(c, vec2(20.0, 20.0)));
    if r.on_hover_text("Back (esc)").clicked() {
        actions.push(Action::Back);
    }
}

/// Back's twin on now playing, which slides down rather than back: an X.
pub(super) fn close(ui: &mut Ui, c: Pos2, actions: &mut Vec<Action>) {
    let pal = palette(ui);
    let r = knob(ui, c, "back");
    let stroke = egui::Stroke::new(1.5, pal.text);
    for d in [vec2(4.0, 4.0), vec2(4.0, -4.0)] {
        ui.painter().line_segment([c - d, c + d], stroke);
    }
    if r.on_hover_text("Close (esc)").clicked() {
        actions.push(Action::Back);
    }
}

/// The title in a pill centred on `c`, led by the avatar when there is one. Pressable, like the knobs.
fn pill(ui: &mut Ui, c: Pos2, title: &str, image: Option<&str>, max: f32) -> Option<Response> {
    let pal = palette(ui);
    if title.is_empty() {
        return None;
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
    let r = ui.interact(rect, ui.id().with("pill"), Sense::click());
    let fill = if r.hovered() || r.has_focus() {
        pal.selected
    } else {
        pal.surface
    };
    ui.painter().rect(
        rect,
        KNOB / 2.0,
        fill,
        egui::Stroke::new(1.0, pal.edge),
        egui::StrokeKind::Inside,
    );
    if image.is_some() {
        let at = Rect::from_min_size(pos2(left + UNIT, c.y - AVATAR / 2.0), vec2(AVATAR, AVATAR));
        face(ui, at, image, title);
    }
    put(ui, g, left + lead, c.y);
    Some(r)
}

/// A channel's avatar in a circle, or its first letter while there's no picture.
pub(super) fn face(ui: &Ui, at: Rect, image: Option<&str>, name: &str) {
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
        channel_image(ui, at, id, r);
    }
}

/// Paints loaded channel artwork, leaving its placeholder intact while loading or on failure.
pub(super) fn channel_image(ui: &Ui, at: Rect, id: &str, corner_radius: f32) {
    if id.trim().is_empty() {
        return;
    }
    let px = (at.width() * ui.pixels_per_point()).round() as u32;
    if let Ok(egui::load::TexturePoll::Ready { texture }) =
        egui::Image::new(channels::avatar_url(id, px)).load_for_size(ui.ctx(), at.size())
    {
        egui::Image::from_texture(texture)
            .corner_radius(corner_radius)
            .paint_at(ui, at);
    }
}

/// One line of text, cut short with an ellipsis past `width`.
pub(super) fn fit(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping::truncate_at_width(width.max(0.0));
    ui.painter().layout_job(job)
}

/// Paints a line with the ink of its first letter at `x` and its capitals centred on `y`, its baseline on a whole pixel.
pub(super) fn put(ui: &Ui, g: Arc<Galley>, x: f32, y: f32) -> Rect {
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
pub(super) fn put_top(ui: &Ui, g: Arc<Galley>, x: f32, y: f32) -> Rect {
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
pub(super) fn ink_width(g: &Galley) -> f32 {
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
pub(super) fn chevron(ui: &Ui, tip: Pos2, dir: f32, h: f32, color: Color32) {
    // Pull the point in by half the stroke, so the ink and not the line's centre meets the edge.
    let tip = tip - vec2(dir * 0.75, 0.0);
    let arm = vec2(-dir * h, h);
    ui.painter().line(
        vec![tip + vec2(arm.x, -arm.y), tip, tip + arm],
        egui::Stroke::new(1.5, color),
    );
}

/// An icon centred on `c`, answering clicks in a `HIT` square, with a soft circle under the pointer.
pub(super) fn icon_button(ui: &mut Ui, icon: Icon, c: Pos2, size: f32, enabled: bool) -> Response {
    icon_button_sized(ui, icon, c, size, HIT, enabled)
}

pub(super) fn icon_button_sized(
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
