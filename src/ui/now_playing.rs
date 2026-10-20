//! The desktop Spotify-style Now Playing side panel: dynamically resizing
//! square cover art, song title & artist beside the Liked Songs button, and
//! a 50/50 split between Lyrics and Queue that collapses into expansion panels
//! (Lyrics first) when vertical space gets tight and scrolls as a whole when
//! space cannot fit 3 queue songs or 3 lyric lines.

use egui::{
    Align, CornerRadius, Frame, Id, Layout, Margin, Rect, Sense, Stroke, UiBuilder, Vec2, pos2,
    vec2,
};

use crate::app::{App, NowPlaying};
use crate::i18n::{gettext, pgettext};
use crate::model::{Action, DragTrack, Loadable, Page};
use crate::theme::{self, Icon};

use super::widgets;

const LIKED_POPUP_ID: &str = "now-playing-panel-liked-playlists";

/// Gap between the Lyrics card and the Queue card.
pub(crate) const CARD_GAP: f32 = 10.0;
/// Height of a collapsed expansion card header.
pub(crate) const COLLAPSED_HEIGHT: f32 = 44.0;
/// Minimum height of an expanded Lyrics card (header + 3 lines of lyrics).
pub(crate) const MIN_LYRICS_HEIGHT: f32 = 152.0;
/// Minimum height of an expanded Queue card (header + 3 songs in the queue).
pub(crate) const MIN_QUEUE_HEIGHT: f32 = 200.0;

/// Computes the target heights `(lyrics_height, queue_height)` for the Lyrics
/// and Queue sections given `remaining` vertical space below the cover & title.
pub(crate) fn section_heights(remaining: f32, lyrics_open: bool, queue_open: bool) -> (f32, f32) {
    match (lyrics_open, queue_open) {
        (true, true) => {
            let avail = (remaining - CARD_GAP).floor().max(0.0);
            let half = (avail * 0.5).floor();
            let queue = half.max(MIN_QUEUE_HEIGHT);
            let lyrics = (avail - queue).max(MIN_LYRICS_HEIGHT);
            (lyrics, queue)
        }
        (false, true) => {
            let avail = (remaining - COLLAPSED_HEIGHT - CARD_GAP).floor().max(0.0);
            (COLLAPSED_HEIGHT, avail.max(MIN_QUEUE_HEIGHT))
        }
        (true, false) => {
            let avail = (remaining - COLLAPSED_HEIGHT - CARD_GAP).floor().max(0.0);
            (avail.max(MIN_LYRICS_HEIGHT), COLLAPSED_HEIGHT)
        }
        (false, false) => (COLLAPSED_HEIGHT, COLLAPSED_HEIGHT),
    }
}

pub fn side_panel(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let fit = super::yielding_panel(
        ui.ctx(),
        "now-playing-panel",
        theme::SIDE_PANEL_MIN_WIDTH..=600.0,
        app.settings.now_playing_width,
        ui.available_width() - super::topbar::least_width(ui.ctx()),
    );
    let panel = egui::Panel::right("now-playing-panel")
        .resizable(true)
        .default_size(app.settings.now_playing_width)
        .size_range(fit.range.clone())
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.panel)
                .inner_margin(Margin::symmetric(12, 12)),
        );
    let response = panel.show(ui, |ui| {
        // Disable egui's scroll-edge fade gradient, which fades toward the
        // window fill and clashes with the grey surface cards.
        ui.spacing_mut().scroll.fade.strength = 0.0;
        let window_controls =
            super::window_controls_reservation(ui.ctx(), true, false, ui.available_width());
        ui.add_space(window_controls.queue_top);
        header(app, ui);
        ui.add_space(8.0);
        let viewport_height = ui.available_height().floor().max(0.0);
        crate::autoscroll::show(
            ui,
            egui::ScrollArea::vertical()
                .id_salt("now-playing-panel-scroll")
                .auto_shrink([false, false]),
            egui::Vec2b::new(false, true),
            |ui| {
                body(app, ui, viewport_height);
            },
        );
    });
    let current_width = response.response.rect.width();
    if (app.settings.now_playing_width - current_width).abs() > 1.0
        && super::panel_width_chosen(ui.ctx(), "now-playing-panel", &fit)
    {
        app.settings.now_playing_width = current_width;
        app.actions.push(Action::SettingsChanged);
    }
}

fn header(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let from = app.playing_from();
    let mut open_page = None;
    let mut close = false;
    egui::Sides::new().shrink_left().show(
        ui,
        |ui| {
            ui.add_space(2.0);
            match from {
                Some(from) => match from.page {
                    Some(page) => {
                        if theme::link(ui, from.name, theme::bold(15.0), palette.text).clicked() {
                            open_page = Some(page);
                        }
                    }
                    None => {
                        theme::text(ui, from.name, theme::bold(15.0), palette.text);
                    }
                },
                None => {
                    theme::text(
                        ui,
                        gettext(app.locale, "Now playing"),
                        theme::bold(15.0),
                        palette.text,
                    );
                }
            }
        },
        |ui| {
            close = theme::icon_button(
                ui,
                Icon::X,
                18.0,
                palette.secondary,
                palette.text,
                &gettext(app.locale, "Close"),
            )
            .clicked();
        },
    );
    if let Some(page) = open_page {
        app.actions.push(Action::Open(page));
    }
    if close {
        app.actions.push(Action::ToggleNowPlayingPanel);
    }
}

fn body(app: &mut App, ui: &mut egui::Ui, viewport_height: f32) {
    let top_y = ui.cursor().top();
    let width = (ui.available_width() - 4.0).max(120.0);
    let now = app.now_playing();

    cover_and_meta(app, ui, width, now.as_ref());
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.add_space(8.0);

    let used_above = (ui.cursor().top() - top_y).max(0.0);
    let remaining = (viewport_height - used_above).floor().max(0.0);
    let show_lyrics = app.settings.now_playing_show_lyrics;
    let show_queue = app.settings.now_playing_show_queue;

    match (show_lyrics, show_queue) {
        (true, true) => {
            let both_fit = remaining >= MIN_LYRICS_HEIGHT + MIN_QUEUE_HEIGHT + CARD_GAP;
            if app.now_playing_lyrics_auto != Some(both_fit) {
                app.now_playing_lyrics_auto = Some(both_fit);
                app.now_playing_lyrics_collapsed = !both_fit;
            }
            let (lyrics_height, queue_height) = section_heights(
                remaining,
                !app.now_playing_lyrics_collapsed,
                !app.now_playing_queue_collapsed,
            );
            lyrics_card(app, ui, width, lyrics_height);
            ui.add_space(CARD_GAP);
            queue_card(app, ui, width, queue_height);
        }
        (true, false) => {
            let height = if app.now_playing_lyrics_collapsed {
                COLLAPSED_HEIGHT
            } else {
                remaining.max(MIN_LYRICS_HEIGHT)
            };
            lyrics_card(app, ui, width, height);
        }
        (false, true) => {
            let height = if app.now_playing_queue_collapsed {
                COLLAPSED_HEIGHT
            } else {
                remaining.max(MIN_QUEUE_HEIGHT)
            };
            queue_card(app, ui, width, height);
        }
        (false, false) => {}
    }
}

fn cover_and_meta(app: &mut App, ui: &mut egui::Ui, width: f32, now: Option<&NowPlaying>) {
    let palette = app.palette;
    let song = app.now_playing_item();
    let cover_radius = app.settings.cover_radius;
    let cover_radius_f32 = f32::from(cover_radius);
    let drag_sense = if song.is_some() {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let (cover_rect, cover_response) = ui.allocate_exact_size(Vec2::splat(width), drag_sense);
    ui.painter().add(
        egui::epaint::Shadow {
            offset: [0, 4],
            blur: 24,
            spread: 0,
            color: egui::Color32::from_black_alpha(if palette.dark { 110 } else { 36 }),
        }
        .as_shape(cover_rect, CornerRadius::same(cover_radius)),
    );

    let Some(now) = now else {
        widgets::paint_cover(
            ui,
            &palette,
            None,
            cover_rect,
            cover_radius_f32,
            Icon::Music,
            None,
        );
        ui.add_space(12.0);
        theme::text(
            ui,
            gettext(app.locale, "Nothing playing"),
            theme::bold(18.0),
            palette.text,
        );
        ui.add_space(2.0);
        theme::text(
            ui,
            gettext(app.locale, "Pick a song, album, or playlist"),
            theme::regular(13.5),
            palette.secondary,
        );
        return;
    };

    let url = now.art_url.as_deref().or(now.art_small.as_deref());
    widgets::playing_cover(
        ui,
        &palette,
        app.backend.art(),
        "now-playing-panel",
        url,
        cover_rect,
        cover_radius_f32,
    );
    let cover_response = cover_response.on_hover_cursor(if song.is_some() {
        egui::CursorIcon::Grab
    } else {
        egui::CursorIcon::PointingHand
    });
    if cover_response.clicked() {
        if let Some(id) = &now.album_id {
            app.actions.push(Action::Open(Page::Album(id.clone())));
        } else if let Some(id) = &now.show_id {
            app.actions.push(Action::Open(Page::Show(id.clone())));
        }
    }
    if cover_response.drag_started_by(egui::PointerButton::Primary)
        && let Some(item) = &song
    {
        egui::DragAndDrop::set_payload(
            ui.ctx(),
            DragTrack {
                title: item.name().to_string(),
                image: item.image(64).map(str::to_string),
                items: vec![item.clone()],
                from: None,
                source_playlist: None,
            },
        );
    }
    if let Some(item) = &song {
        let context = app.editable_context_playlist();
        egui::Popup::context_menu(&cover_response)
            .frame(widgets::menu_frame(&palette))
            .show(|ui| widgets::item_menu(ui, app, item, context.as_ref(), None));
    }

    ui.add_space(12.0);
    let like_width = if now.is_episode { 0.0 } else { 36.0 };
    let text_width = (width - like_width - 8.0).max(60.0);
    ui.horizontal(|ui| {
        ui.set_width(width);
        let text_response = ui.allocate_ui_with_layout(
            vec2(text_width, 44.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_max_width(text_width);
                ui.spacing_mut().item_spacing.y = 3.0;
                let title_res = theme::link(ui, &now.title, theme::bold(18.0), palette.text);
                if title_res.clicked() {
                    if let Some(id) = &now.album_id {
                        app.actions.push(Action::Open(Page::Album(id.clone())));
                    } else if let Some(id) = &now.show_id {
                        app.actions.push(Action::Open(Page::Show(id.clone())));
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if now.artists.is_empty() {
                        if theme::link(ui, &now.subtitle, theme::regular(13.5), palette.secondary)
                            .clicked()
                            && let Some(id) = &now.show_id
                        {
                            app.actions.push(Action::Open(Page::Show(id.clone())));
                        }
                    } else {
                        widgets::artist_links(
                            ui,
                            app,
                            &now.artists,
                            theme::regular(13.5),
                            palette.secondary,
                        );
                    }
                });
                title_res
            },
        );
        if let Some(item) = &song {
            let context = app.editable_context_playlist();
            egui::Popup::context_menu(&text_response.inner)
                .frame(widgets::menu_frame(&palette))
                .show(|ui| widgets::item_menu(ui, app, item, context.as_ref(), None));
        }

        if !now.is_episode {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                liked_button(app, ui, now, song.as_ref());
            });
        }
    });
}

fn liked_button(
    app: &mut App,
    ui: &mut egui::Ui,
    now: &NowPlaying,
    song: Option<&crate::api::models::PlayableItem>,
) {
    let palette = app.palette;
    let saved = app.is_saved(&now.uri).unwrap_or(false);
    let (icon, color, hover, tooltip) = if saved {
        (
            Icon::CircleCheck,
            palette.accent,
            palette.accent_hover,
            gettext(app.locale, "Add to playlist"),
        )
    } else {
        (
            Icon::CirclePlus,
            palette.secondary,
            palette.text,
            gettext(app.locale, "Save to Liked Songs"),
        )
    };
    let response = theme::icon_button(ui, icon, 20.0, color, hover, &tooltip);
    let popup_id = Id::new(LIKED_POPUP_ID);
    match song.filter(|_| saved) {
        None => {
            egui::Popup::close_id(ui.ctx(), popup_id);
            if response.clicked() {
                app.actions.push(Action::ToggleSaved(now.uri.clone()));
            }
        }
        Some(item) => {
            let query_id = popup_id.with("query");
            let shown = egui::Popup::menu(&response)
                .id(popup_id)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .frame(widgets::menu_frame(&palette))
                .show(|ui| {
                    let stored = ui.data(|data| data.get_temp::<String>(query_id));
                    let fresh = stored.is_none();
                    let mut query = stored.unwrap_or_default();
                    let before = app.actions.len();
                    let field =
                        widgets::playlist_picker(ui, app, std::slice::from_ref(item), &mut query);
                    if fresh {
                        field.request_focus();
                    }
                    ui.data_mut(|data| data.insert_temp(query_id, query));
                    widgets::menu_separator(ui, &palette);
                    if widgets::menu_item(
                        ui,
                        &palette,
                        Some(Icon::HeartFilled),
                        &gettext(app.locale, "Remove from Liked Songs"),
                    ) {
                        app.actions.push(Action::ToggleSaved(now.uri.clone()));
                    }
                    if app.actions.len() > before {
                        ui.close();
                    }
                });
            if shown.is_none() {
                ui.data_mut(|data| data.remove::<String>(query_id));
            }
        }
    }
}

fn lyrics_card(app: &mut App, ui: &mut egui::Ui, width: f32, height: f32) {
    let palette = app.palette;
    let collapsed = app.now_playing_lyrics_collapsed;
    let (card_rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect(
        card_rect,
        CornerRadius::same(theme::RADIUS),
        palette.surface,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
    let header_rect = Rect::from_min_size(card_rect.min, vec2(width, COLLAPSED_HEIGHT.min(height)));
    let header_response = ui
        .interact(
            header_rect,
            ui.make_persistent_id("now-playing-lyrics-header"),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    let inner_rect = card_rect.shrink2(vec2(12.0, 8.0));
    let mut card_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(inner_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    card_ui.set_clip_rect(inner_rect.intersect(ui.clip_rect()));

    let locale = app.locale;
    let mut toggle_left = false;
    let mut toggle_right = false;
    let mut expand_fullscreen = false;
    let mut follow = false;
    let loaded = matches!(&app.lyrics, Loadable::Loaded(Some(_)));
    let show_follow = loaded && !app.lyrics_following && !collapsed;

    egui::Sides::new().shrink_left().show(
        &mut card_ui,
        |ui| {
            let label = theme::link(
                ui,
                gettext(locale, "Lyrics"),
                theme::bold(15.0),
                palette.text,
            );
            if label.clicked() {
                toggle_left = true;
            }
        },
        |ui| {
            let chevron = if collapsed {
                Icon::ChevronDown
            } else {
                Icon::ChevronUp
            };
            if theme::icon_button(
                ui,
                chevron,
                16.0,
                palette.secondary,
                palette.text,
                &gettext(locale, "Lyrics"),
            )
            .clicked()
            {
                toggle_right = true;
            }
            if theme::icon_button(
                ui,
                Icon::Expand,
                16.0,
                palette.secondary,
                palette.text,
                &gettext(locale, "Full screen lyrics"),
            )
            .clicked()
            {
                expand_fullscreen = true;
            }
            if show_follow
                && theme::pill_button(ui, &palette, &pgettext(locale, "lyrics", "Follow"), false)
                    .clicked()
            {
                follow = true;
            }
        },
    );

    if (header_response.clicked() || toggle_left || toggle_right) && !expand_fullscreen && !follow {
        app.now_playing_lyrics_collapsed = !app.now_playing_lyrics_collapsed;
    }
    if expand_fullscreen {
        app.actions.push(Action::SetLyricsFullscreen(true));
    }
    if follow {
        app.lyrics_following = true;
        app.lyrics_line_shown = None;
    }

    if !collapsed {
        let body_top = (card_ui.cursor().top() + 4.0).min(inner_rect.bottom());
        let body_rect = Rect::from_min_max(pos2(inner_rect.left(), body_top), inner_rect.max);
        let mut body_ui = card_ui.new_child(
            UiBuilder::new()
                .max_rect(body_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        body_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        body_ui.spacing_mut().scroll.fade.strength = 0.0;
        super::lyrics::contents(app, &mut body_ui);
    }
}

fn queue_card(app: &mut App, ui: &mut egui::Ui, width: f32, height: f32) {
    let palette = app.palette;
    let collapsed = app.now_playing_queue_collapsed;
    let (card_rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect(
        card_rect,
        CornerRadius::same(theme::RADIUS),
        palette.surface,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
    let header_rect = Rect::from_min_size(card_rect.min, vec2(width, COLLAPSED_HEIGHT.min(height)));
    let header_response = ui
        .interact(
            header_rect,
            ui.make_persistent_id("now-playing-queue-header"),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    let inner_rect = card_rect.shrink2(vec2(12.0, 8.0));
    let mut card_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(inner_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    card_ui.set_clip_rect(inner_rect.intersect(ui.clip_rect()));

    let locale = app.locale;
    let offer_save = !app.queue_playlist_uris().is_empty();
    let mut toggle_left = false;
    let mut toggle_right = false;
    let mut save = false;
    let mut clear = false;

    egui::Sides::new().shrink_left().show(
        &mut card_ui,
        |ui| {
            let label = theme::link(
                ui,
                gettext(locale, "Queue"),
                theme::bold(15.0),
                palette.text,
            );
            if label.clicked() {
                toggle_left = true;
            }
        },
        |ui| {
            let chevron = if collapsed {
                Icon::ChevronDown
            } else {
                Icon::ChevronUp
            };
            if theme::icon_button(
                ui,
                chevron,
                16.0,
                palette.secondary,
                palette.text,
                &gettext(locale, "Queue"),
            )
            .clicked()
            {
                toggle_right = true;
            }
            save = super::queue::save_button(ui, &palette, offer_save, locale);
            clear = super::queue::clear_button(app, ui);
        },
    );

    if (header_response.clicked() || toggle_left || toggle_right) && !save && !clear {
        app.now_playing_queue_collapsed = !app.now_playing_queue_collapsed;
    }
    if save {
        app.actions.push(Action::SaveQueueAsPlaylist);
    }

    if !collapsed {
        let body_top = (card_ui.cursor().top() + 4.0).min(inner_rect.bottom());
        let body_rect = Rect::from_min_max(pos2(inner_rect.left(), body_top), inner_rect.max);
        let mut body_ui = card_ui.new_child(
            UiBuilder::new()
                .max_rect(body_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        body_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        body_ui.spacing_mut().scroll.fade.strength = 0.0;
        crate::autoscroll::show(
            &mut body_ui,
            egui::ScrollArea::vertical()
                .id_salt("now-playing-queue-scroll")
                .auto_shrink([false, false])
                .content_margin(Margin {
                    left: 0,
                    right: 8,
                    top: 0,
                    bottom: 0,
                }),
            egui::Vec2b::new(false, true),
            |ui| {
                super::queue::upcoming_contents(app, ui);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_split_evenly_when_space_allows_and_enforce_minimums_when_tight() {
        let (lyrics, queue) = section_heights(510.0, true, true);
        assert_eq!(lyrics, 250.0);
        assert_eq!(queue, 250.0);

        let (lyrics_collapsed, queue_expanded) = section_heights(300.0, false, true);
        assert_eq!(lyrics_collapsed, COLLAPSED_HEIGHT);
        assert_eq!(queue_expanded, 300.0 - COLLAPSED_HEIGHT - CARD_GAP);

        let (lyrics_mid, queue_mid) = section_heights(380.0, true, true);
        assert!(lyrics_mid >= MIN_LYRICS_HEIGHT);
        assert!(queue_mid >= MIN_QUEUE_HEIGHT);
        assert!(lyrics_mid + CARD_GAP + queue_mid <= 380.0);

        let (lyrics_collapsed, queue_min) = section_heights(120.0, false, true);
        assert_eq!(lyrics_collapsed, COLLAPSED_HEIGHT);
        assert_eq!(queue_min, MIN_QUEUE_HEIGHT);
    }
}
