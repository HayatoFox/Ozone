//! Permissions (bitfield 64 bits, valeurs alignées sur Discord). Cf. `docs/features/10-roles-permissions.md`.

#![allow(clippy::unreadable_literal)]

pub const CREATE_INSTANT_INVITE: u64 = 1 << 0;
pub const KICK_MEMBERS: u64 = 1 << 1;
pub const BAN_MEMBERS: u64 = 1 << 2;
pub const ADMINISTRATOR: u64 = 1 << 3;
pub const MANAGE_CHANNELS: u64 = 1 << 4;
pub const MANAGE_GUILD: u64 = 1 << 5;
pub const ADD_REACTIONS: u64 = 1 << 6;
pub const VIEW_AUDIT_LOG: u64 = 1 << 7;
pub const PRIORITY_SPEAKER: u64 = 1 << 8;
pub const STREAM: u64 = 1 << 9;
pub const VIEW_CHANNEL: u64 = 1 << 10;
pub const SEND_MESSAGES: u64 = 1 << 11;
pub const SEND_TTS_MESSAGES: u64 = 1 << 12;
pub const MANAGE_MESSAGES: u64 = 1 << 13;
pub const EMBED_LINKS: u64 = 1 << 14;
pub const ATTACH_FILES: u64 = 1 << 15;
pub const READ_MESSAGE_HISTORY: u64 = 1 << 16;
pub const MENTION_EVERYONE: u64 = 1 << 17;
pub const USE_EXTERNAL_EMOJIS: u64 = 1 << 18;
pub const VIEW_GUILD_INSIGHTS: u64 = 1 << 19;
pub const CONNECT: u64 = 1 << 20;
pub const SPEAK: u64 = 1 << 21;
pub const MUTE_MEMBERS: u64 = 1 << 22;
pub const DEAFEN_MEMBERS: u64 = 1 << 23;
pub const MOVE_MEMBERS: u64 = 1 << 24;
pub const USE_VAD: u64 = 1 << 25;
pub const CHANGE_NICKNAME: u64 = 1 << 26;
pub const MANAGE_NICKNAMES: u64 = 1 << 27;
pub const MANAGE_ROLES: u64 = 1 << 28;
pub const MANAGE_WEBHOOKS: u64 = 1 << 29;
pub const MANAGE_GUILD_EXPRESSIONS: u64 = 1 << 30;
pub const USE_APPLICATION_COMMANDS: u64 = 1 << 31;
pub const REQUEST_TO_SPEAK: u64 = 1 << 32;
pub const MANAGE_EVENTS: u64 = 1 << 33;
pub const MANAGE_THREADS: u64 = 1 << 34;
pub const CREATE_PUBLIC_THREADS: u64 = 1 << 35;
pub const CREATE_PRIVATE_THREADS: u64 = 1 << 36;
pub const USE_EXTERNAL_STICKERS: u64 = 1 << 37;
pub const SEND_MESSAGES_IN_THREADS: u64 = 1 << 38;
pub const USE_EMBEDDED_ACTIVITIES: u64 = 1 << 39;
pub const MODERATE_MEMBERS: u64 = 1 << 40;
pub const VIEW_CREATOR_MONETIZATION_ANALYTICS: u64 = 1 << 41;
pub const USE_SOUNDBOARD: u64 = 1 << 42;
pub const CREATE_GUILD_EXPRESSIONS: u64 = 1 << 43;
pub const CREATE_EVENTS: u64 = 1 << 44;
pub const USE_EXTERNAL_SOUNDS: u64 = 1 << 45;
pub const SEND_VOICE_MESSAGES: u64 = 1 << 46;
pub const SET_VOICE_CHANNEL_STATUS: u64 = 1 << 48;
pub const SEND_POLLS: u64 = 1 << 49;
pub const USE_EXTERNAL_APPS: u64 = 1 << 50;
pub const PIN_MESSAGES: u64 = 1 << 51;
pub const BYPASS_SLOWMODE: u64 = 1 << 52;

/// Toutes les permissions (utilisé pour propriétaire / ADMINISTRATOR).
pub const ALL: u64 = u64::MAX;

/// Permissions par défaut du rôle `@everyone` d'une nouvelle guilde.
pub const DEFAULT_EVERYONE: u64 = CREATE_INSTANT_INVITE
    | VIEW_CHANNEL
    | SEND_MESSAGES
    | EMBED_LINKS
    | ATTACH_FILES
    | ADD_REACTIONS
    | USE_EXTERNAL_EMOJIS
    | USE_EXTERNAL_STICKERS
    | READ_MESSAGE_HISTORY
    | MENTION_EVERYONE
    | SEND_MESSAGES_IN_THREADS
    | CREATE_PUBLIC_THREADS
    | SEND_POLLS
    | CHANGE_NICKNAME
    | CONNECT
    | SPEAK
    | STREAM
    | USE_VAD
    | USE_SOUNDBOARD
    | USE_APPLICATION_COMMANDS
    | REQUEST_TO_SPEAK;

/// `true` si `perms` contient **toutes** les permissions de `needed`.
pub fn has(perms: u64, needed: u64) -> bool {
    perms & needed == needed
}

/// Parse un bitfield depuis sa représentation chaîne (décimale). Vide/erreur → 0.
pub fn parse(s: &str) -> u64 {
    s.trim().parse::<u64>().unwrap_or(0)
}
