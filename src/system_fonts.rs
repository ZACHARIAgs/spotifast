//! The installed face the Winamp playlist draws its text in.
//!
//! The fallbacks for scripts Inter does not cover come from
//! `fastframe_fonts::system`; this looks through the same font directories
//! for one family by name.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use skrifa::MetadataProvider as _;

/// A face from an installed font file.
pub struct PlaylistFace {
    pub bytes: Vec<u8>,
    pub index: u32,
}

/// How deep to walk each font directory. Distributions nest a level or two
/// (`/usr/share/fonts/truetype/noto`); nothing legitimate goes deeper, and the
/// bound also ends any symlink loop.
const FONT_SCAN_DEPTH: usize = 4;

/// A collection says how many faces it holds, and a corrupt or hostile file
/// can say billions. No real one holds more than a few dozen.
const MAX_FACES: u32 = 64;

/// The face Winamp's playlists were drawn in, or the nearest this desktop
/// carries: Arial, then the metric twins that stand in for it on Linux,
/// then a plain sans. `None` leaves it to the interface font.
pub fn pledit_face() -> Option<&'static PlaylistFace> {
    static FACE: OnceLock<Option<PlaylistFace>> = OnceLock::new();
    FACE.get_or_init(|| {
        find_family(&[
            "arial",
            "liberation sans",
            "arimo",
            "helvetica",
            "helvetica neue",
            "nimbus sans",
            "dejavu sans",
        ])
    })
    .as_ref()
}

/// The regular face of the first family in `wanted` that is installed.
fn find_family(wanted: &[&str]) -> Option<PlaylistFace> {
    let mut best: Option<(usize, PathBuf, u32)> = None;
    for dir in fastframe_fonts::system::font_directories() {
        walk_fonts(&dir, 0, &mut |path| rank_family(path, wanted, &mut best));
    }
    let (_, path, index) = best?;
    let bytes = std::fs::read(&path).ok()?;
    log::debug!("playlist face: {} (face {index})", path.display());
    Some(PlaylistFace { bytes, index })
}

/// Visits every font file below `dir`.
fn walk_fonts(dir: &Path, depth: usize, visit: &mut dyn FnMut(&Path)) {
    if depth >= FONT_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
            walk_fonts(&path, depth + 1, visit);
        } else if is_font_file(&path) {
            visit(&path);
        }
    }
}

/// Whether a path names a font this can open.
fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        })
}

/// Keeps the file if it holds a regular face of a wanted family that ranks
/// above the one held so far.
fn rank_family(path: &Path, wanted: &[&str], best: &mut Option<(usize, PathBuf, u32)>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    // Safety: a read-only mapping that lives inside this call, so only the
    // pages holding the names are read. A font file rewritten underneath it
    // meanwhile could fault, the bet every font enumerator makes.
    let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
        return;
    };
    let faces: Vec<(u32, skrifa::FontRef)> = match skrifa::raw::FileRef::new(&map) {
        Ok(skrifa::raw::FileRef::Font(font)) => vec![(0, font)],
        Ok(skrifa::raw::FileRef::Collection(collection)) => (0..collection.len().min(MAX_FACES))
            .filter_map(|index| collection.get(index).ok().map(|font| (index, font)))
            .collect(),
        Err(_) => return,
    };
    for (index, font) in faces {
        let attributes = font.attributes();
        if attributes.style != skrifa::attribute::Style::Normal
            || !(350.0..=450.0).contains(&attributes.weight.value())
        {
            continue;
        }
        let family = font
            .localized_strings(skrifa::string::StringId::FAMILY_NAME)
            .english_or_first()
            .map(|name| name.to_string())
            .unwrap_or_default()
            .to_lowercase();
        let Some(rank) = wanted.iter().position(|name| *name == family) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(held, held_path, _)| (rank, path) < (*held, held_path.as_path()))
        {
            *best = Some((rank, path.to_path_buf(), index));
        }
    }
}

/// Installed font families suitable for UI / playlist / album text, sorted
/// alphabetically.
pub fn available_families() -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let mut by_lower: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for dir in fastframe_fonts::system::font_directories() {
            walk_fonts(&dir, 0, &mut |path| collect_families(path, &mut by_lower));
        }
        let mut list: Vec<String> = by_lower.into_values().collect();
        list.sort_by_key(|a| a.to_lowercase());
        list
    })
}

fn collect_families(path: &Path, by_lower: &mut std::collections::BTreeMap<String, String>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
        return;
    };
    let faces: Vec<(u32, skrifa::FontRef)> = match skrifa::raw::FileRef::new(&map) {
        Ok(skrifa::raw::FileRef::Font(font)) => vec![(0, font)],
        Ok(skrifa::raw::FileRef::Collection(collection)) => (0..collection.len().min(MAX_FACES))
            .filter_map(|index| collection.get(index).ok().map(|font| (index, font)))
            .collect(),
        Err(_) => return,
    };
    for (_, font) in faces {
        let attributes = font.attributes();
        if attributes.style != skrifa::attribute::Style::Normal
            || !(300.0..=500.0).contains(&attributes.weight.value())
        {
            continue;
        }
        let Some(family) = font
            .localized_strings(skrifa::string::StringId::FAMILY_NAME)
            .english_or_first()
            .map(|name| name.to_string())
        else {
            continue;
        };
        let trimmed = family.trim();
        if trimmed.is_empty() || trimmed.starts_with('.') || trimmed.starts_with('@') {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if [
            "emoji",
            "dingbats",
            "webdings",
            "wingdings",
            "marlett",
            "mdl2",
            "fluent icons",
            "symbol",
            "math",
            "braille",
        ]
        .iter()
        .any(|skip| lower.contains(skip))
        {
            continue;
        }
        by_lower.entry(lower).or_insert_with(|| trimmed.to_string());
    }
}

/// Loads the regular face and (if available) bold face for `family`.
pub fn load_family_faces(family: &str) -> Option<(PlaylistFace, Option<PlaylistFace>)> {
    let target = family.trim().to_lowercase();
    if target.is_empty() {
        return None;
    }
    let mut best_regular: Option<(u32, PathBuf, u32)> = None;
    let mut best_bold: Option<(u32, PathBuf, u32)> = None;
    for dir in fastframe_fonts::system::font_directories() {
        walk_fonts(&dir, 0, &mut |path| {
            rank_regular_and_bold(path, &target, &mut best_regular, &mut best_bold)
        });
    }
    let (_, reg_path, reg_index) = best_regular?;
    let reg_bytes = std::fs::read(&reg_path).ok()?;
    let regular = PlaylistFace {
        bytes: reg_bytes,
        index: reg_index,
    };
    let bold = best_bold.and_then(|(_, bold_path, bold_index)| {
        let bytes = std::fs::read(&bold_path).ok()?;
        Some(PlaylistFace {
            bytes,
            index: bold_index,
        })
    });
    Some((regular, bold))
}

fn rank_regular_and_bold(
    path: &Path,
    target_lower: &str,
    best_regular: &mut Option<(u32, PathBuf, u32)>,
    best_bold: &mut Option<(u32, PathBuf, u32)>,
) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
        return;
    };
    let faces: Vec<(u32, skrifa::FontRef)> = match skrifa::raw::FileRef::new(&map) {
        Ok(skrifa::raw::FileRef::Font(font)) => vec![(0, font)],
        Ok(skrifa::raw::FileRef::Collection(collection)) => (0..collection.len().min(MAX_FACES))
            .filter_map(|index| collection.get(index).ok().map(|font| (index, font)))
            .collect(),
        Err(_) => return,
    };
    for (index, font) in faces {
        let attributes = font.attributes();
        if attributes.style != skrifa::attribute::Style::Normal {
            continue;
        }
        let family = font
            .localized_strings(skrifa::string::StringId::FAMILY_NAME)
            .english_or_first()
            .map(|name| name.to_string())
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if family != target_lower {
            continue;
        }
        let weight = attributes.weight.value();
        let reg_dist = (weight - 400.0).abs().round() as u32;
        if reg_dist <= 150
            && best_regular
                .as_ref()
                .is_none_or(|(d, p, _)| (reg_dist, path) < (*d, p.as_path()))
        {
            *best_regular = Some((reg_dist, path.to_path_buf(), index));
        }
        if (550.0..=900.0).contains(&weight) {
            let bold_dist = (weight - 700.0).abs().round() as u32;
            if best_bold
                .as_ref()
                .is_none_or(|(d, p, _)| (bold_dist, path) < (*d, p.as_path()))
            {
                *best_bold = Some((bold_dist, path.to_path_buf(), index));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_font_files_are_probed() {
        assert!(is_font_file(Path::new("/x/NotoSans.ttf")));
        assert!(is_font_file(Path::new("/x/NotoSansCJK.TTC")));
        assert!(is_font_file(Path::new("/x/PingFang.otf")));
        assert!(!is_font_file(Path::new("/x/fonts.dir")));
        assert!(!is_font_file(Path::new("/x/README")));
    }

    #[test]
    fn asking_the_system_never_panics() {
        // Whatever fonts this machine has, including none.
        let _ = pledit_face();
    }
}
