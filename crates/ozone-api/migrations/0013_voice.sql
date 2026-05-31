-- S16 : signalisation vocale (états vocaux). Le transport média (SFU/SRTP) et le
-- chiffrement E2EE (DAVE/MLS) sont un sous-projet média séparé. Cf. docs/06-infrastructure-vocale.md.

-- Un utilisateur est connecté à au plus un salon vocal à la fois (clé = user_id).
CREATE TABLE voice_states (
    user_id     INTEGER PRIMARY KEY,
    guild_id    INTEGER NOT NULL,
    channel_id  INTEGER NOT NULL,
    session_id  TEXT    NOT NULL,
    self_mute   INTEGER NOT NULL DEFAULT 0,
    self_deaf   INTEGER NOT NULL DEFAULT 0,
    self_video  INTEGER NOT NULL DEFAULT 0,
    self_stream INTEGER NOT NULL DEFAULT 0,
    mute        INTEGER NOT NULL DEFAULT 0,   -- mute serveur (modération)
    deaf        INTEGER NOT NULL DEFAULT 0,   -- deaf serveur (modération)
    suppress    INTEGER NOT NULL DEFAULT 0,   -- audience d'un salon stage
    joined_at   INTEGER NOT NULL
);
CREATE INDEX idx_voice_channel ON voice_states (channel_id);
CREATE INDEX idx_voice_guild ON voice_states (guild_id);
