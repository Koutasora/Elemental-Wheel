//! Labels of the demo wheel (from `docs/koncept.html`), used until the config arrives.

use crate::geometry::clamp_outer_sectors;

/// Inner ring: edit commands. Cut, Copy and Paste are at the top (11, 0, 1),
/// Undo on the left (9), Redo on the right (3).
pub const INNER_LABELS: [&str; 12] = [
    "Kopiuj",
    "Wklej",
    "Bez formatu",
    "Ponów",
    "Zapisz",
    "Znajdź",
    "Usuń",
    "Właściwości",
    "Zmień nazwę",
    "Cofnij",
    "Zaznacz wszystko",
    "Wytnij",
];

/// Outer ring: programs and actions.
pub const OUTER_LABELS: [&str; 12] = [
    "Terminal",
    "Przeglądarka",
    "Pliki",
    "Zrzut ekranu",
    "Głośność",
    "Schowek",
    "Zamknij okno",
    "Blender",
    "Ableton Live",
    "Godot",
    "VS Code",
    "Zablokuj",
];

/// The first `n` outer labels (`n` is clamped to the allowed sector range).
pub fn outer_labels(n: u8) -> &'static [&'static str] {
    &OUTER_LABELS[..clamp_outer_sectors(n) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_labels_length() {
        for (n, want) in [
            (0, 4),
            (3, 4),
            (4, 4),
            (8, 8),
            (12, 12),
            (13, 12),
            (255, 12),
        ] {
            assert_eq!(outer_labels(n).len(), want, "n={n}");
        }
        assert_eq!(outer_labels(4)[0], OUTER_LABELS[0]);
    }

    #[test]
    fn undo_left_redo_right() {
        assert_eq!(INNER_LABELS[9], "Cofnij");
        assert_eq!(INNER_LABELS[3], "Ponów");
        assert_eq!(INNER_LABELS[11], "Wytnij");
        assert_eq!(INNER_LABELS[0], "Kopiuj");
        assert_eq!(INNER_LABELS[1], "Wklej");
    }
}
