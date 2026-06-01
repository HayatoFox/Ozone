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
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Alignment, Color, Element, Length, Subscription, Task, Theme};

use crate::style;
use ozone_core::proto::dto::{
    Channel, Guild, InstanceInfo, Member, Message as ChatMessage, PresenceView, TokenPair,
};
use ozone_core::proto::gateway::GatewayFrame;
use ozone_core::proto::Snowflake;
use ozone_core::{ApiClient, InstanceRef};
use std::collections::HashMap;

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
    MembersLoaded(usize, Result<Vec<Member>, String>),
    PresencesLoaded(usize, Result<Vec<PresenceView>, String>),
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
    /// Libellé de l'utilisateur connecté (pour le panneau utilisateur).
    user_label: String,
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
    members: Vec<Member>,
    /// Statut effectif par utilisateur (`online`/`idle`/`dnd`/`offline`), alimenté par la Gateway.
    presences: HashMap<i64, String>,
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
                members: Vec::new(),
                presences: HashMap::new(),
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
            user_label: String::new(),
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
                let label = self.form_login.clone();
                if let Some(inst) = self.instances.get_mut(idx) {
                    inst.api.set_token(Some(tokens.access_token.clone()));
                    inst.token = Some(tokens.access_token); // pour l'abonnement Gateway
                    inst.authed = true;
                    inst.user_label = label;
                }
                self.current = Some(idx);
                self.form_password.clear(); // ne pas conserver le mot de passe en mémoire
                self.form_gate_password.clear();
                self.form_email.clear();
                self.status.clear();
                self.guilds.clear();
                self.channels.clear();
                self.members.clear();
                self.presences.clear();
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
                    self.members.clear();
                    self.presences.clear();
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
                self.members.clear();
                self.messages.clear();
                self.selected_channel = None;
                let gsnow = Snowflake::from_i64(gid);
                let api = self.instances[idx].api.clone();
                let channels = {
                    let api = api.clone();
                    Task::perform(
                        async move { api.list_channels(gsnow).await.map_err(|e| e.to_string()) },
                        move |r| Message::ChannelsLoaded(idx, r),
                    )
                };
                let members = {
                    let api = api.clone();
                    Task::perform(
                        async move { api.list_members(gsnow).await.map_err(|e| e.to_string()) },
                        move |r| Message::MembersLoaded(idx, r),
                    )
                };
                let presences = Task::perform(
                    async move { api.list_presences(gsnow).await.map_err(|e| e.to_string()) },
                    move |r| Message::PresencesLoaded(idx, r),
                );
                Task::batch([channels, members, presences])
            }
            Message::MembersLoaded(idx, Ok(members)) => {
                if self.current == Some(idx) {
                    self.members = members;
                }
                Task::none()
            }
            Message::MembersLoaded(_, Err(_)) => Task::none(),
            Message::PresencesLoaded(idx, Ok(list)) => {
                if self.current == Some(idx) {
                    for p in list {
                        self.presences.insert(p.user_id.as_i64(), p.status);
                    }
                }
                Task::none()
            }
            Message::PresencesLoaded(_, Err(_)) => Task::none(),
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
            "PRESENCE_UPDATE" => {
                if let (Some(uid), Some(status)) = (
                    frame_id(d, "user_id"),
                    d.get("status").and_then(|v| v.as_str()),
                ) {
                    self.presences.insert(uid, status.to_string());
                }
            }
            _ => {} // salons/guildes temps réel : à étoffer
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
        row![self.nav_rail(), content].height(Length::Fill).into()
    }

    /// Rail vertical façon Discord : instances en haut, serveurs (guildes) de l'instance courante,
    /// puis bascule de thème en bas. Colonne sombre étroite.
    fn nav_rail(&self) -> Element<'_, Message> {
        let mut instances = column![].spacing(8).align_x(Alignment::Center);
        for (i, inst) in self.instances.iter().enumerate() {
            instances = instances.push(rail_icon(
                &inst.name,
                self.current == Some(i),
                Message::SelectInstance(i),
            ));
        }
        instances = instances.push(
            button(
                container(text("+").size(24).color(style::color::green()))
                    .center_x(Length::Fixed(48.0))
                    .center_y(Length::Fixed(48.0)),
            )
            .padding(0)
            .style(style::guild_icon(false))
            .on_press(Message::ShowAddInstance),
        );

        let mut guilds = column![].spacing(8).align_x(Alignment::Center);
        for g in &self.guilds {
            let selected = self.selected_guild == Some(g.id.as_i64());
            guilds = guilds.push(rail_icon(
                &g.name,
                selected,
                Message::SelectGuild(g.id.as_i64()),
            ));
        }

        let theme_btn = button(text("🎨").size(18))
            .padding(8)
            .style(style::subtle)
            .on_press(Message::CycleTheme);

        let rail = column![
            instances,
            rail_divider(),
            scrollable(guilds).height(Length::Fill),
            theme_btn,
        ]
        .spacing(8)
        .padding(8)
        .align_x(Alignment::Center)
        .width(Length::Fill);

        container(rail)
            .width(Length::Fixed(72.0))
            .height(Length::Fill)
            .style(style::rail_bg)
            .into()
    }

    fn add_instance_view(&self) -> Element<'_, Message> {
        let form = column![
            text("Ozone").size(28).color(style::color::header()),
            text("Connecte-toi à une instance")
                .size(14)
                .color(style::color::muted()),
            text_input("https://mon-instance", &self.form_address)
                .on_input(Message::AddressChanged)
                .on_submit(Message::CheckInstance)
                .padding(12)
                .style(style::input_field),
            button(text("Continuer"))
                .on_press(Message::CheckInstance)
                .padding(12)
                .width(Length::Fill)
                .style(style::primary),
            text(self.status.clone())
                .size(13)
                .color(style::color::muted()),
        ]
        .spacing(14)
        .max_width(380);

        let card = container(form).padding(28).style(style::card);
        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(24)
            .style(style::chat_bg)
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
            text(title).size(22).color(style::color::header()),
            text_input(login_placeholder, &self.form_login)
                .on_input(Message::LoginChanged)
                .padding(12)
                .style(style::input_field),
        ]
        .spacing(14)
        .max_width(380);

        // L'e-mail n'est demandé qu'à l'inscription.
        if register {
            form = form.push(
                text_input("e-mail", &self.form_email)
                    .on_input(Message::EmailChanged)
                    .padding(12)
                    .style(style::input_field),
            );
        }

        form = form.push(
            text_input("mot de passe", &self.form_password)
                .on_input(Message::PasswordChanged)
                .on_submit(Message::SubmitAuth)
                .secure(true)
                .padding(12)
                .style(style::input_field),
        );

        if gated {
            form = form.push(
                text_input("mot de passe d'instance", &self.form_gate_password)
                    .on_input(Message::GatePasswordChanged)
                    .secure(true)
                    .padding(12)
                    .style(style::input_field),
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
                .padding(12)
                .width(Length::Fill)
                .style(style::primary),
        );

        // Bascule connexion ⇄ inscription.
        let toggle_label = if register {
            "Déjà un compte ? Se connecter"
        } else {
            "Pas de compte ? En créer un"
        };
        form = form.push(
            button(text(toggle_label).size(13))
                .on_press(Message::ToggleAuthMode)
                .style(style::link),
        );

        if !self.status.is_empty() {
            form = form.push(
                text(self.status.clone())
                    .size(13)
                    .color(style::color::muted()),
            );
        }

        let card = container(form).padding(28).style(style::card);
        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(24)
            .style(style::chat_bg)
            .into()
    }

    fn main_view(&self) -> Element<'_, Message> {
        row![self.channel_sidebar(), self.chat_view(), self.member_list(),]
            .height(Length::Fill)
            .into()
    }

    /// Panneau des membres (à droite) : avatar + pastille de présence + nom.
    fn member_list(&self) -> Element<'_, Message> {
        let mut list = column![text(format!("MEMBRES — {}", self.members.len()))
            .size(11)
            .color(style::color::muted())]
        .spacing(6)
        .padding(8);
        for m in &self.members {
            let name = m
                .nick
                .clone()
                .or_else(|| m.user.display_name.clone())
                .unwrap_or_else(|| m.user.username.clone());
            let status = self
                .presences
                .get(&m.user.id.as_i64())
                .map(|s| s.as_str())
                .unwrap_or("offline");
            let name_color = if status == "offline" {
                style::color::muted()
            } else {
                style::color::text()
            };
            list = list.push(
                row![
                    avatar(&name, 28.0),
                    status_dot(status),
                    text(name.clone()).size(14).color(name_color),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        container(scrollable(list).height(Length::Fill))
            .width(Length::Fixed(220.0))
            .height(Length::Fill)
            .style(style::member_bg)
            .into()
    }

    /// Sidebar : en-tête (nom de la guilde), liste des salons, panneau utilisateur en bas.
    fn channel_sidebar(&self) -> Element<'_, Message> {
        let title = self
            .guilds
            .iter()
            .find(|g| Some(g.id.as_i64()) == self.selected_guild)
            .map(|g| g.name.clone())
            .or_else(|| self.current.map(|i| self.instances[i].name.clone()))
            .unwrap_or_default();
        let header = container(text(title).size(16).color(style::color::header()))
            .width(Length::Fill)
            .padding(16)
            .style(style::header_bar);

        let mut chans = column![text("SALONS").size(11).color(style::color::muted())]
            .spacing(2)
            .padding(8);
        for c in self.channels.iter().filter(|c| c.kind == 0 || c.kind == 2) {
            let selected = self.selected_channel == Some(c.id.as_i64());
            let prefix = if c.kind == 2 { "🔊" } else { "#" };
            chans = chans.push(
                button(text(format!("{prefix}  {}", c.name)).size(15))
                    .width(Length::Fill)
                    .padding(8)
                    .style(style::channel_item(selected))
                    .on_press(Message::SelectChannel(c.id.as_i64())),
            );
        }

        let col = column![
            header,
            scrollable(chans).height(Length::Fill),
            self.user_panel(),
        ]
        .height(Length::Fill);

        container(col)
            .width(Length::Fixed(240.0))
            .height(Length::Fill)
            .style(style::sidebar_bg)
            .into()
    }

    /// Panneau utilisateur (bas de la sidebar) : avatar + pseudo + statut.
    fn user_panel(&self) -> Element<'_, Message> {
        let label = self
            .current
            .map(|i| self.instances[i].user_label.clone())
            .unwrap_or_default();
        let info = column![
            text(label.clone()).size(14).color(style::color::text()),
            text("En ligne").size(11).color(style::color::green()),
        ]
        .spacing(1);
        let panel = row![avatar(&label, 32.0), info]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding(8);
        container(panel)
            .width(Length::Fill)
            .style(style::user_panel_bg)
            .into()
    }

    /// Zone de chat : en-tête du salon, fil de messages (avatars), composeur.
    fn chat_view(&self) -> Element<'_, Message> {
        let chan = self
            .channels
            .iter()
            .find(|c| Some(c.id.as_i64()) == self.selected_channel)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let header = container(
            row![
                text("#").size(20).color(style::color::muted()),
                text(chan.clone()).size(16).color(style::color::header()),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(16)
        .style(style::header_bar);

        // Fil : groupement des messages consécutifs d'un même auteur (façon Discord).
        let body: Element<Message> = if self.messages.is_empty() {
            container(
                text(format!("C'est le tout début de #{chan}."))
                    .size(14)
                    .color(style::color::muted()),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else {
            let mut feed = column![].spacing(4).padding(16);
            let mut prev: Option<i64> = None;
            for m in &self.messages {
                let aid = m.author.id.as_i64();
                if prev == Some(aid) {
                    // Message groupé : juste le contenu, indenté sous l'avatar.
                    feed = feed.push(row![
                        Space::with_width(Length::Fixed(52.0)),
                        text(m.content.clone()).size(15).color(style::color::text()),
                    ]);
                } else {
                    if prev.is_some() {
                        feed = feed.push(Space::with_height(Length::Fixed(8.0)));
                    }
                    let author = m
                        .author
                        .display_name
                        .clone()
                        .unwrap_or_else(|| m.author.username.clone());
                    let head = row![
                        text(author.clone()).size(15).color(style::color::header()),
                        text(fmt_time(m.created_at))
                            .size(11)
                            .color(style::color::muted()),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);
                    let col = column![
                        head,
                        text(m.content.clone()).size(15).color(style::color::text())
                    ]
                    .spacing(2);
                    feed = feed.push(row![avatar(&author, 40.0), col].spacing(12));
                }
                prev = Some(aid);
            }
            scrollable(feed)
                .height(Length::Fill)
                .width(Length::Fill)
                .into()
        };

        let placeholder = if chan.is_empty() {
            "Envoyer un message".to_string()
        } else {
            format!("Envoyer un message dans #{chan}")
        };
        let composer = container(
            text_input(&placeholder, &self.composer)
                .on_input(Message::ComposerChanged)
                .on_submit(Message::SendMessage)
                .padding(12)
                .style(style::composer_field),
        )
        .padding(16);

        let col = column![header, body, composer]
            .width(Length::Fill)
            .height(Length::Fill);
        container(col)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(style::chat_bg)
            .into()
    }
}

/// Heure (UTC `HH:MM`) à partir d'un timestamp en millisecondes.
fn fmt_time(ms: u64) -> String {
    let secs = ms / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    format!("{h:02}:{m:02}")
}

/// Première lettre (majuscule) d'un nom, pour les avatars/icônes.
fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Couleur d'avatar dérivée du nom (palette façon Discord).
fn avatar_color(seed: &str) -> Color {
    let palette = [
        Color::from_rgb8(0x58, 0x65, 0xf2),
        Color::from_rgb8(0x23, 0xa5, 0x5a),
        Color::from_rgb8(0xda, 0x37, 0x3c),
        Color::from_rgb8(0xfa, 0xa6, 0x1a),
        Color::from_rgb8(0x9b, 0x59, 0xb6),
        Color::from_rgb8(0x1a, 0xbc, 0x9c),
    ];
    let h = seed
        .bytes()
        .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    palette[(h as usize) % palette.len()]
}

/// Pastille avatar : initiale blanche sur disque coloré (dérivé du nom).
fn avatar<'a>(name: &str, size: f32) -> Element<'a, Message> {
    container(
        text(initial(name))
            .size(size * 0.42)
            .color(style::color::white()),
    )
    .center_x(Length::Fixed(size))
    .center_y(Length::Fixed(size))
    .style(style::dot(avatar_color(name), size / 2.0))
    .into()
}

/// Icône ronde du rail (initiale) ; survol/sélection mis en évidence.
fn rail_icon<'a>(label: &str, selected: bool, msg: Message) -> Element<'a, Message> {
    button(
        container(text(initial(label)).size(18).color(style::color::white()))
            .center_x(Length::Fixed(48.0))
            .center_y(Length::Fixed(48.0)),
    )
    .padding(0)
    .style(style::guild_icon(selected))
    .on_press(msg)
    .into()
}

/// Couleur d'une pastille de présence.
fn presence_color(status: &str) -> Color {
    match status {
        "online" => style::color::green(),
        "idle" => style::color::idle(),
        "dnd" => style::color::dnd(),
        _ => style::color::muted(),
    }
}

/// Petite pastille de présence (10 px).
fn status_dot<'a>(status: &str) -> Element<'a, Message> {
    container(text(" "))
        .width(Length::Fixed(10.0))
        .height(Length::Fixed(10.0))
        .style(style::dot(presence_color(status), 5.0))
        .into()
}

/// Fin séparateur horizontal du rail.
fn rail_divider<'a>() -> Element<'a, Message> {
    container(text(" "))
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(2.0))
        .style(style::dot(style::color::input(), 1.0))
        .into()
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
    fn fmt_time_is_utc_hh_mm() {
        assert_eq!(fmt_time(0), "00:00");
        assert_eq!(fmt_time((3600 + 120) * 1000), "01:02");
        assert_eq!(fmt_time(23 * 3600 * 1000 + 59 * 60 * 1000), "23:59");
    }

    #[test]
    fn initial_uppercases_first_char() {
        assert_eq!(initial("alice"), "A");
        assert_eq!(initial("  bob"), "B");
        assert_eq!(initial(""), "?");
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
