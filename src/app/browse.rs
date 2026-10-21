//! Channel browsing, search, details, and track lists.
use super::Action;
use super::list::{ListScroll, Reveal, note, row, rows};
use super::navigation::ChannelRef;
use super::transport::DOCK;
use super::widgets::{
    ICON, Icon, KNOB, TOP, back, chrome, face, fade, fit, ink_width, knob, put, put_top,
};
use crate::channels::{self, Channel};
use crate::player::State;
use crate::style::{BODY, LARGE, MARGIN, SMALL, UNIT, font, palette};
use crate::tracks::Track;
use egui::text::LayoutJob;
use egui::{Rect, Sense, Ui, pos2, vec2};
use fastframe_fonts::Weight;

/// What's typed in the search panel, and its list's scroll and cursor.
#[derive(Default)]
pub(super) struct Search {
    pub(super) query: String,
    pub(super) scroll: ListScroll,
}

pub(super) fn title(t: &Track) -> &str {
    t.title.as_deref().unwrap_or(&t.url)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn channel_list(
    ui: &mut Ui,
    channels: Option<Result<&[Channel], &str>>,
    open: Option<&str>,
    playing: Option<&str>,
    opens: bool,
    scroll: &mut ListScroll,
    keyed: bool,
    actions: &mut Vec<Action>,
) {
    let list = match channels {
        None => return note(ui, "Loading…"),
        Some(Err(e)) => return note(ui, &format!("Could not load channels: {e}")),
        Some(Ok(list)) => list,
    };
    let rect = ui.max_rect();
    let cursor = keyed.then_some(scroll.cursor);
    rows(ui, list.len(), None, scroll, |ui, i| {
        let c = &list[i];
        let selected = open == Some(c.slug.as_str()) || cursor == Some(i);
        channel_row(ui, c, selected, playing, opens, actions);
    });
    fade(ui, rect);
    let side = 2.0 * UNIT + KNOB / 2.0;
    let c = pos2(rect.left() + side, rect.top() + side);
    let r = knob(ui, c, "search");
    let pal = palette(ui);
    Icon::Search
        .image(pal.text, ICON)
        .paint_at(ui, Rect::from_center_size(c, vec2(ICON, ICON)));
    if r.on_hover_text("Search (/)").clicked() {
        actions.push(Action::Search);
    }
}

fn channel_row(
    ui: &mut Ui,
    c: &Channel,
    selected: bool,
    playing: Option<&str>,
    opens: bool,
    actions: &mut Vec<Action>,
) {
    let (r, play) = row(
        ui,
        &c.name,
        Some(c.image.as_deref()),
        selected,
        playing == Some(&c.slug),
        opens,
    );
    if r.clicked() {
        actions.push(Action::Open(c.slug.clone(), c.name.clone()));
    }
    if play.is_some_and(|p| p.on_hover_text("Play, shuffled").clicked()) {
        actions.push(Action::PlayChannel(c.slug.clone(), c.name.clone()));
    }
}

/// The search panel: a back button and the field floating at the top, the channels that match under them.
/// The cursor always shows, so return opens the first match straight away.
pub(super) fn search_panel(
    ui: &mut Ui,
    search: &mut Search,
    channels: Option<Result<&[Channel], &str>>,
    playing: Option<&str>,
    actions: &mut Vec<Action>,
) {
    let pal = palette(ui);
    let rect = ui.max_rect();
    match channels {
        None => note(ui, "Loading…"),
        Some(Err(e)) => note(ui, &format!("Could not load channels: {e}")),
        Some(Ok(list)) => {
            let hits = matching(&search.query, list);
            if hits.is_empty() {
                note(ui, "No channels match.");
            } else {
                search.scroll.cursor = search.scroll.cursor.min(hits.len() - 1);
                let cursor = search.scroll.cursor;
                rows(ui, hits.len(), None, &mut search.scroll, |ui, i| {
                    channel_row(ui, &list[hits[i]], cursor == i, playing, true, actions);
                });
            }
        }
    }

    fade(ui, rect);
    let side = 2.0 * UNIT + KNOB / 2.0;
    let cy = rect.top() + side;
    back(ui, pos2(rect.left() + side, cy), actions);
    let field = Rect::from_x_y_ranges(
        rect.left() + 2.0 * side..=rect.right() - 2.0 * UNIT,
        cy - KNOB / 2.0..=cy + KNOB / 2.0,
    );
    ui.painter().rect(
        field,
        KNOB / 2.0,
        pal.surface,
        egui::Stroke::new(1.0, pal.edge),
        egui::StrokeKind::Inside,
    );
    let icon = pos2(field.left() + 2.0 * UNIT + ICON / 2.0, cy);
    Icon::Search
        .image(pal.secondary, ICON)
        .paint_at(ui, Rect::from_center_size(icon, vec2(ICON, ICON)));
    let text = Rect::from_x_y_ranges(
        icon.x + ICON / 2.0 + 2.0 * UNIT..=field.right() - 3.0 * UNIT,
        field.y_range(),
    );
    let edit = egui::TextEdit::singleline(&mut search.query)
        .frame(egui::Frame::NONE)
        .margin(0.0)
        .vertical_align(egui::Align::Center)
        .font(font(BODY, Weight::Regular))
        .text_color(pal.text)
        .hint_text(egui::RichText::new("Search channels").color(pal.secondary))
        .desired_width(text.width());
    let r = ui.put(text, edit);
    // The field is the panel's only input: it keeps the keyboard.
    if !r.has_focus() {
        r.request_focus();
    }
    // The list was drawn for the old query; draw it again for the new one.
    if r.changed() {
        search.scroll.cursor = 0;
        search.scroll.reveal = Some(Reveal::Show(0));
        ui.ctx().request_repaint();
    }
}

/// The channels whose name or slug holds every word typed, best first: those starting with the whole query, then those
/// where every word starts a word, then the rest. Within each, the list's own order, latest track first.
/// No query finds every channel.
pub(super) fn matching(query: &str, list: &[Channel]) -> Vec<usize> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut hits: Vec<(u8, usize)> = list
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = c.name.to_lowercase();
            let slug = c.slug.to_lowercase();
            if !words.iter().all(|w| name.contains(w) || slug.contains(w)) {
                return None;
            }
            let whole = words.join(" ");
            let starts = |w: &str| {
                name.split(|ch: char| !ch.is_alphanumeric())
                    .chain(slug.split(|ch: char| !ch.is_alphanumeric()))
                    .any(|part| part.starts_with(w))
            };
            let rank = if name.starts_with(&whole) || slug.starts_with(&whole) {
                0
            } else if words.iter().all(|w| starts(w)) {
                1
            } else {
                2
            };
            Some((rank, i))
        })
        .collect();
    hits.sort_by_key(|&(rank, _)| rank);
    hits.into_iter().map(|(_, i)| i).collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn tracklist(
    ui: &mut Ui,
    open: &ChannelRef,
    image: Option<&str>,
    tracks: Option<Result<&[Track], &str>>,
    s: &State,
    back: bool,
    scroll: &mut ListScroll,
    keyed: bool,
    actions: &mut Vec<Action>,
) {
    let rect = ui.max_rect();
    match tracks {
        None => note(ui, "Loading…"),
        Some(Err(e)) => note(ui, &format!("Could not load tracks: {e}")),
        Some(Ok(list)) => {
            let current = s.index.filter(|_| s.queue.as_deref() == Some(&open.slug));
            let cursor = keyed.then_some(scroll.cursor);
            rows(ui, list.len(), current, scroll, |ui, i| {
                let selected = cursor == Some(i);
                if row(
                    ui,
                    title(&list[i]),
                    None,
                    selected,
                    current == Some(i),
                    false,
                )
                .0
                .clicked()
                {
                    actions.push(Action::Play(i));
                }
            });
        }
    }
    chrome(ui, rect, &open.name, image, back, actions);
}

/// The channel's page on radio4000.com, in the browser.
pub(super) fn open_page(ctx: &egui::Context, slug: &str) {
    ctx.open_url(egui::OpenUrl::new_tab(format!(
        "https://{}",
        channels::page(slug)
    )));
}

/// The open channel's details, over its tracklist: its face, name and figures, its own words, and a link to its
/// page, centred in the room between the top band and the mini player, scrolling when the words run long.
pub(super) fn channel_info(
    ui: &mut Ui,
    open: &ChannelRef,
    channel: Option<&Channel>,
    actions: &mut Vec<Action>,
) {
    let pal = palette(ui);
    let rect = ui.max_rect();
    let width = rect.width() - 2.0 * MARGIN;
    let side = 20.0 * UNIT;
    let name = fit(
        ui,
        &open.name,
        font(LARGE, Weight::SemiBold),
        pal.text,
        width,
    );
    let stats: Vec<_> = channel
        .map(|c| c.stats())
        .unwrap_or_default()
        .into_iter()
        .map(|(figure, label)| {
            (
                fit(ui, &figure, font(BODY, Weight::SemiBold), pal.text, width),
                fit(
                    ui,
                    label,
                    font(SMALL, Weight::Regular),
                    pal.secondary,
                    width,
                ),
            )
        })
        .collect();
    // As many figures as fit side by side, each column its widest line and a gap; the later ones go first.
    let column = |n: usize| (width / n as f32).min(24.0 * UNIT);
    let fits = |n: usize| {
        stats[..n]
            .iter()
            .all(|(f, l)| ink_width(f).max(ink_width(l)) + 2.0 * UNIT <= column(n))
    };
    let shown = (1..=stats.len()).rev().find(|&n| fits(n)).unwrap_or(1);
    let stats = &stats[..shown.min(stats.len())];
    let body = channel
        .and_then(|c| c.description.as_deref())
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            let mut job = LayoutJob::simple(
                d.to_owned(),
                font(BODY, Weight::Regular),
                pal.text,
                width.min(80.0 * UNIT),
            );
            job.halign = egui::Align::Center;
            ui.painter().layout_job(job)
        });
    let page = channels::page(&open.slug);
    let link = fit(ui, &page, font(BODY, Weight::Regular), pal.accent, width);

    // Each step down is a gap on the grid.
    let gap = 6.0 * UNIT;
    let figures = if stats.is_empty() {
        0.0
    } else {
        8.0 * UNIT + gap
    };
    let words = body.as_ref().map_or(0.0, |b| b.size().y + gap);
    let height = side + 4.0 * UNIT + name.size().y + gap + figures + words + link.size().y;
    let room = DOCK + 6.0 * UNIT;

    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let inner = ui.max_rect();
            let free = rect.height() - TOP - room;
            let mut y = inner.top() + TOP + ((free - height) / 2.0).max(0.0);
            y = (y / UNIT).round() * UNIT;
            ui.set_height(y - inner.top() + height + room);
            let cx = inner.center().x.round();

            face(
                ui,
                Rect::from_min_size(pos2(cx - side / 2.0, y), vec2(side, side)),
                channel.and_then(|c| c.image.as_deref()),
                &open.name,
            );
            y += side + 4.0 * UNIT;
            put_top(ui, name.clone(), cx - ink_width(&name) / 2.0, y);
            y += name.size().y + gap;

            if !stats.is_empty() {
                // Columns sharing the room evenly, at most 96 wide, the group centred.
                let col = column(stats.len());
                let left = cx - col * stats.len() as f32 / 2.0;
                for (i, (figure, label)) in stats.iter().enumerate() {
                    let mid = left + col * (i as f32 + 0.5);
                    put(
                        ui,
                        figure.clone(),
                        mid - ink_width(figure) / 2.0,
                        y + 2.0 * UNIT,
                    );
                    put(
                        ui,
                        label.clone(),
                        mid - ink_width(label) / 2.0,
                        y + 6.0 * UNIT,
                    );
                }
                y += figures;
            }

            if let Some(b) = &body {
                ui.painter().galley(pos2(cx, y), b.clone(), pal.text);
                y += words;
            }

            let at = put_top(ui, link.clone(), cx - ink_width(&link) / 2.0, y);
            let r = ui
                .interact(at, ui.id().with("page"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.hovered() {
                let under = (at.bottom() - 1.0).round();
                ui.painter()
                    .hline(at.x_range(), under, egui::Stroke::new(1.0, pal.accent));
            }
            if r.clicked() {
                open_page(ui.ctx(), &open.slug);
            }
        });

    fade(ui, rect);
    back(
        ui,
        pos2(
            rect.left() + 2.0 * UNIT + KNOB / 2.0,
            rect.top() + 2.0 * UNIT + KNOB / 2.0,
        ),
        actions,
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn search_finds_every_word_and_ranks_starts_first() {
        let channel = |slug: &str, name: &str| crate::channels::Channel {
            slug: slug.into(),
            name: name.into(),
            description: None,
            image: None,
            created_at: None,
            latest_track_at: None,
            track_count: None,
        };
        let list = [
            channel("tele", "Teleradio"),
            channel("fans", "Fans of radio"),
            channel("belleville", "Radio Belleville"),
            channel("oskar", "Radio Oskar"),
            channel("sempre", "SEMPRELIGADO FM"),
            channel("radiohead-fans", "Fans"),
        ];
        let found = |q| -> Vec<&str> {
            super::matching(q, &list)
                .into_iter()
                .map(|i| list[i].slug.as_str())
                .collect()
        };
        assert_eq!(found("").len(), list.len());
        // Whole-query starts first, then word starts, then anywhere, each in the list's order.
        assert_eq!(
            found("radio"),
            ["belleville", "oskar", "radiohead-fans", "fans", "tele"]
        );
        assert_eq!(found("osk RAD"), ["oskar"]);
        assert_eq!(found("ville"), ["belleville"]);
        assert_eq!(found("liga"), ["sempre"]);
        assert_eq!(found("bel"), ["belleville"]);
        assert!(found("jazz").is_empty());
    }
}
