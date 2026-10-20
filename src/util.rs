//! Small helpers shared across the application.

use std::borrow::Cow;

use crate::i18n::{Locale, gettext, ngettext, pgettext};

/// `3:45` for track lengths, `1:02:03` past an hour.
pub fn format_duration_ms(ms: u32) -> String {
    let total = ms / 1000;
    let hours = total / 3600;
    let minutes = (total / 60) % 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// `2 hr 13 min` for playlist totals, `45 min 12 sec` under an hour.
pub fn format_total_ms(locale: Locale, ms: u64) -> String {
    let total = ms / 1000;
    let hours = total / 3600;
    let minutes = (total / 60) % 60;
    let seconds = total % 60;
    if hours > 0 {
        // Translators: A length of time, abbreviated. Keep {hours} and {minutes}.
        gettext(locale, "{hours} hr {minutes} min")
            .replace("{hours}", &hours.to_string())
            .replace("{minutes}", &minutes.to_string())
    } else if minutes > 0 {
        // Translators: A length of time, abbreviated. Keep {minutes} and {seconds}.
        gettext(locale, "{minutes} min {seconds} sec")
            .replace("{minutes}", &minutes.to_string())
            .replace("{seconds}", &seconds.to_string())
    } else {
        // Translators: A length of time, abbreviated. Keep {seconds}.
        gettext(locale, "{seconds} sec").replace("{seconds}", &seconds.to_string())
    }
}

/// Episode lengths read as `1 hr 12 min` or `38 min`.
pub fn format_episode_ms(locale: Locale, ms: u32) -> String {
    let minutes = ms / 60_000;
    let hours = minutes / 60;
    if hours > 0 {
        gettext(locale, "{hours} hr {minutes} min")
            .replace("{hours}", &hours.to_string())
            .replace("{minutes}", &(minutes % 60).to_string())
    } else {
        // Translators: A length of time, abbreviated. Keep {minutes}.
        gettext(locale, "{minutes} min").replace("{minutes}", &minutes.max(1).to_string())
    }
}

pub fn format_count(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// `Jan 5, 2024` from an ISO-8601 timestamp or a bare date.
pub fn format_date(locale: Locale, iso: &str) -> String {
    let date = iso.get(..10).unwrap_or(iso);
    let mut parts = date.split('-');
    let (Some(year), Some(month)) = (parts.next(), parts.next()) else {
        return iso.to_string();
    };
    let day = parts.next();
    let month_name = match month {
        "01" => pgettext(locale, "month", "Jan"),
        "02" => pgettext(locale, "month", "Feb"),
        "03" => pgettext(locale, "month", "Mar"),
        "04" => pgettext(locale, "month", "Apr"),
        "05" => pgettext(locale, "month", "May"),
        "06" => pgettext(locale, "month", "Jun"),
        "07" => pgettext(locale, "month", "Jul"),
        "08" => pgettext(locale, "month", "Aug"),
        "09" => pgettext(locale, "month", "Sep"),
        "10" => pgettext(locale, "month", "Oct"),
        "11" => pgettext(locale, "month", "Nov"),
        "12" => pgettext(locale, "month", "Dec"),
        _ => return iso.to_string(),
    };
    let dated = match day.and_then(|day| day.trim_start_matches('0').parse::<u8>().ok()) {
        Some(day) => {
            // Translators: A date. {month} is an abbreviated month name; reorder as your language writes dates.
            gettext(locale, "{month} {day}, {year}").replace("{day}", &day.to_string())
        }
        // Translators: A month and year. {month} is an abbreviated month name.
        None => gettext(locale, "{month} {year}").into_owned(),
    };
    dated
        .replace("{month}", &month_name)
        .replace("{year}", year)
}

/// `5 minutes ago` for recent ISO-8601 timestamps, otherwise the usual date.
///
/// Dates are shown relatively for their first 30 days, matching the playlist
/// table's compact, time-aware presentation. `now` is an argument so callers
/// can render against one instant and the boundary behaviour stays testable.
pub fn format_relative_date(locale: Locale, iso: &str, now: jiff::Timestamp) -> String {
    let Ok(added) = iso.parse::<jiff::Timestamp>() else {
        return format_date(locale, iso);
    };
    let seconds = added.duration_until(now).as_secs_f64().floor() as i64;
    if !(0..30 * 24 * 60 * 60).contains(&seconds) {
        return format_date(locale, iso);
    }

    let (count, text) = if seconds < 60 {
        let count = seconds;
        let text = ngettext(
            locale,
            // Translators: How long ago a song was added. Keep {count}.
            "{count} second ago",
            "{count} seconds ago",
            count as u32,
        );
        (count, text)
    } else if seconds < 60 * 60 {
        let count = seconds / 60;
        let text = ngettext(
            locale,
            // Translators: How long ago a song was added. Keep {count}.
            "{count} minute ago",
            "{count} minutes ago",
            count as u32,
        );
        (count, text)
    } else if seconds < 24 * 60 * 60 {
        let count = seconds / (60 * 60);
        let text = ngettext(
            locale,
            // Translators: How long ago a song was added. Keep {count}.
            "{count} hour ago",
            "{count} hours ago",
            count as u32,
        );
        (count, text)
    } else if seconds < 7 * 24 * 60 * 60 {
        let count = seconds / (24 * 60 * 60);
        // Translators: How long ago a song was added. Keep {count}.
        let text = ngettext(locale, "{count} day ago", "{count} days ago", count as u32);
        (count, text)
    } else {
        let count = seconds / (7 * 24 * 60 * 60);
        let text = ngettext(
            locale,
            // Translators: How long ago a song was added. Keep {count}.
            "{count} week ago",
            "{count} weeks ago",
            count as u32,
        );
        (count, text)
    };
    text.replace("{count}", &count.to_string())
}

/// Tears the id out of `spotify:track:abc` and friends.
pub fn uri_id(uri: &str) -> Option<&str> {
    uri.rsplit(':').next().filter(|id| !id.is_empty())
}

pub fn uri_kind(uri: &str) -> Option<&str> {
    let mut parts = uri.split(':');
    parts.next()?;
    parts.next()
}

/// Spotify's radio station seeded by a song, playlist, album, or artist.
pub fn station_uri(seed: &str) -> Option<String> {
    let kind = uri_kind(seed)?;
    let id = uri_id(seed)?;
    (seed == format!("spotify:{kind}:{id}")
        && matches!(kind, "track" | "playlist" | "album" | "artist"))
    .then(|| format!("spotify:station:{kind}:{id}"))
}

/// The song, playlist, album, or artist a radio station is seeded by.
pub fn station_seed(station: &str) -> Option<String> {
    let seed = format!("spotify:{}", station.strip_prefix("spotify:station:")?);
    station_uri(&seed).is_some().then_some(seed)
}

pub fn open_spotify_url(uri: &str) -> Option<String> {
    let kind = uri_kind(uri)?;
    let id = uri_id(uri)?;
    Some(format!("https://open.spotify.com/{kind}/{id}"))
}

/// The menu-bar shape for macOS: the circle with the three waves punched
/// out. macOS template images use only the alpha channel and paint the
/// shape themselves, black in a light menu bar and white in a dark one.
pub fn tray_template_rgba(size: usize) -> Vec<u8> {
    let mut rgba = mark_rgba(size);
    for pixel in rgba.as_chunks_mut::<4>().0 {
        // The waves are the dark colour; make them holes instead.
        if pixel[1] < 128 {
            pixel[3] = 0;
        }
        pixel[0] = 0;
        pixel[1] = 0;
        pixel[2] = 0;
    }
    rgba
}

/// The mark rasterised to pixels: the window icon, the trays and the logo
/// drawn in the app (`theme::logo`) all use this one picture.
pub fn app_icon_rgba(size: usize) -> Vec<u8> {
    mark_rgba(size)
}

/// Mixes two colours, `t` of the way from `a` to `b`.
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Builds the three curved wave bars from `packaging/icons/spotifast.svg`
/// (`viewBox="0 0 496 512"`, disc centred at `(248, 256)` with radius `248`).
fn spotify_waves_path() -> Option<tiny_skia::Path> {
    struct SvgPen {
        builder: tiny_skia::PathBuilder,
        x: f32,
        y: f32,
        start_x: f32,
        start_y: f32,
        last_c2_x: f32,
        last_c2_y: f32,
    }
    impl SvgPen {
        fn new() -> Self {
            Self {
                builder: tiny_skia::PathBuilder::new(),
                x: 0.0,
                y: 0.0,
                start_x: 0.0,
                start_y: 0.0,
                last_c2_x: 0.0,
                last_c2_y: 0.0,
            }
        }
        fn move_to(&mut self, x: f32, y: f32) {
            self.builder.move_to(x, y);
            self.x = x;
            self.y = y;
            self.start_x = x;
            self.start_y = y;
            self.last_c2_x = x;
            self.last_c2_y = y;
        }
        fn move_by(&mut self, dx: f32, dy: f32) {
            self.move_to(self.x + dx, self.y + dy);
        }
        fn cubic_by(&mut self, dx1: f32, dy1: f32, dx2: f32, dy2: f32, dx: f32, dy: f32) {
            let (x1, y1) = (self.x + dx1, self.y + dy1);
            let (x2, y2) = (self.x + dx2, self.y + dy2);
            let (x, y) = (self.x + dx, self.y + dy);
            self.builder.cubic_to(x1, y1, x2, y2, x, y);
            self.last_c2_x = x2;
            self.last_c2_y = y2;
            self.x = x;
            self.y = y;
        }
        fn smooth_cubic_by(&mut self, dx2: f32, dy2: f32, dx: f32, dy: f32) {
            let (x1, y1) = (2.0 * self.x - self.last_c2_x, 2.0 * self.y - self.last_c2_y);
            let (x2, y2) = (self.x + dx2, self.y + dy2);
            let (x, y) = (self.x + dx, self.y + dy);
            self.builder.cubic_to(x1, y1, x2, y2, x, y);
            self.last_c2_x = x2;
            self.last_c2_y = y2;
            self.x = x;
            self.y = y;
        }
        fn close(&mut self) {
            self.builder.close();
            self.x = self.start_x;
            self.y = self.start_y;
            self.last_c2_x = self.x;
            self.last_c2_y = self.y;
        }
    }

    let mut pen = SvgPen::new();
    pen.move_to(406.6, 231.1);
    pen.cubic_by(-5.2, 0.0, -8.4, -1.3, -12.9, -3.9);
    pen.cubic_by(-71.2, -42.5, -198.5, -52.7, -280.9, -29.7);
    pen.cubic_by(-3.6, 1.0, -8.1, 2.6, -12.9, 2.6);
    pen.cubic_by(-13.2, 0.0, -23.3, -10.3, -23.3, -23.6);
    pen.cubic_by(0.0, -13.6, 8.4, -21.3, 17.4, -23.9);
    pen.cubic_by(35.2, -10.3, 74.6, -15.2, 117.5, -15.2);
    pen.cubic_by(73.0, 0.0, 149.5, 15.2, 205.4, 47.8);
    pen.cubic_by(7.8, 4.5, 12.9, 10.7, 12.9, 22.6);
    pen.cubic_by(0.0, 13.6, -11.0, 23.3, -23.2, 23.3);
    pen.close();

    pen.move_by(-31.0, 76.2);
    pen.cubic_by(-5.2, 0.0, -8.7, -2.3, -12.3, -4.2);
    pen.cubic_by(-62.5, -37.0, -155.7, -51.9, -238.6, -29.4);
    pen.cubic_by(-4.8, 1.3, -7.4, 2.6, -11.9, 2.6);
    pen.cubic_by(-10.7, 0.0, -19.4, -8.7, -19.4, -19.4);
    pen.smooth_cubic_by(5.2, -17.8, 15.5, -20.7);
    pen.cubic_by(27.8, -7.8, 56.2, -13.6, 97.8, -13.6);
    pen.cubic_by(64.9, 0.0, 127.6, 16.1, 177.0, 45.5);
    pen.cubic_by(8.1, 4.8, 11.3, 11.0, 11.3, 19.7);
    pen.cubic_by(-0.1, 10.8, -8.5, 19.5, -19.4, 19.5);
    pen.close();

    pen.move_by(-26.9, 65.6);
    pen.cubic_by(-4.2, 0.0, -6.8, -1.3, -10.7, -3.6);
    pen.cubic_by(-62.4, -37.6, -135.0, -39.2, -206.7, -24.5);
    pen.cubic_by(-3.9, 1.0, -9.0, 2.6, -11.9, 2.6);
    pen.cubic_by(-9.7, 0.0, -15.8, -7.7, -15.8, -15.8);
    pen.cubic_by(0.0, -10.3, 6.1, -15.2, 13.6, -16.8);
    pen.cubic_by(81.9, -18.1, 165.6, -16.5, 237.0, 26.2);
    pen.cubic_by(6.1, 3.9, 9.7, 7.4, 9.7, 16.5);
    pen.smooth_cubic_by(-7.1, 15.4, -15.2, 15.4);
    pen.close();

    pen.builder.finish()
}

/// Rasterises `packaging/icons/spotifast.svg` (`#1ed760` disc with three
/// black wave bars) onto a `size × size` square.
fn mark_rgba(size: usize) -> Vec<u8> {
    const GREEN: [f32; 3] = [30.0, 215.0, 96.0];
    const INK: [f32; 3] = [0.0, 0.0, 0.0];
    let mut rgba = vec![0u8; size * size * 4];
    if size == 0 {
        return rgba;
    }
    let half = size as f32 / 2.0;
    // Keep two pixels of margin so the disc's anti-aliased edge is never clipped.
    let radius = (half - 2.0).max(half * 0.9);
    let scale = radius / 248.0;
    let mut waves = tiny_skia::Pixmap::new(size as u32, size as u32);
    if let (Some(pixmap), Some(path)) = (waves.as_mut(), spotify_waves_path()) {
        let mut paint = tiny_skia::Paint {
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        paint.set_color_rgba8(255, 255, 255, 255);
        let transform = tiny_skia::Transform::from_translate(-248.0, -256.0)
            .post_scale(scale, scale)
            .post_translate(half, half);
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
    }
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - half;
            let dy = y as f32 + 0.5 - half;
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = (radius - distance + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let glyph = waves
                .as_ref()
                .and_then(|pixmap| pixmap.pixel(x as u32, y as u32))
                .map_or(0.0, |pixel| pixel.alpha() as f32 / 255.0);
            let colour = mix(GREEN, INK, glyph);
            let index = (y * size + x) * 4;
            rgba[index] = colour[0].round() as u8;
            rgba[index + 1] = colour[1].round() as u8;
            rgba[index + 2] = colour[2].round() as u8;
            rgba[index + 3] = (coverage * 255.0).round() as u8;
        }
    }
    rgba
}

pub fn greeting(locale: Locale) -> Cow<'static, str> {
    match local_hour() {
        5..=11 => gettext(locale, "Good morning"),
        12..=17 => gettext(locale, "Good afternoon"),
        _ => gettext(locale, "Good evening"),
    }
}

fn local_hour() -> u8 {
    jiff::Zoned::now().hour() as u8
}

/// Strips the HTML Spotify embeds in playlist descriptions.
pub fn strip_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(character),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&#x2F;", "/")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// The temporary files a replacement (fastframe-file) left in `dir`, which
/// a finished or failed one never does.
#[cfg(test)]
pub(crate) fn temporary_files(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".tmp"))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        let index = (y * size + x) * 4;
        [
            rgba[index],
            rgba[index + 1],
            rgba[index + 2],
            rgba[index + 3],
        ]
    }

    /// The icon draws the green disc and three dark waves at every size, and
    /// the tray template punches the waves out of the disc.
    #[test]
    fn the_icon_is_polished_at_every_size() {
        // #given the icon at a dock size and at a tray size
        let (large, small) = (app_icon_rgba(128), app_icon_rgba(32));

        // #then both have the #1ed760 green disc around the waves
        assert_eq!(pixel(&large, 128, 64, 16), [30, 215, 96, 255]);
        assert_eq!(pixel(&small, 32, 16, 4), [30, 215, 96, 255]);

        // #and both carry the dark wave bars across the centre
        for (icon, size) in [(&large, 128), (&small, 32)] {
            let centre = pixel(icon, size, size / 2, size / 2);
            assert!(centre[1] < 100, "wave missing at {size}: {centre:?}");
        }

        // #and the corners stay clear
        assert_eq!(pixel(&large, 128, 1, 1)[3], 0);

        // #and the menu-bar template is the disc with the waves cut out
        let template = tray_template_rgba(44);
        assert_eq!(pixel(&template, 44, 22, 22)[3], 0);
        assert_eq!(pixel(&template, 44, 22, 6), [0, 0, 0, 255]);
    }

    #[test]
    fn radio_stations_map_to_their_seeds_and_back() {
        for kind in ["track", "playlist", "album", "artist"] {
            let seed = format!("spotify:{kind}:4uLU6hMCjMI75M1A2tKUQC");
            let station = station_uri(&seed).expect("a station");
            assert_eq!(
                station,
                format!("spotify:station:{kind}:4uLU6hMCjMI75M1A2tKUQC")
            );
            assert_eq!(station_seed(&station).as_deref(), Some(seed.as_str()));
        }
        for seed in [
            "spotify:show:abc",
            "spotify:episode:abc",
            "spotify:user:me:collection",
            "spotify:track:",
            "track:abc",
        ] {
            assert_eq!(station_uri(seed), None, "{seed}");
        }
        assert_eq!(station_seed("spotify:playlist:abc"), None);
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration_ms(225_000), "3:45");
        assert_eq!(format_duration_ms(3_723_000), "1:02:03");
        assert_eq!(format_total_ms(Locale::English, 7_980_000), "2 hr 13 min");
        assert_eq!(format_total_ms(Locale::English, 2_712_000), "45 min 12 sec");
        assert_eq!(format_episode_ms(Locale::English, 4_320_000), "1 hr 12 min");
    }

    #[test]
    fn counts_and_dates() {
        assert_eq!(format_count(1_234_567), "1,234,567");
        assert_eq!(format_count(12), "12");
        assert_eq!(
            format_date(Locale::English, "2024-01-05T10:00:00Z"),
            "Jan 5, 2024"
        );
        assert_eq!(format_date(Locale::English, "2024-03"), "Mar 2024");
        assert_eq!(format_date(Locale::English, "2024"), "2024");
    }

    #[test]
    fn recent_dates_are_relative_for_the_first_month() {
        let now: jiff::Timestamp = "2026-08-31T12:00:00Z".parse().unwrap();
        assert_eq!(
            format_relative_date(Locale::English, "2026-08-31T11:59:30Z", now),
            "30 seconds ago"
        );
        assert_eq!(
            format_relative_date(Locale::English, "2026-08-31T11:59:00Z", now),
            "1 minute ago"
        );
        assert_eq!(
            format_relative_date(Locale::English, "2026-08-31T11:00:00Z", now),
            "1 hour ago"
        );
        assert_eq!(
            format_relative_date(Locale::English, "2026-08-30T12:00:00Z", now),
            "1 day ago"
        );
        assert_eq!(
            format_relative_date(Locale::English, "2026-08-17T12:00:00Z", now),
            "2 weeks ago"
        );
        assert_eq!(
            format_relative_date(Locale::English, "2026-08-01T12:00:00Z", now),
            "Aug 1, 2026"
        );
    }

    #[test]
    fn dates_and_lengths_follow_the_interface_language() {
        let now: jiff::Timestamp = "2026-08-31T12:00:00Z".parse().unwrap();
        assert_eq!(format_date(Locale::Spanish, "2024-01-05"), "5 ene 2024");
        assert_eq!(format_date(Locale::Spanish, "2024-09"), "sept 2024");
        assert_eq!(format_total_ms(Locale::Spanish, 7_980_000), "2 h 13 min");
        assert_eq!(format_episode_ms(Locale::Spanish, 2_280_000), "38 min");
        for (added, expected) in [
            ("2026-08-31T11:59:59Z", "hace 1 segundo"),
            ("2026-08-31T11:58:00Z", "hace 2 minutos"),
            ("2026-08-30T12:00:00Z", "hace 1 día"),
            ("2026-08-17T12:00:00Z", "hace 2 semanas"),
        ] {
            assert_eq!(format_relative_date(Locale::Spanish, added, now), expected);
        }
        // Each language orders the date its own way.
        assert_eq!(format_date(Locale::Swedish, "2024-01-05"), "5 jan. 2024");
        assert_eq!(format_date(Locale::English, "2024-01-05"), "Jan 5, 2024");
        assert_eq!(
            format_relative_date(Locale::Swedish, "2026-08-30T12:00:00Z", now),
            "för 1 dag sedan"
        );
        assert_eq!(format_date(Locale::Turkish, "2024-01-05"), "5 Oca 2024");
        assert_eq!(format_date(Locale::Turkish, "2024-09"), "Eyl 2024");
        assert_eq!(format_total_ms(Locale::Turkish, 7_980_000), "2 sa 13 dk");
        assert_eq!(format_episode_ms(Locale::Turkish, 2_280_000), "38 dk");
        for (added, expected) in [
            ("2026-08-31T11:59:59Z", "1 saniye önce"),
            ("2026-08-31T11:58:00Z", "2 dakika önce"),
            ("2026-08-30T12:00:00Z", "1 gün önce"),
            ("2026-08-17T12:00:00Z", "2 hafta önce"),
        ] {
            assert_eq!(format_relative_date(Locale::Turkish, added, now), expected);
        }
        assert_eq!(
            format_date(Locale::Ukrainian, "2024-01-05"),
            "5 січ. 2024 р."
        );
        assert_eq!(format_date(Locale::Ukrainian, "2024-09"), "вер. 2024 р.");
        assert_eq!(format_total_ms(Locale::Ukrainian, 7_980_000), "2 год 13 хв");
        assert_eq!(format_episode_ms(Locale::Ukrainian, 2_280_000), "38 хв");
        for (added, expected) in [
            ("2026-08-31T11:59:59Z", "1 секунду тому"),
            ("2026-08-31T11:58:00Z", "2 хвилини тому"),
            ("2026-08-31T07:00:00Z", "5 годин тому"),
            ("2026-08-30T12:00:00Z", "1 день тому"),
            ("2026-08-17T12:00:00Z", "2 тижні тому"),
        ] {
            assert_eq!(
                format_relative_date(Locale::Ukrainian, added, now),
                expected
            );
        }
        assert_eq!(format_date(Locale::Korean, "2024-01-05"), "2024년 1월 5일");
        assert_eq!(format_date(Locale::Korean, "2024-09"), "2024년 9월");
        assert_eq!(format_total_ms(Locale::Korean, 7_980_000), "2시간 13분");
        assert_eq!(format_episode_ms(Locale::Korean, 2_280_000), "38분");
        for (added, expected) in [
            ("2026-08-31T11:59:59Z", "1초 전"),
            ("2026-08-31T11:58:00Z", "2분 전"),
            ("2026-08-31T07:00:00Z", "5시간 전"),
            ("2026-08-30T12:00:00Z", "1일 전"),
            ("2026-08-17T12:00:00Z", "2주 전"),
        ] {
            assert_eq!(format_relative_date(Locale::Korean, added, now), expected);
        }
    }

    #[test]
    fn relative_dates_fall_back_for_future_and_invalid_timestamps() {
        let now: jiff::Timestamp = "2026-08-31T12:00:00Z".parse().unwrap();
        assert_eq!(
            format_relative_date(Locale::English, "2026-09-01T12:00:00Z", now),
            "Sep 1, 2026"
        );
        assert_eq!(
            format_relative_date(Locale::English, "not-a-date", now),
            "not-a-date"
        );
    }

    #[test]
    fn uris() {
        assert_eq!(uri_id("spotify:track:abc"), Some("abc"));
        assert_eq!(uri_kind("spotify:playlist:x"), Some("playlist"));
        assert_eq!(
            open_spotify_url("spotify:album:z").as_deref(),
            Some("https://open.spotify.com/album/z")
        );
    }

    #[test]
    fn html_is_stripped() {
        assert_eq!(
            strip_html("Hi <a href=\"x\">there</a> &amp; you"),
            "Hi there & you"
        );
        assert_eq!(strip_html("ONE&#x2F;TWO&#x2F;THREE"), "ONE/TWO/THREE");
    }
}
