//! Channel browsing, search, details, and track lists.
use super::Action;
use super::list::{ListScroll, Reveal, note, row, rows};
use super::navigation::{ChannelRef, snap};
use super::transport::DOCK;
use super::widgets::{
    ICON, Icon, KNOB, TOP, back, chrome, close, face, fade, fit, ink_width, knob, put, put_top,
};
use crate::channels::{self, Channel};
use crate::player::State;
use crate::style::{BODY, LARGE, MARGIN, SMALL, UNIT, font, palette};
use crate::tracks::{self, Track};
use egui::text::LayoutJob;
use egui::{Rect, Sense, Ui, pos2, vec2};
use fastframe_fonts::Weight;

/// The library list this search filters.
pub(super) enum SearchTarget {
    Channels,
    Tracks(ChannelRef),
    /// The channel's tags, picking one narrows the channel to it, on top of what it's narrowed to already.
    Tags(ChannelRef),
}

/// The query and its independent result-list scroll and cursor.
pub(super) struct Search {
    pub(super) target: SearchTarget,
    pub(super) query: String,
    pub(super) scroll: ListScroll,
}

impl Search {
    pub(super) fn new(target: SearchTarget) -> Self {
        let mut scroll = ListScroll::default();
        scroll.reveal = Some(Reveal::Show(0));
        Self {
            target,
            query: String::new(),
            scroll,
        }
    }
}

pub(super) fn title(t: &Track) -> &str {
    t.title.as_deref().unwrap_or(&t.url)
}

/// What the channel wrote about a track, on one line, if anything.
pub(super) fn description(t: &Track) -> Option<String> {
    let d = t
        .description
        .as_deref()?
        .split_whitespace()
        .collect::<Vec<_>>();
    (!d.is_empty()).then(|| d.join(" "))
}

/// The tracks a channel shows: all of them, or those matching `filter`, as indices into its list.
pub(super) fn shown_tracks(list: &[Track], filter: Option<&str>) -> Vec<usize> {
    match filter {
        Some(filter) => matching_tracks(filter, list),
        None => (0..list.len()).collect(),
    }
}

/// The tags among what the channel shows, but those already filtered by, holding every word typed,
/// a leading # or not, most used first.
pub(super) fn matching_tags<'a>(
    query: &str,
    list: &'a [Track],
    filter: Option<&str>,
) -> Vec<(&'a str, usize)> {
    let query = query.to_lowercase();
    let words: Vec<_> = query
        .split_whitespace()
        .map(|w| w.trim_start_matches('#'))
        .collect();
    let taken = filter.map(tag_words).unwrap_or_default();
    let shown = shown_tracks(list, filter);
    tracks::tags(shown.iter().map(|&i| &list[i]))
        .into_iter()
        .filter(|(tag, _)| !taken.iter().any(|t| t == tag) && words.iter().all(|w| tag.contains(w)))
        .collect()
}

/// The tags asked for in a query: its words that start with #.
fn tag_words(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter_map(|w| w.strip_prefix('#'))
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn channel_list(
    ui: &mut Ui,
    channels: Option<Result<&[Channel], &str>>,
    open: Option<&str>,
    playing: Option<&str>,
    opens: bool,
    search: bool,
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
    // While a channel slides over, its own search button stands in for this one.
    if !search {
        return;
    }
    let side = 2.0 * UNIT + KNOB / 2.0;
    let c = pos2(rect.right() - side, rect.top() + side);
    let r = knob(ui, c, "search");
    let pal = palette(ui);
    Icon::Search
        .image(pal.text, ICON)
        .paint_at(ui, Rect::from_center_size(c, vec2(ICON, ICON)));
    if r.on_hover_text("Search (/)").clicked() {
        actions.push(Action::Search(SearchTarget::Channels));
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
        None,
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

/// The search panel: a back button and the field floating at the top, matching channels or tracks below.
/// The cursor always shows, so Return opens or plays the selected match.
pub(super) fn search_panel(
    ui: &mut Ui,
    search: &mut Search,
    channels: Option<Result<&[Channel], &str>>,
    tracks: Option<Result<&[Track], &str>>,
    filter: Option<&str>,
    state: &State,
    actions: &mut Vec<Action>,
) {
    let pal = palette(ui);
    let rect = ui.max_rect();
    match &search.target {
        SearchTarget::Channels => match channels {
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
                        channel_row(
                            ui,
                            &list[hits[i]],
                            cursor == i,
                            state.queue.as_deref(),
                            true,
                            actions,
                        );
                    });
                }
            }
        },
        SearchTarget::Tracks(open) => match tracks {
            None => note(ui, "Loading…"),
            Some(Err(e)) => note(ui, &format!("Could not load tracks: {e}")),
            Some(Ok(list)) => {
                let hits = matching_tracks(&search.query, list);
                if hits.is_empty() {
                    note(ui, "No tracks match.");
                } else {
                    search.scroll.cursor = search.scroll.cursor.min(hits.len() - 1);
                    let cursor = search.scroll.cursor;
                    let current = state
                        .index
                        .filter(|_| state.queue.as_deref() == Some(&open.slug));
                    rows(ui, hits.len(), None, &mut search.scroll, |ui, i| {
                        let index = hits[i];
                        if row(
                            ui,
                            title(&list[index]),
                            description(&list[index]).as_deref(),
                            None,
                            cursor == i,
                            current == Some(index),
                            false,
                        )
                        .0
                        .clicked()
                        {
                            actions.push(Action::PlayTrack(open.clone(), index));
                        }
                    });
                }
            }
        },
        SearchTarget::Tags(open) => match tracks {
            None => note(ui, "Loading…"),
            Some(Err(e)) => note(ui, &format!("Could not load tracks: {e}")),
            Some(Ok(list)) => {
                let hits = matching_tags(&search.query, list, filter);
                if hits.is_empty() {
                    note(ui, "No tags match.");
                } else {
                    search.scroll.cursor = search.scroll.cursor.min(hits.len() - 1);
                    let cursor = search.scroll.cursor;
                    // The tag playing on its own carries the dot.
                    let playing = state
                        .filter
                        .as_deref()
                        .filter(|_| state.queue.as_deref() == Some(&open.slug))
                        .and_then(|f| f.strip_prefix('#'))
                        .filter(|f| !f.contains(char::is_whitespace));
                    rows(ui, hits.len(), None, &mut search.scroll, |ui, i| {
                        let (tag, count) = hits[i];
                        if row(
                            ui,
                            &format!("#{tag}"),
                            Some(&count.to_string()),
                            None,
                            cursor == i,
                            playing == Some(tag),
                            true,
                        )
                        .0
                        .clicked()
                        {
                            actions.push(Action::Tag(tag.to_owned()));
                        }
                    });
                }
            }
        },
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
    let hint = match &search.target {
        SearchTarget::Channels => "Search channels",
        SearchTarget::Tracks(_) => "Search tracks",
        SearchTarget::Tags(_) => "Find a tag",
    };
    let edit = egui::TextEdit::singleline(&mut search.query)
        .frame(egui::Frame::NONE)
        .margin(0.0)
        .vertical_align(egui::Align::Center)
        .font(font(BODY, Weight::Regular))
        .text_color(pal.text)
        .hint_text(egui::RichText::new(hint).color(pal.secondary))
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

/// Track matches keep their original queue indices and ordering, including URL fallback titles.
/// Words are looked for in the description too; a word starting with # asks for that tag exactly.
pub(super) fn matching_tracks(query: &str, tracks: &[Track]) -> Vec<usize> {
    let query = query.to_lowercase();
    let tags = tag_words(&query);
    let words: Vec<_> = query
        .split_whitespace()
        .filter(|w| !w.starts_with('#'))
        .collect();
    tracks
        .iter()
        .enumerate()
        .filter_map(|(i, track)| {
            let name = format!(
                "{} {}",
                title(track),
                track.description.as_deref().unwrap_or("")
            )
            .to_lowercase();
            (tags.iter().all(|t| track.tagged(t)) && words.iter().all(|word| name.contains(word)))
                .then_some(i)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn tracklist(
    ui: &mut Ui,
    open: &ChannelRef,
    image: Option<&str>,
    tracks: Option<Result<&[Track], &str>>,
    tag: Option<&str>,
    s: &State,
    back: Option<f32>,
    scroll: &mut ListScroll,
    keyed: bool,
    actions: &mut Vec<Action>,
) {
    let rect = ui.max_rect();
    let mut tagged = false;
    match tracks {
        None => note(ui, "Loading…"),
        Some(Err(e)) => note(ui, &format!("Could not load tracks: {e}")),
        Some(Ok(list)) => {
            tagged = list.iter().any(|t| !t.tags.is_empty());
            let shown = shown_tracks(list, tag);
            let current = s
                .index
                .filter(|_| s.queue.as_deref() == Some(&open.slug))
                .and_then(|i| shown.iter().position(|&j| j == i));
            let cursor = keyed.then_some(scroll.cursor);
            rows(ui, shown.len(), current, scroll, |ui, i| {
                let track = &list[shown[i]];
                let selected = cursor == Some(i);
                if row(
                    ui,
                    title(track),
                    description(track).as_deref(),
                    None,
                    selected,
                    current == Some(i),
                    false,
                )
                .0
                .clicked()
                {
                    actions.push(Action::Play(shown[i]));
                }
            });
        }
    }
    chrome(ui, rect, open, image, tag, tagged, back, actions);
}

/// The channel's page on radio4000.com, in the browser.
pub(super) fn open_page(ctx: &egui::Context, slug: &str) {
    ctx.open_url(egui::OpenUrl::new_tab(format!(
        "https://{}",
        channels::page(slug)
    )));
}

/// The open channel's details in a sheet `risen` part of the way over its tracklist: its face, name and figures,
/// its own words, and a link to its page. The sheet is as tall as that, short of the top band, scrolling beyond.
pub(super) fn channel_info(
    ui: &mut Ui,
    open: &ChannelRef,
    channel: Option<&Channel>,
    risen: f32,
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
    // The face sits level with the close button.
    let head = 2.0 * UNIT;
    let full = (head + height + room).min(rect.height() - TOP);
    let sheet = Rect::from_min_max(pos2(rect.left(), rect.bottom() - full), rect.max)
        .translate(snap(ui, vec2(0.0, (1.0 - risen) * full)));
    let corner = (4.0 * UNIT) as u8;
    ui.painter().rect(
        sheet,
        egui::CornerRadius {
            nw: corner,
            ne: corner,
            sw: 0,
            se: 0,
        },
        pal.bg,
        egui::Stroke::new(1.0, pal.edge),
        egui::StrokeKind::Inside,
    );
    // Clicks on the sheet stay on it, rather than closing it from the scrim beneath.
    ui.interact(sheet, ui.id().with("sheet"), Sense::click_and_drag());

    ui.scope_builder(egui::UiBuilder::new().max_rect(sheet), |ui| {
        ui.shrink_clip_rect(sheet.shrink(1.0));
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let inner = ui.max_rect();
                let mut y = inner.top() + head;
                ui.set_height(head + height + room);
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
    });

    close(
        ui,
        pos2(
            sheet.left() + 2.0 * UNIT + KNOB / 2.0,
            sheet.top() + 2.0 * UNIT + KNOB / 2.0,
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

    #[test]
    fn track_search_matches_all_words_case_insensitively_and_uses_display_titles() {
        let track = |title: Option<&str>, url: &str| crate::tracks::Track {
            title: title.map(str::to_owned),
            url: url.into(),
            provider: None,
            ..Default::default()
        };
        let list = [
            track(Some("Blue Train — live"), "https://example.com/one"),
            track(Some("Blue Moon"), "https://example.com/train"),
            track(None, "https://example.com/BLUE-train-live"),
            track(
                Some("Live at the station"),
                "https://example.com/blue-train",
            ),
        ];
        assert_eq!(super::matching_tracks("LIVE blue", &list), [0, 2]);
        assert_eq!(super::matching_tracks("train", &list), [0, 2]);
        assert_eq!(super::matching_tracks("   ", &list), [0, 1, 2, 3]);
        assert!(super::matching_tracks("blue jazz", &list).is_empty());
    }

    #[test]
    fn tags_and_words_mix_in_one_query() {
        let track = |title: &str, description: &str, tags: &[&str]| crate::tracks::Track {
            title: Some(title.into()),
            description: Some(description.into()),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        };
        let list = [
            track("King Tubby – Dub", "#dub #roots", &["dub", "roots"]),
            track("Basic Channel – Phylyps", "#dubstep heavy", &["dubstep"]),
            track("Augustus Pablo – East", "#dub house party", &["dub"]),
            track("Moodymann – Shades", "#house", &["house"]),
        ];
        // A # word is that tag exactly, other words look in titles and descriptions.
        assert_eq!(super::matching_tracks("#dub", &list), [0, 2]);
        assert_eq!(super::matching_tracks("#DUB party", &list), [2]);
        assert_eq!(super::matching_tracks("#dub #roots", &list), [0]);
        assert_eq!(super::shown_tracks(&list, None), [0, 1, 2, 3]);
        // Tags offered on a narrowed channel count only what it shows, and leave out what's already asked for.
        assert_eq!(
            super::matching_tags("", &list, Some("#dub")),
            [("roots", 1)]
        );
        assert_eq!(
            super::matching_tags("#du", &list, None),
            [("dub", 2), ("dubstep", 1)]
        );
    }
}
