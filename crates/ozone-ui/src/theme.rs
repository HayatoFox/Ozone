//! Thèmes de l'UI : palettes **de marque** « Ozone » (sombre/clair) + thèmes intégrés d'Iced.
//!
//! Iced applique la **palette** à tous les widgets (fond, texte, accent, succès, danger) : changer
//! de [`ThemeChoice`] reteinte l'ensemble de l'application. Base pour des effets/décors plus poussés
//! à terme (styles par widget, widget `shader` wgpu).

use iced::theme::Palette;
use iced::{Color, Theme};

/// Choix de thème exposé à l'utilisateur (cyclable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    OzoneDark,
    OzoneLight,
    Dark,
    Light,
}

impl ThemeChoice {
    /// Thème suivant dans le cycle (pour un bouton « changer de thème »).
    pub fn next(self) -> Self {
        match self {
            ThemeChoice::OzoneDark => ThemeChoice::OzoneLight,
            ThemeChoice::OzoneLight => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::OzoneDark,
        }
    }

    /// Thème Iced concret (palette de marque pour les variantes « Ozone »).
    pub fn to_theme(self) -> Theme {
        match self {
            ThemeChoice::OzoneDark => Theme::custom(
                "Ozone sombre".to_string(),
                Palette {
                    background: Color::from_rgb8(0x1a, 0x1b, 0x26),
                    text: Color::from_rgb8(0xc0, 0xc6, 0xe0),
                    primary: Color::from_rgb8(0x7a, 0x5c, 0xff), // accent violet « Ozone »
                    success: Color::from_rgb8(0x3f, 0xb9, 0x50),
                    danger: Color::from_rgb8(0xe5, 0x4b, 0x4b),
                },
            ),
            ThemeChoice::OzoneLight => Theme::custom(
                "Ozone clair".to_string(),
                Palette {
                    background: Color::from_rgb8(0xf6, 0xf6, 0xfb),
                    text: Color::from_rgb8(0x20, 0x22, 0x30),
                    primary: Color::from_rgb8(0x6a, 0x4c, 0xef),
                    success: Color::from_rgb8(0x2f, 0x9e, 0x44),
                    danger: Color::from_rgb8(0xd0, 0x3b, 0x3b),
                },
            ),
            ThemeChoice::Dark => Theme::Dark,
            ThemeChoice::Light => Theme::Light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_cycles_through_all_and_returns_to_start() {
        let start = ThemeChoice::default();
        let mut t = start;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(t);
            t = t.next();
        }
        assert_eq!(t, start, "le cycle revient au départ après 4 pas");
        // Les 4 choix sont distincts.
        seen.dedup();
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn every_choice_builds_a_theme() {
        for c in [
            ThemeChoice::OzoneDark,
            ThemeChoice::OzoneLight,
            ThemeChoice::Dark,
            ThemeChoice::Light,
        ] {
            let _ = c.to_theme(); // ne panique pas
        }
    }
}
