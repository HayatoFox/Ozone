//! Client natif Ozone — application Iced.
//!
//! Architecture **Elm** (`Message`/`update`/`view`) dans `app.rs`, branchée sur `ozone-core`
//! (REST typé). Premier écran : **connexion à une instance** ; puis guildes/salons/messages.
//! Cf. `docs/11-client.md`. Le temps réel (Gateway) et le vocal viendront ensuite.

mod app;
mod style;
mod theme;

use app::App;

fn main() -> iced::Result {
    iced::application("Ozone", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .window(iced::window::Settings {
            size: iced::Size::new(1180.0, 760.0),
            min_size: Some(iced::Size::new(940.0, 560.0)),
            ..iced::window::Settings::default()
        })
        .run_with(App::new)
}
