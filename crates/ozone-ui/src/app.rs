//! Application Iced (Elm) : état, messages, `update`, `view`, thème.
//!
//! **Multi-instances** : le client gère plusieurs instances auto-hébergées (rail de gauche), chacune
//! avec sa propre session (jeton conservé en mémoire). L'ajout d'une instance passe par une **porte
//! d'accès** optionnelle (mot de passe d'instance) avant l'authentification.
//!
//! Le réseau passe par `ozone_core::ApiClient` (REST typé, `Clone`) dans des `Task` asynchrones
//! dont le **résultat revient en `Message`** (étiqueté par l'index d'instance pour ignorer les
//! résultats périmés après un changement d'instance). L'état affichable vit dans `App`. Le rendu
//! n'exécute jamais le contenu : tout est affiché en **texte brut** (aucune exécution côté UI).

use iced::futures::{SinkExt, Stream};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Subscription, Task, Theme};
use ozone_core::proto::dto::{Channel, Guild, InstanceInfo, Message as ChatMessage, TokenPair};
use ozone_core::proto::gateway::GatewayFrame;
use ozone_core::proto::Snowflake;
use ozone_core::{ApiClient, InstanceRef};

use crate::theme::ThemeChoice;

/// Extrait un identifiant (string → i64) d'un champ d'une trame Gateway.
fn frame_id(d: &serde_json::Value, key: &str) -> Option<i64> {
    d.get(key)?.as_str()?.parse::<i64>().ok()
}

/// Événements de l'UI (entrées utilisateur + résultats des `Task`, étiquetés par index d'instance).
///
/// `Message` est le nom conventionnel d'Iced ; plusieurs variantes évoquent « message » (domaine),
/// d'où l'`allow`. Les gros payloads sont boxés (variantes de taille homogène).
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum Message {
    // Formulaire d'ajout / authentification.
    AddressChanged(String),
    LoginChanged(String),
    PasswordChanged(String),
    GatePasswordChanged(String),
    CheckInstance,
    InstanceChecked(Result<Box<InstanceInfo>, String>),
    SubmitAuth,
    Authenticated(usize, Result<Box<TokenPair>, String>),
    // Navigation multi-instances.
    ShowAddInstance,
    SelectInstance(usize),
    // Données de l'instance courante.
    GuildsLoaded(usize, Result<Vec<Guild>, String>),
    SelectGuild(i64),
    ChannelsLoaded(usize, Result<Vec<Channel>, String>),
    SelectChannel(i64),
    MessagesLoaded(usize, Result<Vec<ChatMessage>, String>),
    ComposerChanged(String),
    SendMessage,
    MessageSent(usize, Result<Box<ChatMessage>, String>),
    /// Événement temps réel reçu de la Gateway de l'instance courante.
    GatewayEvent(Box<GatewayFrame>),
    /// Bascule vers le thème suivant.
    CycleTheme,
    /// Champ e-mail (utilisé en mode inscription).
    EmailChanged(String),
    /// Bascule entre connexion et inscription.
    ToggleAuthMode,
}

/// Mode de l'écran d'authentification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AuthMode {
    #[default]
    Login,
    Register,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// Ajout d'une instance (saisie de l'adresse).
    AddInstance,
    /// Authentification de l'instance courante (avec porte d'accès si requise).
    Auth,
    /// Vue principale (guildes/salons/messages) de l'instance courante.
    Main,
}

/// Une instance connue côté client (sa propre session).
struct Instance {
    address: String,
    name: String,
    gated: bool,
    api: ApiClient,
    authed: bool,
    /// Jeton d'accès (conservé pour alimenter l'abonnement Gateway temps réel).
    token: Option<String>,
}

/// État de l'application.
pub struct App {
    instances: Vec<Instance>,
    current: Option<usize>,
    screen: Screen,
    // Formulaire d'ajout / d'authentification (instance ciblée = `current`).
    form_address: String,
    form_login: String,
    form_email: String,
    form_password: String,
    form_gate_password: String,
    auth_mode: AuthMode,
    status: String,
    // Données de l'instance courante.
    guilds: Vec<Guild>,
    channels: Vec<Channel>,
    messages: Vec<ChatMessage>,
    selected_guild: Option<i64>,
    selected_channel: Option<i64>,
    composer: String,
    theme_choice: ThemeChoice,
}

/// Le formulaire d'authentification est-il complet (gate inclus si requis) ?
fn can_authenticate(login: &str, password: &str, gated: bool, gate_password: &str) -> bool {
    !login.trim().is_empty() && !password.is_empty() && (!gated || !gate_password.is_empty())
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                instances: Vec::new(),
                current: None,
                screen: Screen::AddInstance,
                form_address: "http://127.0.0.1:8080".to_string(),
                form_login: String::new(),
                form_email: String::new(),
                form_password: String::new(),
                form_gate_password: String::new(),
                auth_mode: AuthMode::Login,
                status: String::new(),
                guilds: Vec::new(),
                channels: Vec::new(),
                messages: Vec::new(),
                selected_guild: None,
                selected_channel: None,
                composer: String::new(),
                theme_choice: ThemeChoice::default(),
            },
            Task::none(),
        )
    }

    pub fn theme(&self) -> Theme {
        self.theme_choice.to_theme()
    }

    /// Insère ou met à jour une instance par adresse normalisée ; renvoie son index.
    fn upsert_instance(&mut self, address: &str, name: String, gated: bool) -> usize {
        let base = InstanceRef::new(address).api_base();
        if let Some(i) = self.instances.iter().position(|x| x.address == base) {
            self.instances[i].name = name;
            self.instances[i].gated = gated;
            return i;
        }
        self.instances.push(Instance {
            address: base.clone(),
            name,
            gated,
            api: ApiClient::new(base),
            authed: false,
            token: None,
        });
        self.instances.len() - 1
    }

    /// Charge les guildes de l'instance `idx` (étiquetage pour ignorer les résultats périmés).
    fn load_guilds(&self, idx: usize) -> Task<Message> {
        let api = self.instances[idx].api.clone();
        Task::perform(
            async move { api.list_guilds().await.map_err(|e| e.to_string()) },
            move |r| Message::GuildsLoaded(idx, r),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddressChanged(v) => {
                self.form_address = v;
                Task::none()
            }
            Message::LoginChanged(v) => {
                self.form_login = v;
                Task::none()
            }
            Message::PasswordChanged(v) => {
                self.form_password = v;
                Task::none()
            }
            Message::GatePasswordChanged(v) => {
                self.form_gate_password = v;
                Task::none()
            }
            Message::EmailChanged(v) => {
                self.form_email = v;
                Task::none()
            }
            Message::ShowAddInstance => {
                self.form_login.clear();
                self.form_email.clear();
                self.form_password.clear();
                self.form_gate_password.clear();
                self.auth_mode = AuthMode::Login;
                self.status.clear();
                self.screen = Screen::AddInstance;
                Task::none()
            }
            Message::CheckInstance => {
                if self.form_address.trim().is_empty() {
                    self.status = "Renseigne l'adresse de l'instance.".into();
                    return Task::none();
                }
                self.status = "Vérification de l'instance…".into();
                let api = ApiClient::new(InstanceRef::new(self.form_address.trim()).api_base());
                Task::perform(
                    async move {
                        api.instance_info()
                            .await
                            .map(Box::new)
                            .map_err(|e| e.to_string())
                    },
                    Message::InstanceChecked,
                )
            }
            Message::InstanceChecked(Ok(info)) => {
                let address = self.form_address.trim().to_string();
                let idx =
                    self.upsert_instance(&address, info.name.clone(), info.access_gate.required);
                self.current = Some(idx);
                self.status.clear();
                self.screen = Screen::Auth;
                Task::none()
            }
            Message::InstanceChecked(Err(e)) => {
                self.status = format!("Instance injoignable : {e}");
                Task::none()
            }
            Message::ToggleAuthMode => {
                self.auth_mode = match self.auth_mode {
                    AuthMode::Login => AuthMode::Register,
                    AuthMode::Register => AuthMode::Login,
                };
                self.status.clear();
                Task::none()
            }
            Message::SubmitAuth => {
                let Some(idx) = self.current else {
                    return Task::none();
                };
                let gated = self.instances[idx].gated;
                let register = self.auth_mode == AuthMode::Register;
                let base_ok = can_authenticate(
                    &self.form_login,
                    &self.form_password,
                    gated,
                    &self.form_gate_password,
                );
                // En inscription, l'e-mail est requis en plus.
                if !base_ok || (register && self.form_email.trim().is_empty()) {
                    self.status = "Complète les champs requis.".into();
                    return Task::none();
                }
                self.status = if register {
                    "Création du compte…".into()
                } else {
                    "Connexion…".into()
                };
                let mut api = self.instances[idx].api.clone();
                let login = self.form_login.clone();
                let email = self.form_email.clone();
                let password = self.form_password.clone();
                let gate_password = self.form_gate_password.clone();
                Task::perform(
                    async move {
                        if gated {
                            let gt = api.gate(&gate_password).await.map_err(|e| e.to_string())?;
                            api.set_gate_token(Some(gt));
                        }
                        if register {
                            api.register(&login, &email, &password)
                                .await
                                .map(Box::new)
                                .map_err(|e| e.to_string())
                        } else {
                            api.login(&login, &password)
                                .await
                                .map(Box::new)
                                .map_err(|e| e.to_string())
                        }
                    },
                    move |r| Message::Authenticated(idx, r),
                )
            }
            Message::Authenticated(idx, Ok(tokens)) => {
                if let Some(inst) = self.instances.get_mut(idx) {
                    inst.api.set_token(Some(tokens.access_token.clone()));
                    inst.token = Some(tokens.access_token); // pour l'abonnement Gateway
                    inst.authed = true;
                }
                self.current = Some(idx);
                self.form_password.clear(); // ne pas conserver le mot de passe en mémoire
                self.form_gate_password.clear();
                self.form_email.clear();
                self.status.clear();
                self.guilds.clear();
                self.channels.clear();
                self.messages.clear();
                self.selected_guild = None;
                self.selected_channel = None;
                self.screen = Screen::Main;
                self.load_guilds(idx)
            }
            Message::Authenticated(_, Err(e)) => {
                self.screen = Screen::Auth;
                self.status = format!("Échec de connexion : {e}");
                Task::none()
            }
            Message::SelectInstance(idx) => {
                if idx >= self.instances.len() {
                    return Task::none();
                }
                self.current = Some(idx);
                self.status.clear();
                if self.instances[idx].authed {
                    self.guilds.clear();
                    self.channels.clear();
                    self.messages.clear();
                    self.selected_guild = None;
                    self.selected_channel = None;
                    self.screen = Screen::Main;
                    self.load_guilds(idx)
                } else {
                    self.form_address = self.instances[idx].address.clone();
                    self.screen = Screen::Auth;
                    Task::none()
                }
            }
            Message::GuildsLoaded(idx, Ok(guilds)) => {
                if self.current != Some(idx) {
                    return Task::none(); // résultat périmé (instance changée)
                }
                self.guilds = guilds;
                if let Some(first) = self.guilds.first() {
                    let gid = first.id.as_i64();
                    return self.update(Message::SelectGuild(gid));
                }
                Task::none()
            }
            Message::GuildsLoaded(_, Err(e)) => {
                self.status = format!("Guildes : {e}");
                Task::none()
            }
            Message::SelectGuild(gid) => {
                let Some(idx) = self.current else {
                    return Task::none();
                };
                self.selected_guild = Some(gid);
                self.channels.clear();
                self.messages.clear();
                self.selected_channel = None;
                let api = self.instances[idx].api.clone();
                Task::perform(
                    async move {
                        api.list_channels(Snowflake::from_i64(gid))
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |r| Message::ChannelsLoaded(idx, r),
                )
            }
            Message::ChannelsLoaded(idx, Ok(channels)) => {
                if self.current != Some(idx) {
                    return Task::none();
                }
                self.channels = channels;
                if let Some(first) = self.channels.iter().find(|c| c.kind == 0) {
                    let cid = first.id.as_i64();
                    return self.update(Message::SelectChannel(cid));
                }
                Task::none()
            }
            Message::ChannelsLoaded(_, Err(e)) => {
                self.status = format!("Salons : {e}");
                Task::none()
            }
            Message::SelectChannel(cid) => {
                let Some(idx) = self.current else {
                    return Task::none();
                };
                self.selected_channel = Some(cid);
                self.messages.clear();
                let api = self.instances[idx].api.clone();
                Task::perform(
                    async move {
                        api.list_messages(Snowflake::from_i64(cid))
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |r| Message::MessagesLoaded(idx, r),
                )
            }
            Message::MessagesLoaded(idx, Ok(messages)) => {
                if self.current != Some(idx) {
                    return Task::none();
                }
                self.messages = messages;
                Task::none()
            }
            Message::MessagesLoaded(_, Err(e)) => {
                self.status = format!("Messages : {e}");
                Task::none()
            }
            Message::ComposerChanged(v) => {
                self.composer = v;
                Task::none()
            }
            Message::SendMessage => {
                let Some(idx) = self.current else {
                    return Task::none();
                };
                let content = self.composer.trim().to_string();
                let Some(cid) = self.selected_channel else {
                    return Task::none();
                };
                if content.is_empty() {
                    return Task::none();
                }
                self.composer.clear();
                let api = self.instances[idx].api.clone();
                Task::perform(
                    async move {
                        api.send_message(Snowflake::from_i64(cid), &content)
                            .await
                            .map(Box::new)
                            .map_err(|e| e.to_string())
                    },
                    move |r| Message::MessageSent(idx, r),
                )
            }
            Message::MessageSent(idx, Ok(m)) => {
                if self.current == Some(idx) {
                    self.messages.push(*m); // optimiste ; l'écho Gateway remplacera ce chemin
                }
                Task::none()
            }
            Message::MessageSent(_, Err(e)) => {
                self.status = format!("Envoi : {e}");
                Task::none()
            }
            Message::GatewayEvent(frame) => {
                self.apply_gateway(&frame);
                Task::none()
            }
            Message::CycleTheme => {
                self.theme_choice = self.theme_choice.next();
                Task::none()
            }
        }
    }

    /// Applique un événement Gateway au fil affiché (salon courant uniquement).
    /// Déduplique par identifiant (l'écho d'un message envoyé optimiste ne crée pas de doublon).
    fn apply_gateway(&mut self, frame: &GatewayFrame) {
        let Some(t) = frame.t.as_deref() else {
            return;
        };
        let Some(d) = frame.d.as_ref() else {
            return;
        };
        match t {
            "MESSAGE_CREATE" | "MESSAGE_UPDATE" => {
                if let Ok(m) = serde_json::from_value::<ChatMessage>(d.clone()) {
                    if self.selected_channel == Some(m.channel_id.as_i64()) {
                        if let Some(slot) = self.messages.iter_mut().find(|x| x.id == m.id) {
                            *slot = m;
                        } else {
                            self.messages.push(m);
                        }
                    }
                }
            }
            "MESSAGE_DELETE" => {
                if let (Some(cid), Some(mid)) = (frame_id(d, "channel_id"), frame_id(d, "id")) {
                    if self.selected_channel == Some(cid) {
                        self.messages.retain(|m| m.id.as_i64() != mid);
                    }
                }
            }
            _ => {} // présence, salons, guildes : ignorés pour ce fil (à étoffer)
        }
    }

    /// Abonnement temps réel : ouvre la Gateway de l'instance **courante authentifiée** et pousse
    /// ses événements dans l'UI. Re-clé sur (adresse, jeton) ⇒ se relance à chaque (ré)auth ou
    /// changement d'instance ; se ferme quand aucune instance authentifiée n'est sélectionnée.
    pub fn subscription(&self) -> Subscription<Message> {
        match self.current {
            Some(idx) if self.instances[idx].authed => {
                let base = self.instances[idx].address.clone();
                let token = self.instances[idx].token.clone().unwrap_or_default();
                Subscription::run_with_id(format!("gw:{base}:{token}"), gateway_stream(base, token))
            }
            _ => Subscription::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = match self.screen {
            Screen::AddInstance => self.add_instance_view(),
            Screen::Auth => self.auth_view(),
            Screen::Main => self.main_view(),
        };
        // Rail des instances toujours visible à gauche.
        row![self.instance_rail(), content]
            .height(Length::Fill)
            .into()
    }

    fn instance_rail(&self) -> Element<'_, Message> {
        let mut rail = column![text("Instances").size(12)].spacing(6).padding(8);
        for (i, inst) in self.instances.iter().enumerate() {
            let mark = if self.current == Some(i) { "▸ " } else { "" };
            let dot = if inst.authed { "●" } else { "○" };
            rail = rail.push(
                button(text(format!("{mark}{dot} {}", inst.name)))
                    .width(Length::Fill)
                    .on_press(Message::SelectInstance(i)),
            );
        }
        rail = rail.push(
            button(text("+ Ajouter"))
                .width(Length::Fill)
                .on_press(Message::ShowAddInstance),
        );
        rail = rail.push(
            button(text(format!("🎨 {}", self.theme_choice.label())))
                .width(Length::Fill)
                .on_press(Message::CycleTheme),
        );
        container(rail)
            .width(Length::Fixed(170.0))
            .height(Length::Fill)
            .into()
    }

    fn add_instance_view(&self) -> Element<'_, Message> {
        let form = column![
            text("Ozone").size(32),
            text("Ajouter une instance").size(16),
            text_input("https://mon-instance", &self.form_address)
                .on_input(Message::AddressChanged)
                .on_submit(Message::CheckInstance)
                .padding(8),
            button(text("Continuer"))
                .on_press(Message::CheckInstance)
                .padding(10),
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

    fn auth_view(&self) -> Element<'_, Message> {
        let gated = self
            .current
            .map(|i| self.instances[i].gated)
            .unwrap_or(false);
        let name = self
            .current
            .map(|i| self.instances[i].name.clone())
            .unwrap_or_default();

        let register = self.auth_mode == AuthMode::Register;
        let title = if register {
            format!("Créer un compte sur {name}")
        } else {
            format!("Connexion à {name}")
        };
        let login_placeholder = if register {
            "pseudo"
        } else {
            "identifiant ou e-mail"
        };

        let mut form = column![
            text(title).size(24),
            text_input(login_placeholder, &self.form_login)
                .on_input(Message::LoginChanged)
                .padding(8),
        ]
        .spacing(12)
        .max_width(360);

        // L'e-mail n'est demandé qu'à l'inscription.
        if register {
            form = form.push(
                text_input("e-mail", &self.form_email)
                    .on_input(Message::EmailChanged)
                    .padding(8),
            );
        }

        form = form.push(
            text_input("mot de passe", &self.form_password)
                .on_input(Message::PasswordChanged)
                .on_submit(Message::SubmitAuth)
                .secure(true)
                .padding(8),
        );

        if gated {
            form = form.push(
                text_input("mot de passe d'instance", &self.form_gate_password)
                    .on_input(Message::GatePasswordChanged)
                    .secure(true)
                    .padding(8),
            );
        }

        let submit_label = if register {
            "Créer le compte"
        } else {
            "Se connecter"
        };
        form = form.push(
            button(text(submit_label))
                .on_press(Message::SubmitAuth)
                .padding(10),
        );

        // Bascule connexion ⇄ inscription.
        let toggle_label = if register {
            "Déjà un compte ? Se connecter"
        } else {
            "Pas de compte ? En créer un"
        };
        form = form.push(button(text(toggle_label)).on_press(Message::ToggleAuthMode));

        form = form.push(text(self.status.clone()));

        container(form)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(20)
            .into()
    }

    fn main_view(&self) -> Element<'_, Message> {
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

        let mut channels = column![text("Salons").size(14)].spacing(6).padding(8);
        for c in self.channels.iter().filter(|c| c.kind == 0 || c.kind == 2) {
            let prefix = if c.kind == 2 { "🔊" } else { "#" };
            channels = channels.push(
                button(text(format!("{prefix} {}", c.name)))
                    .width(Length::Fill)
                    .on_press(Message::SelectChannel(c.id.as_i64())),
            );
        }

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
                .width(Length::Fixed(180.0))
                .height(Length::Fill),
            container(channels)
                .width(Length::Fixed(200.0))
                .height(Length::Fill),
            container(main).width(Length::Fill).height(Length::Fill),
        ]
        .height(Length::Fill)
        .into()
    }
}

/// Flux d'événements Gateway pour l'abonnement Iced : (re)connecte avec **RESUME** quand possible
/// (rejeu des événements manqués), se termine si l'instance devient injoignable.
fn gateway_stream(base: String, token: String) -> impl Stream<Item = Message> {
    iced::stream::channel(64, move |mut output| async move {
        let mut resume: Option<(String, u64)> = None;
        loop {
            let conn = match &resume {
                Some((sid, seq)) => {
                    match ozone_core::gateway::connect_resume(&base, &token, sid, *seq).await {
                        Ok(ozone_core::gateway::Resumed::Ok(c)) => Some(c),
                        // Session expirée/refusée ou erreur ⇒ connexion complète.
                        _ => ozone_core::gateway_connect(&base, &token).await.ok(),
                    }
                }
                None => ozone_core::gateway_connect(&base, &token).await.ok(),
            };
            let Some(mut conn) = conn else {
                break; // instance injoignable : on arrête le flux
            };
            while let Some(frame) = conn.next_event().await {
                if output
                    .send(Message::GatewayEvent(Box::new(frame)))
                    .await
                    .is_err()
                {
                    return; // l'abonnement a été abandonné (changement d'instance / fermeture)
                }
            }
            // Déconnecté : on mémorise la session pour tenter un RESUME au prochain tour.
            resume = conn.session_id().map(|s| (s.to_string(), conn.last_seq()));
            if resume.is_none() {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_auth_form_with_optional_gate() {
        assert!(can_authenticate("a", "b", false, ""));
        assert!(!can_authenticate("", "b", false, ""));
        assert!(!can_authenticate("a", "", false, ""));
        // Instance gardée : le mot de passe d'instance devient requis.
        assert!(!can_authenticate("a", "b", true, ""));
        assert!(can_authenticate("a", "b", true, "gate"));
    }

    fn info(name: &str, gated: bool) -> Box<InstanceInfo> {
        Box::new(
            serde_json::from_value(serde_json::json!({
                "instance_id": "1", "name": name, "description": null,
                "accent_color": null, "version": "t", "registration_policy": "open",
                "access_gate": { "required": gated }
            }))
            .unwrap(),
        )
    }

    #[test]
    fn checking_instance_adds_it_and_goes_to_auth() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::AddressChanged("http://127.0.0.1:9".into()));
        let _ = app.update(Message::InstanceChecked(Ok(info("Atelier", true))));
        assert_eq!(app.instances.len(), 1);
        assert_eq!(app.current, Some(0));
        assert!(app.instances[0].gated);
        assert_eq!(app.screen, Screen::Auth);
        // Re-vérifier la même adresse ne crée pas de doublon.
        let _ = app.update(Message::InstanceChecked(Ok(info("Atelier", true))));
        assert_eq!(app.instances.len(), 1);
    }

    #[test]
    fn authenticated_marks_instance_and_opens_main() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::AddressChanged("http://127.0.0.1:9".into()));
        let _ = app.update(Message::InstanceChecked(Ok(info("X", false))));
        let tokens = Box::new(
            serde_json::from_value::<TokenPair>(serde_json::json!({
                "access_token": "acc", "refresh_token": "ref",
                "token_type": "Bearer", "expires_in": 600
            }))
            .unwrap(),
        );
        let _ = app.update(Message::Authenticated(0, Ok(tokens)));
        assert!(app.instances[0].authed);
        assert_eq!(app.screen, Screen::Main);
        assert!(app.form_password.is_empty()); // effacé après connexion
    }

    #[test]
    fn gateway_events_update_current_channel_feed() {
        let (mut app, _) = App::new();
        app.selected_channel = Some(42);
        let create = |id: u64, content: &str| {
            Message::GatewayEvent(Box::new(GatewayFrame::dispatch(
                "MESSAGE_CREATE",
                serde_json::json!({
                    "id": id.to_string(), "channel_id": "42",
                    "author": { "id": "1", "username": "u", "display_name": null, "avatar_id": null },
                    "content": content, "type": 0, "created_at": 1, "edited_at": null, "pinned": false
                }),
                id,
            )))
        };
        let _ = app.update(create(1, "salut"));
        assert_eq!(app.messages.len(), 1);
        // Même id ⇒ mise à jour en place (pas de doublon avec l'écho optimiste).
        let _ = app.update(create(1, "salut (édité)"));
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "salut (édité)");
        // Message d'un autre salon ⇒ ignoré.
        let _ = app.update(Message::GatewayEvent(Box::new(GatewayFrame::dispatch(
            "MESSAGE_CREATE",
            serde_json::json!({
                "id": "2", "channel_id": "99",
                "author": { "id": "1", "username": "u", "display_name": null, "avatar_id": null },
                "content": "ailleurs", "type": 0, "created_at": 1, "edited_at": null, "pinned": false
            }),
            2,
        ))));
        assert_eq!(app.messages.len(), 1);
        // Suppression.
        let _ = app.update(Message::GatewayEvent(Box::new(GatewayFrame::dispatch(
            "MESSAGE_DELETE",
            serde_json::json!({ "id": "1", "channel_id": "42" }),
            3,
        ))));
        assert!(app.messages.is_empty());
    }

    #[test]
    fn register_mode_requires_email() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::InstanceChecked(Ok(info("X", false))));
        assert_eq!(app.auth_mode, AuthMode::Login);
        let _ = app.update(Message::ToggleAuthMode);
        assert_eq!(app.auth_mode, AuthMode::Register);
        // Pseudo + mot de passe remplis, mais e-mail manquant ⇒ soumission refusée (pas de Task réseau).
        let _ = app.update(Message::LoginChanged("bob".into()));
        let _ = app.update(Message::PasswordChanged("motdepasse".into()));
        let _ = app.update(Message::SubmitAuth);
        assert!(!app.status.is_empty(), "champ requis manquant signalé");
        assert_eq!(app.screen, Screen::Auth);
        // Avec l'e-mail, le formulaire est complet.
        let _ = app.update(Message::EmailChanged("bob@ex.fr".into()));
        assert_eq!(app.form_email, "bob@ex.fr");
        // Retour en mode connexion.
        let _ = app.update(Message::ToggleAuthMode);
        assert_eq!(app.auth_mode, AuthMode::Login);
    }

    #[test]
    fn cycle_theme_changes_choice() {
        let (mut app, _) = App::new();
        let start = app.theme_choice;
        let _ = app.update(Message::CycleTheme);
        assert_ne!(app.theme_choice, start);
        // 4 bascules ⇒ retour au thème initial.
        for _ in 0..3 {
            let _ = app.update(Message::CycleTheme);
        }
        assert_eq!(app.theme_choice, start);
    }

    #[test]
    fn stale_guilds_result_is_ignored_after_switch() {
        let (mut app, _) = App::new();
        // Deux instances connues, courante = 1.
        let _ = app.update(Message::InstanceChecked(Ok(info("A", false))));
        let _ = app.update(Message::AddressChanged("http://127.0.0.1:10".into()));
        let _ = app.update(Message::InstanceChecked(Ok(info("B", false))));
        assert_eq!(app.current, Some(1));
        // Un résultat de guildes pour l'instance 0 (périmé) doit être ignoré.
        let _ = app.update(Message::GuildsLoaded(0, Ok(Vec::new())));
        assert_eq!(app.screen, Screen::Auth); // inchangé par le résultat périmé
    }
}
