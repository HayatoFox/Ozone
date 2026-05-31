//! Application Iced (Elm) : état, messages, `update`, `view`, thème.
//!
//! La logique réseau passe par `ozone_core::ApiClient` (REST typé, `Clone`), invoquée dans des
//! `Task` asynchrones dont le **résultat revient sous forme de `Message`** ; l'état affichable vit
//! dans `App` (lisible synchroniquement par `view`). Le rendu n'exécute jamais le contenu : les
//! messages sont affichés en **texte brut** (pas de markup/HTML) — aucune exécution côté UI.

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};
use ozone_core::proto::dto::{Channel, Guild, Message as ChatMessage, TokenPair};
use ozone_core::proto::Snowflake;
use ozone_core::{ApiClient, InstanceRef};

/// Événements de l'UI (entrées utilisateur + résultats des `Task` réseau).
///
/// `Message` est le nom conventionnel d'Iced ; plusieurs variantes mentionnent « message » (le
/// domaine de l'app), d'où l'`allow` ciblé. Les gros payloads (`TokenPair`, un `ChatMessage`) sont
/// **boxés** pour garder des variantes de taille homogène (déplacements bon marché dans la file Iced).
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum Message {
    AddressChanged(String),
    LoginChanged(String),
    PasswordChanged(String),
    SubmitLogin,
    LoggedIn(Result<Box<TokenPair>, String>),
    GuildsLoaded(Result<Vec<Guild>, String>),
    SelectGuild(i64),
    ChannelsLoaded(Result<Vec<Channel>, String>),
    SelectChannel(i64),
    MessagesLoaded(Result<Vec<ChatMessage>, String>),
    ComposerChanged(String),
    SendMessage,
    MessageSent(Result<Box<ChatMessage>, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Screen {
    #[default]
    Connect,
    Loading,
    Main,
}

/// État de l'application (une instance connectée).
pub struct App {
    screen: Screen,
    api: ApiClient,
    // Formulaire de connexion.
    address: String,
    login: String,
    password: String,
    status: String,
    // Données affichées.
    guilds: Vec<Guild>,
    channels: Vec<Channel>,
    messages: Vec<ChatMessage>,
    selected_guild: Option<i64>,
    selected_channel: Option<i64>,
    composer: String,
}

/// Le formulaire de connexion est-il complet ?
fn can_connect(address: &str, login: &str, password: &str) -> bool {
    !address.trim().is_empty() && !login.trim().is_empty() && !password.is_empty()
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let address = "http://127.0.0.1:8080".to_string();
        (
            Self {
                screen: Screen::Connect,
                api: ApiClient::new(&address),
                address,
                login: String::new(),
                password: String::new(),
                status: String::new(),
                guilds: Vec::new(),
                channels: Vec::new(),
                messages: Vec::new(),
                selected_guild: None,
                selected_channel: None,
                composer: String::new(),
            },
            Task::none(),
        )
    }

    pub fn theme(&self) -> Theme {
        // Thème sombre par défaut ; base pour des thèmes/effets personnalisés à terme.
        Theme::Dark
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddressChanged(v) => {
                self.address = v;
                Task::none()
            }
            Message::LoginChanged(v) => {
                self.login = v;
                Task::none()
            }
            Message::PasswordChanged(v) => {
                self.password = v;
                Task::none()
            }
            Message::SubmitLogin => {
                if !can_connect(&self.address, &self.login, &self.password) {
                    self.status = "Renseigne l'adresse, l'identifiant et le mot de passe.".into();
                    return Task::none();
                }
                // (Re)construit le client REST pour l'instance saisie.
                self.api = ApiClient::new(InstanceRef::new(self.address.trim()).api_base());
                self.screen = Screen::Loading;
                self.status = "Connexion…".into();
                let api = self.api.clone();
                let login = self.login.clone();
                let password = self.password.clone();
                Task::perform(
                    async move {
                        api.login(&login, &password)
                            .await
                            .map(Box::new)
                            .map_err(|e| e.to_string())
                    },
                    Message::LoggedIn,
                )
            }
            Message::LoggedIn(Ok(tokens)) => {
                self.api.set_token(Some(tokens.access_token));
                self.password.clear(); // ne pas conserver le mot de passe en mémoire après usage
                self.status.clear();
                let api = self.api.clone();
                Task::perform(
                    async move { api.list_guilds().await.map_err(|e| e.to_string()) },
                    Message::GuildsLoaded,
                )
            }
            Message::LoggedIn(Err(e)) => {
                self.screen = Screen::Connect;
                self.status = format!("Échec de connexion : {e}");
                Task::none()
            }
            Message::GuildsLoaded(Ok(guilds)) => {
                self.guilds = guilds;
                self.screen = Screen::Main;
                if let Some(first) = self.guilds.first() {
                    let gid = first.id.as_i64();
                    return self.update(Message::SelectGuild(gid));
                }
                Task::none()
            }
            Message::GuildsLoaded(Err(e)) => {
                self.status = format!("Guildes : {e}");
                Task::none()
            }
            Message::SelectGuild(gid) => {
                self.selected_guild = Some(gid);
                self.channels.clear();
                self.messages.clear();
                self.selected_channel = None;
                let api = self.api.clone();
                Task::perform(
                    async move {
                        api.list_channels(Snowflake::from_i64(gid))
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::ChannelsLoaded,
                )
            }
            Message::ChannelsLoaded(Ok(channels)) => {
                self.channels = channels;
                // Sélectionne le premier salon textuel (type 0).
                if let Some(first) = self.channels.iter().find(|c| c.kind == 0) {
                    let cid = first.id.as_i64();
                    return self.update(Message::SelectChannel(cid));
                }
                Task::none()
            }
            Message::ChannelsLoaded(Err(e)) => {
                self.status = format!("Salons : {e}");
                Task::none()
            }
            Message::SelectChannel(cid) => {
                self.selected_channel = Some(cid);
                self.messages.clear();
                let api = self.api.clone();
                Task::perform(
                    async move {
                        api.list_messages(Snowflake::from_i64(cid))
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::MessagesLoaded,
                )
            }
            Message::MessagesLoaded(Ok(messages)) => {
                self.messages = messages;
                Task::none()
            }
            Message::MessagesLoaded(Err(e)) => {
                self.status = format!("Messages : {e}");
                Task::none()
            }
            Message::ComposerChanged(v) => {
                self.composer = v;
                Task::none()
            }
            Message::SendMessage => {
                let content = self.composer.trim().to_string();
                let Some(cid) = self.selected_channel else {
                    return Task::none();
                };
                if content.is_empty() {
                    return Task::none();
                }
                self.composer.clear();
                let api = self.api.clone();
                Task::perform(
                    async move {
                        api.send_message(Snowflake::from_i64(cid), &content)
                            .await
                            .map(Box::new)
                            .map_err(|e| e.to_string())
                    },
                    Message::MessageSent,
                )
            }
            Message::MessageSent(Ok(m)) => {
                // Affichage optimiste ; l'écho Gateway temps réel remplacera ce chemin plus tard.
                self.messages.push(*m);
                Task::none()
            }
            Message::MessageSent(Err(e)) => {
                self.status = format!("Envoi : {e}");
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::Connect | Screen::Loading => self.connect_view(),
            Screen::Main => self.main_view(),
        }
    }

    fn connect_view(&self) -> Element<'_, Message> {
        let connecting = self.screen == Screen::Loading;
        let mut submit = button(text("Se connecter")).padding(10);
        if !connecting {
            submit = submit.on_press(Message::SubmitLogin);
        }
        let form = column![
            text("Ozone").size(32),
            text("Connexion à une instance").size(16),
            text_input("https://mon-instance", &self.address)
                .on_input(Message::AddressChanged)
                .padding(8),
            text_input("identifiant ou e-mail", &self.login)
                .on_input(Message::LoginChanged)
                .padding(8),
            text_input("mot de passe", &self.password)
                .on_input(Message::PasswordChanged)
                .on_submit(Message::SubmitLogin)
                .secure(true)
                .padding(8),
            submit,
            text(self.status.clone()),
        ]
        .spacing(12)
        .max_width(360);

        container(form)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(20)
            .into()
    }

    fn main_view(&self) -> Element<'_, Message> {
        // Barre des guildes.
        let mut guilds = column![text("Guildes").size(14)].spacing(6).padding(8);
        for g in &self.guilds {
            let selected = self.selected_guild == Some(g.id.as_i64());
            let label = if selected {
                format!("▸ {}", g.name)
            } else {
                g.name.clone()
            };
            guilds = guilds.push(
                button(text(label))
                    .width(Length::Fill)
                    .on_press(Message::SelectGuild(g.id.as_i64())),
            );
        }

        // Liste des salons (texte = 0, vocal = 2).
        let mut channels = column![text("Salons").size(14)].spacing(6).padding(8);
        for c in self.channels.iter().filter(|c| c.kind == 0 || c.kind == 2) {
            let prefix = if c.kind == 2 { "🔊" } else { "#" };
            channels = channels.push(
                button(text(format!("{prefix} {}", c.name)))
                    .width(Length::Fill)
                    .on_press(Message::SelectChannel(c.id.as_i64())),
            );
        }

        // Fil de messages.
        let mut feed = column![].spacing(10).padding(8);
        for m in &self.messages {
            let author = m
                .author
                .display_name
                .clone()
                .unwrap_or_else(|| m.author.username.clone());
            feed = feed
                .push(column![text(author).size(13), text(m.content.clone()).size(15)].spacing(2));
        }
        let feed = scrollable(feed).height(Length::Fill);

        let composer = row![
            text_input("Message…", &self.composer)
                .on_input(Message::ComposerChanged)
                .on_submit(Message::SendMessage)
                .padding(8),
            button(text("Envoyer"))
                .on_press(Message::SendMessage)
                .padding(8),
        ]
        .spacing(8)
        .padding(8);

        let main = column![feed, composer]
            .width(Length::Fill)
            .height(Length::Fill);

        row![
            container(guilds)
                .width(Length::Fixed(190.0))
                .height(Length::Fill),
            container(channels)
                .width(Length::Fixed(210.0))
                .height(Length::Fill),
            container(main).width(Length::Fill).height(Length::Fill),
        ]
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_connect_form() {
        assert!(!can_connect("", "a", "b"));
        assert!(!can_connect("  ", "a", "b"));
        assert!(!can_connect("x", "", "b"));
        assert!(!can_connect("x", "a", ""));
        assert!(can_connect("x", "a", "b"));
    }

    #[test]
    fn reducer_updates_form_and_selection() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::AddressChanged("https://h".into()));
        assert_eq!(app.address, "https://h");
        let _ = app.update(Message::LoginChanged("bob".into()));
        assert_eq!(app.login, "bob");
        let _ = app.update(Message::ComposerChanged("salut".into()));
        assert_eq!(app.composer, "salut");
        // La sélection est enregistrée (la Task de chargement n'est pas exécutée hors runtime Iced).
        let _ = app.update(Message::SelectGuild(42));
        assert_eq!(app.selected_guild, Some(42));
        let _ = app.update(Message::SelectChannel(7));
        assert_eq!(app.selected_channel, Some(7));
    }

    #[test]
    fn empty_submit_sets_status_and_stays_on_connect() {
        let (mut app, _) = App::new();
        app.login.clear();
        app.password.clear();
        let _ = app.update(Message::SubmitLogin);
        assert_eq!(app.screen, Screen::Connect);
        assert!(!app.status.is_empty());
    }

    #[test]
    fn login_failure_returns_to_connect_screen() {
        let (mut app, _) = App::new();
        app.screen = Screen::Loading;
        let _ = app.update(Message::LoggedIn(Err("nope".into())));
        assert_eq!(app.screen, Screen::Connect);
        assert!(app.status.contains("Échec"));
    }

    #[test]
    fn loaded_data_populates_state() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::GuildsLoaded(Ok(Vec::new())));
        assert_eq!(app.screen, Screen::Main);
        let _ = app.update(Message::MessagesLoaded(Ok(Vec::new())));
        assert!(app.messages.is_empty());
    }
}
