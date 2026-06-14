-- Rôle @everyone explicite pour TOUS les membres existants (id du rôle == guild_id).
-- Désormais attribué à chaque arrivée (create_guild / join_invite / discovery) ; on rattrape ici
-- les membres déjà présents pour cohérence.
INSERT OR IGNORE INTO member_roles (guild_id, user_id, role_id)
SELECT guild_id, user_id, guild_id FROM guild_members;
