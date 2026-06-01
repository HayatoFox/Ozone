//! Style « façon Discord » : palette + fonctions de style par widget (Iced 0.13).
//!
//! Centralise les couleurs et les styles (conteneurs, boutons, champs) pour reproduire
//! l'esthétique de Discord (rail de serveurs sombre, sidebar, zone de chat, accents blurple).

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

/// Palette Discord (thème sombre).
pub mod color {
    use iced::Color;

    pub fn deepest() -> Color {
        Color::from_rgb8(0x1e, 0x1f, 0x22)
    } // rail serveurs / fond profond
    pub fn sidebar() -> Color {
        Color::from_rgb8(0x2b, 0x2d, 0x31)
    } // sidebar salons
    pub fn chat() -> Color {
        Color::from_rgb8(0x31, 0x33, 0x38)
    } // zone de chat
    pub fn input() -> Color {
        Color::from_rgb8(0x38, 0x3a, 0x40)
    } // champs / composeur
    pub fn hover() -> Color {
        Color::from_rgb8(0x35, 0x37, 0x3c)
    } // survol sidebar
    pub fn selected() -> Color {
        Color::from_rgb8(0x40, 0x42, 0x49)
    } // salon sélectionné
    pub fn user_panel() -> Color {
        Color::from_rgb8(0x23, 0x24, 0x28)
    } // panneau utilisateur
    pub fn blurple() -> Color {
        Color::from_rgb8(0x58, 0x65, 0xf2)
    }
    pub fn blurple_hover() -> Color {
        Color::from_rgb8(0x4a, 0x55, 0xd0)
    }
    pub fn green() -> Color {
        Color::from_rgb8(0x23, 0xa5, 0x5a)
    }
    pub fn text() -> Color {
        Color::from_rgb8(0xdb, 0xde, 0xe1)
    } // texte normal
    pub fn muted() -> Color {
        Color::from_rgb8(0x94, 0x9b, 0xa4)
    } // texte atténué
    pub fn channel() -> Color {
        Color::from_rgb8(0x80, 0x84, 0x8e)
    } // # salon au repos
    pub fn header() -> Color {
        Color::from_rgb8(0xf2, 0xf3, 0xf5)
    } // titres clairs
    pub fn white() -> Color {
        Color::WHITE
    }
}

fn round(radius: f32) -> Border {
    Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: radius.into(),
    }
}

fn filled(bg: Color) -> container::Style {
    container::Style {
        text_color: None,
        background: Some(Background::Color(bg)),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

// ─────────────── Conteneurs ───────────────

pub fn rail_bg(_t: &Theme) -> container::Style {
    filled(color::deepest())
}
pub fn sidebar_bg(_t: &Theme) -> container::Style {
    filled(color::sidebar())
}
pub fn chat_bg(_t: &Theme) -> container::Style {
    filled(color::chat())
}
pub fn user_panel_bg(_t: &Theme) -> container::Style {
    filled(color::user_panel())
}

/// En-tête avec une légère ombre portée vers le bas (effet d'élévation Discord).
pub fn header_bar(_t: &Theme) -> container::Style {
    container::Style {
        text_color: Some(color::header()),
        background: Some(Background::Color(color::chat())),
        border: Border::default(),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
    }
}

/// Carte centrale (écrans de connexion / ajout d'instance).
pub fn card(_t: &Theme) -> container::Style {
    container::Style {
        text_color: Some(color::text()),
        background: Some(Background::Color(color::sidebar())),
        border: round(8.0),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 12.0,
        },
    }
}

/// Pastille colorée (avatar par initiale, point de présence).
pub fn dot(bg: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_t| container::Style {
        text_color: Some(color::white()),
        background: Some(Background::Color(bg)),
        border: round(radius),
        shadow: Shadow::default(),
    }
}

// ─────────────── Boutons ───────────────

fn btn(bg: Option<Color>, text_color: Color, radius: f32) -> button::Style {
    button::Style {
        background: bg.map(Background::Color),
        text_color,
        border: round(radius),
        shadow: Shadow::default(),
    }
}

fn is_hot(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

/// Icône de serveur dans le rail (arrondie au repos, « squircle » blurple si actif/survol).
pub fn guild_icon(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_t, status| {
        let hot = selected || is_hot(status);
        let bg = if hot {
            color::blurple()
        } else {
            color::input()
        };
        let radius = if hot { 16.0 } else { 24.0 };
        btn(Some(bg), color::white(), radius)
    }
}

/// Entrée de salon dans la sidebar (survol/sélection mis en évidence).
pub fn channel_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_t, status| {
        let bg = if selected {
            Some(color::selected())
        } else if is_hot(status) {
            Some(color::hover())
        } else {
            None
        };
        let txt = if selected {
            color::white()
        } else {
            color::channel()
        };
        btn(bg, txt, 4.0)
    }
}

/// Bouton discret (survol = léger fond). Utilisé pour les actions de la sidebar/panneaux.
pub fn subtle(_t: &Theme, status: button::Status) -> button::Style {
    let bg = if is_hot(status) {
        Some(color::hover())
    } else {
        None
    };
    btn(bg, color::muted(), 4.0)
}

/// Bouton principal (blurple plein).
pub fn primary(_t: &Theme, status: button::Status) -> button::Style {
    let bg = if is_hot(status) {
        color::blurple_hover()
    } else {
        color::blurple()
    };
    btn(Some(bg), color::white(), 4.0)
}

/// Lien-bouton (texte blurple, sans fond) — bascules « créer un compte / se connecter ».
pub fn link(_t: &Theme, status: button::Status) -> button::Style {
    let txt = if is_hot(status) {
        color::white()
    } else {
        color::blurple()
    };
    btn(None, txt, 4.0)
}

// ─────────────── Champs de saisie ───────────────

pub fn input_field(_t: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(color::input()),
        border: round(8.0),
        icon: color::muted(),
        placeholder: color::muted(),
        value: color::text(),
        selection: color::blurple(),
    }
}

/// Composeur de message (fond arrondi plus clair, sans bordure visible).
pub fn composer_field(_t: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(color::input()),
        border: round(8.0),
        icon: color::muted(),
        placeholder: color::muted(),
        value: color::text(),
        selection: color::blurple(),
    }
}
