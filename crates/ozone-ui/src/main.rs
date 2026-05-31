//! Client natif Ozone — application Iced.
//!
//! Architecture **Elm** (`Message`/`update`/`view`) dans `app.rs`, branchée sur `ozone-core`
//! (REST typé). Premier écran : **connexion à une instance** ; puis guildes/salons/messages.
//! Cf. `docs/11-client.md`. Le temps réel (Gateway) et le vocal viendront ensuite.

mod app;
mod theme;

use app::App;

fn main() -> iced::Result {
    iced::application("Ozone", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .run_with(App::new)
}
