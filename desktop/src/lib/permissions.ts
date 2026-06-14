// Permissions Discord (bitfield u64 sérialisé en **chaîne décimale**).
// Manipulé en BigInt (un u64 dépasse la précision de Number). Cf. crates/ozone-proto/src/perms.rs.

// Catégorie d'affichage (fidélité Discord : permissions groupées).
export type PermCat = "general" | "membership" | "text" | "voice" | "advanced";

export interface PermDef {
  bit: bigint;
  key: string;
  label: string;
  cat: PermCat;
  desc: string; // description courte (style Discord)
}

// Ordre & libellés des catégories.
export const PERM_CATEGORIES: { id: PermCat; label: string }[] = [
  { id: "general", label: "Permissions générales du serveur" },
  { id: "membership", label: "Adhésion" },
  { id: "text", label: "Permissions des salons textuels" },
  { id: "voice", label: "Permissions des salons vocaux" },
  { id: "advanced", label: "Permissions avancées" },
];

// Sous-ensemble éditable (le plus utile), libellé en français, groupé par catégorie.
export const PERMISSIONS: PermDef[] = [
  // Général
  { bit: 1n << 10n, key: "VIEW_CHANNEL", label: "Voir les salons", cat: "general", desc: "Permet aux membres de voir les salons par défaut (hors salons privés)." },
  { bit: 1n << 4n, key: "MANAGE_CHANNELS", label: "Gérer les salons", cat: "general", desc: "Permet de créer, modifier et supprimer des salons." },
  { bit: 1n << 28n, key: "MANAGE_ROLES", label: "Gérer les rôles", cat: "general", desc: "Permet de créer et modifier les rôles situés sous leur rôle le plus élevé." },
  { bit: 1n << 30n, key: "MANAGE_EXPRESSIONS", label: "Gérer les expressions", cat: "general", desc: "Permet d'ajouter ou retirer émojis, autocollants et sons personnalisés." },
  { bit: 1n << 7n, key: "VIEW_AUDIT_LOG", label: "Voir le journal d'audit", cat: "general", desc: "Permet de consulter l'historique des actions de modération du serveur." },
  { bit: 1n << 29n, key: "MANAGE_WEBHOOKS", label: "Gérer les webhooks", cat: "general", desc: "Permet de créer, modifier et supprimer des webhooks." },
  { bit: 1n << 5n, key: "MANAGE_GUILD", label: "Gérer le serveur", cat: "general", desc: "Permet de changer le nom du serveur, sa région et d'autres réglages." },

  // Adhésion
  { bit: 1n << 0n, key: "CREATE_INSTANT_INVITE", label: "Créer une invitation", cat: "membership", desc: "Permet d'inviter de nouveaux membres sur le serveur." },
  { bit: 1n << 27n, key: "MANAGE_NICKNAMES", label: "Gérer les pseudos", cat: "membership", desc: "Permet de modifier le pseudo des autres membres." },
  { bit: 1n << 1n, key: "KICK_MEMBERS", label: "Expulser des membres", cat: "membership", desc: "Permet de retirer des membres ; ils pourront revenir avec une invitation." },
  { bit: 1n << 2n, key: "BAN_MEMBERS", label: "Bannir des membres", cat: "membership", desc: "Permet de bannir définitivement des membres du serveur." },
  { bit: 1n << 40n, key: "MODERATE_MEMBERS", label: "Exclure temporairement", cat: "membership", desc: "Empêche temporairement un membre d'écrire, parler ou réagir." },

  // Salons textuels
  { bit: 1n << 11n, key: "SEND_MESSAGES", label: "Envoyer des messages", cat: "text", desc: "Permet d'envoyer des messages dans les salons textuels." },
  { bit: 1n << 6n, key: "ADD_REACTIONS", label: "Ajouter des réactions", cat: "text", desc: "Permet d'ajouter de nouvelles réactions à un message." },
  { bit: 1n << 14n, key: "EMBED_LINKS", label: "Intégrer des liens", cat: "text", desc: "Affiche un aperçu enrichi des liens partagés." },
  { bit: 1n << 15n, key: "ATTACH_FILES", label: "Joindre des fichiers", cat: "text", desc: "Permet d'envoyer des fichiers et des images." },
  { bit: 1n << 16n, key: "READ_MESSAGE_HISTORY", label: "Voir l'historique", cat: "text", desc: "Permet de lire les messages antérieurs des salons." },
  { bit: 1n << 17n, key: "MENTION_EVERYONE", label: "Mentionner @everyone", cat: "text", desc: "Permet d'utiliser @everyone et @here pour notifier tout le monde." },
  { bit: 1n << 13n, key: "MANAGE_MESSAGES", label: "Gérer les messages", cat: "text", desc: "Permet de supprimer ou d'épingler les messages des autres." },
  { bit: 1n << 51n, key: "PIN_MESSAGES", label: "Épingler des messages", cat: "text", desc: "Permet d'épingler et de désépingler des messages." },

  // Vocal
  { bit: 1n << 20n, key: "CONNECT", label: "Se connecter", cat: "voice", desc: "Permet de rejoindre les salons vocaux et d'entendre les autres." },
  { bit: 1n << 21n, key: "SPEAK", label: "Parler", cat: "voice", desc: "Permet de parler dans les salons vocaux." },
  { bit: 1n << 22n, key: "MUTE_MEMBERS", label: "Rendre muets les membres", cat: "voice", desc: "Permet de couper le micro d'autres membres en vocal." },
  { bit: 1n << 23n, key: "DEAFEN_MEMBERS", label: "Mettre en sourdine", cat: "voice", desc: "Empêche d'autres membres d'entendre le salon vocal." },
  { bit: 1n << 24n, key: "MOVE_MEMBERS", label: "Déplacer des membres", cat: "voice", desc: "Permet de déplacer des membres entre les salons vocaux." },

  // Événements & avancé
  { bit: 1n << 44n, key: "CREATE_EVENTS", label: "Créer des événements", cat: "advanced", desc: "Permet de créer des événements programmés." },
  { bit: 1n << 33n, key: "MANAGE_EVENTS", label: "Gérer les événements", cat: "advanced", desc: "Permet de modifier et d'annuler les événements programmés." },
  { bit: 1n << 3n, key: "ADMINISTRATOR", label: "Administrateur", cat: "advanced", desc: "Accorde toutes les permissions et contourne les restrictions de salon. À donner avec prudence." },
];

// Bits nommés (pour gating de l'UI). Cf. crates/ozone-proto/src/perms.rs.
export const PERM = {
  CREATE_INSTANT_INVITE: 1n << 0n,
  KICK_MEMBERS: 1n << 1n,
  BAN_MEMBERS: 1n << 2n,
  ADMINISTRATOR: 1n << 3n,
  MANAGE_CHANNELS: 1n << 4n,
  MANAGE_GUILD: 1n << 5n,
  VIEW_AUDIT_LOG: 1n << 7n,
  MANAGE_MESSAGES: 1n << 13n,
  MENTION_EVERYONE: 1n << 17n,
  MUTE_MEMBERS: 1n << 22n,
  DEAFEN_MEMBERS: 1n << 23n,
  MOVE_MEMBERS: 1n << 24n,
  CHANGE_NICKNAME: 1n << 26n,
  MANAGE_NICKNAMES: 1n << 27n,
  MANAGE_ROLES: 1n << 28n,
  MANAGE_WEBHOOKS: 1n << 29n,
  MANAGE_EXPRESSIONS: 1n << 30n, // émoji / autocollants / soundboard
  MANAGE_EVENTS: 1n << 33n,
  MODERATE_MEMBERS: 1n << 40n,
  CREATE_EVENTS: 1n << 44n,
} as const;

// Toutes les permissions (propriétaire / administrateur).
export const PERM_ALL = (1n << 64n) - 1n;

function toBig(perms: string): bigint {
  try {
    return BigInt(perms || "0");
  } catch {
    return 0n;
  }
}

export function hasPerm(perms: string, bit: bigint): boolean {
  return (toBig(perms) & bit) === bit;
}

export function togglePerm(perms: string, bit: bigint, on: boolean): string {
  const cur = toBig(perms);
  return (on ? cur | bit : cur & ~bit).toString();
}
