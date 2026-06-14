import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import {
  ArrowUpDown,
  Ban as BanIcon,
  CalendarClock,
  Check,
  Crown,
  Layers,
  Lock,
  MoreVertical,
  ScrollText,
  Search,
  Settings as SettingsIcon,
  Shield,
  ShieldCheck,
  Smile,
  Sticker,
  Trash2,
  Upload,
  UserPlus,
  Users,
  Volume2,
  X,
} from "lucide-react";
import { api } from "../api";
import { canIn, permsIn, roleColorHex, useStore } from "../store";
import { PERM } from "../lib/permissions";
import { colorFor, displayName, initials, snowflakeMs, timeAgo } from "../lib/format";
import { mediaUrl } from "../lib/instance";
import { OVERLAY_ANIM, staggerDelay } from "../lib/anim";
import { GAMES, gameName } from "../lib/games";
import { CH_TEXT, CH_VOICE, type Invite, type Member, type Role, type Snowflake } from "../types";
import { Avatar } from "./Avatar";
import { RolesPage } from "./RolesPage";
import { BansModal } from "./BansModal";
import { AuditLogModal } from "./AuditLogModal";
import { EmojiModal } from "./EmojiModal";
import { StickersPage, SoundboardPage } from "./ExpressionPages";
import { AutomodPage } from "./AutomodPage";
import { ImageCropModal } from "./ImageCropModal";
import { Spinner } from "./ui/Spinner";

type PageId =
  | "overview" | "template"
  | "emoji" | "stickers" | "soundboard"
  | "members" | "roles" | "invites"
  | "safety" | "audit" | "bans";

interface NavItem {
  id: PageId;
  label: string;
  icon: React.ReactNode;
  perm?: bigint; // permission requise pour voir la page (sinon visible par tous)
}
interface NavGroup {
  title: string;
  items: NavItem[];
}

const I = (n: React.ReactNode) => n;
const NAV: NavGroup[] = [
  {
    title: "Serveur",
    items: [
      { id: "overview", label: "Aperçu", icon: I(<SettingsIcon size={16} />), perm: PERM.MANAGE_GUILD },
      { id: "template", label: "Modèle de serveur", icon: I(<Layers size={16} />), perm: PERM.MANAGE_GUILD },
    ],
  },
  {
    title: "Expression",
    items: [
      { id: "emoji", label: "Émoji", icon: I(<Smile size={16} />), perm: PERM.MANAGE_EXPRESSIONS },
      { id: "stickers", label: "Autocollants", icon: I(<Sticker size={16} />), perm: PERM.MANAGE_EXPRESSIONS },
      { id: "soundboard", label: "Soundboard", icon: I(<Volume2 size={16} />), perm: PERM.MANAGE_EXPRESSIONS },
    ],
  },
  {
    title: "Personnes",
    items: [
      { id: "members", label: "Membres", icon: I(<Users size={16} />) },
      { id: "roles", label: "Rôles", icon: I(<Shield size={16} />), perm: PERM.MANAGE_ROLES },
      { id: "invites", label: "Invitations", icon: I(<UserPlus size={16} />), perm: PERM.MANAGE_GUILD },
    ],
  },
  {
    title: "Modération",
    items: [
      { id: "safety", label: "Configuration de Sécurité", icon: I(<ShieldCheck size={16} />), perm: PERM.MANAGE_GUILD },
      { id: "audit", label: "Logs du serveur", icon: I(<ScrollText size={16} />), perm: PERM.VIEW_AUDIT_LOG },
      { id: "bans", label: "Bannissements", icon: I(<BanIcon size={16} />), perm: PERM.BAN_MEMBERS },
    ],
  },
];

export function ServerSettings({ guildId, onClose }: { guildId: Snowflake; onClose: () => void }) {
  const guild = useStore((s) => s.guilds.find((g) => g.id === guildId));
  const me = useStore((s) => s.me);
  const refreshGuilds = useStore((s) => s.refreshGuilds);
  const selectHome = useStore((s) => s.selectHome);
  const perms = useStore((s) => permsIn(s, guildId)); // bitfield effectif (stable sous Object.is)
  const isOwner = !!guild && guild.owner_id === me?.id;
  const [confirmDelete, setConfirmDelete] = useState(false);

  async function destroyGuild() {
    try {
      await api.deleteGuild(guildId);
      await refreshGuilds();
      onClose();
      await selectHome();
    } catch {
      setConfirmDelete(false);
    }
  }

  // Groupes/pages visibles selon les permissions.
  const groups = useMemo(
    () =>
      NAV.map((g) => ({
        ...g,
        items: g.items.filter((it) => !it.perm || (perms & it.perm) === it.perm),
      })).filter((g) => g.items.length > 0),
    [perms],
  );
  const firstPage = groups[0]?.items[0]?.id ?? "overview";
  const [page, setPage] = useState<PageId>(firstPage);
  const active = groups.some((g) => g.items.some((it) => it.id === page)) ? page : firstPage;

  const [closing, setClosing] = useState(false);
  const requestClose = useCallback(() => setClosing(true), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") requestClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [requestClose]);

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6 ${
        closing ? "animate-overlay-out" : "animate-overlay-in"
      }`}
      onClick={requestClose}
      onAnimationEnd={() => {
        if (closing) onClose();
      }}
    >
      <div
        className={`relative flex h-[85vh] max-h-[820px] w-[72vw] min-w-[820px] max-w-[1180px] overflow-hidden rounded-xl border border-line bg-modal shadow-2xl ${
          closing ? "animate-pop-out" : "animate-pop-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Navigation */}
        <div className="flex w-[232px] shrink-0 flex-col overflow-y-auto bg-modal-nav py-6 pl-4 pr-2 scroll-thin">
          <div className="mb-1 truncate px-2.5 text-xs font-bold uppercase tracking-wide text-channel">
            {guild?.name ?? "Serveur"}
          </div>
          {groups.map((g) => (
            <div key={g.title} className="mb-3">
              <div className="mb-1 px-2.5 pt-2 text-[11px] font-bold uppercase tracking-wide text-muted">
                {g.title}
              </div>
              {g.items.map((it) => (
                <button
                  key={it.id}
                  onClick={() => setPage(it.id)}
                  className={`pressable relative mb-0.5 flex w-full items-center gap-2 rounded px-2.5 py-1.5 text-left text-sm transition-colors duration-150 ${
                    active === it.id
                      ? "bg-selected text-header"
                      : "text-channel hover:translate-x-0.5 hover:bg-hover hover:text-normal"
                  }`}
                >
                  <span
                    className={`absolute left-0 top-1/2 w-1 -translate-y-1/2 rounded-r-full bg-accent transition-all duration-200 ${
                      active === it.id ? "h-5 opacity-100" : "h-0 opacity-0"
                    }`}
                  />
                  <span className="shrink-0 text-muted">{it.icon}</span>
                  <span className="truncate">{it.label}</span>
                </button>
              ))}
            </div>
          ))}
          {isOwner && (
            <button
              onClick={() => setConfirmDelete(true)}
              className="pressable mt-1 flex w-full items-center justify-between rounded px-2.5 py-1.5 text-left text-sm text-dnd transition-colors hover:bg-dnd/15"
            >
              <span className="truncate">Supprimer le serveur</span>
              <Trash2 size={15} className="shrink-0" />
            </button>
          )}
        </div>

        {/* Contenu — keyé sur la page pour une entrée douce à chaque changement. */}
        <div className="relative flex-1 overflow-hidden bg-modal">
          <div key={active} className="h-full animate-msg-in">
            <Page page={active} guildId={guildId} />
          </div>
        </div>

        {/* Fermer */}
        <button
          onClick={requestClose}
          className="group absolute right-5 top-5 z-10 flex flex-col items-center text-muted transition-colors hover:text-header"
          title="Fermer (Échap)"
        >
          <span className="pressable flex h-9 w-9 items-center justify-center rounded-full border border-muted/40 transition-transform group-hover:rotate-90">
            <X size={18} />
          </span>
          <span className="mt-1 text-[11px] font-semibold">ÉCHAP</span>
        </button>
      </div>

      {confirmDelete && (
        <div
          className="absolute inset-0 z-[60] flex animate-overlay-in items-center justify-center bg-black/60"
          onClick={(e) => {
            e.stopPropagation();
            setConfirmDelete(false);
          }}
        >
          <div
            className="w-[440px] animate-pop-in rounded-xl border border-line bg-modal p-6 shadow-2xl surface-card"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-2 text-lg font-bold text-header">Supprimer « {guild?.name} »</h3>
            <p className="mb-5 text-sm text-muted">
              Es-tu sûr ? Cette action est définitive : tous les salons, messages et données du serveur
              seront supprimés.
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setConfirmDelete(false)}
                className="pressable rounded-lg px-4 py-2 text-sm text-normal transition-colors hover:bg-hover"
              >
                Annuler
              </button>
              <button
                onClick={() => void destroyGuild()}
                className="pressable rounded-lg bg-dnd px-4 py-2 text-sm font-semibold text-white ring-dnd/40 transition hover:opacity-90 hover:ring-2"
              >
                Supprimer le serveur
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function Page({ page, guildId }: { page: PageId; guildId: Snowflake }) {
  // Les pages adossées à une logique existante sont rendues « embedded ».
  if (page === "roles") return <RolesPage guildId={guildId} />;
  if (page === "bans") return <Embed><BansModal guildId={guildId} embedded /></Embed>;
  if (page === "audit") return <Embed><AuditLogModal guildId={guildId} embedded /></Embed>;
  if (page === "emoji") return <Embed><EmojiModal guildId={guildId} embedded /></Embed>;
  if (page === "stickers") return <Embed><StickersPage guildId={guildId} /></Embed>;
  if (page === "soundboard") return <Embed><SoundboardPage guildId={guildId} /></Embed>;
  if (page === "safety") return <Embed><AutomodPage guildId={guildId} /></Embed>;
  if (page === "overview") return <OverviewPage guildId={guildId} />;
  if (page === "members") return <MembersPage guildId={guildId} />;
  if (page === "invites") return <Scroll><InvitesPage guildId={guildId} /></Scroll>;
  return <Scroll><Placeholder page={page} /></Scroll>;
}

// Conteneur scrollable pour les pages « formulaire » (style Discord : colonne centrée).
function Scroll({ children }: { children: React.ReactNode }) {
  return (
    <div className="h-full overflow-y-auto px-10 py-14 scroll-thin">
      <div className="mx-auto max-w-[740px]">{children}</div>
    </div>
  );
}
// Conteneur plein cadre pour les pages denses (rôles, bans, etc.).
function Embed({ children }: { children: React.ReactNode }) {
  return <div className="h-full p-4">{children}</div>;
}

function PageTitle({ title, desc }: { title: string; desc?: string }) {
  return (
    <div className="mb-6">
      <h2 className="text-xl font-bold text-header">{title}</h2>
      {desc && <p className="mt-1 text-sm text-muted">{desc}</p>}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      <label className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">{label}</label>
      {children}
    </div>
  );
}

// ───────────────────────────── Aperçu ─────────────────────────────

// Couleurs de base des dégradés de bannière (vers le bas).
const BANNER_COLORS = [
  0x6b7256, 0xf0359b, 0xf23f42, 0xe6792b, 0xf0b132, 0x9b59b6, 0x3aa0ff, 0x2dd4bf, 0x5a8f29,
  0x4e5058,
];

function bannerStyle(color: number | null, url: string | null): React.CSSProperties {
  if (url) return { backgroundImage: `url(${url})`, backgroundSize: "cover", backgroundPosition: "center" };
  const hex = roleColorHex(color ?? 0x4e5058);
  return { background: `linear-gradient(180deg, ${hex}, #0c0c0e)` };
}

// Transfert de propriété : choix d'un membre + confirmation par saisie du nom du serveur.
function TransferOwnership({ guildId }: { guildId: Snowflake }) {
  const guild = useStore((s) => s.guilds.find((g) => g.id === guildId));
  const members = useStore((s) => s.membersByGuild[guildId]);
  const me = useStore((s) => s.me);
  const refreshGuilds = useStore((s) => s.refreshGuilds);
  const [target, setTarget] = useState<Member | null>(null);
  const [confirmName, setConfirmName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Membres éligibles (tous sauf moi).
  const candidates = (members ?? []).filter((m) => m.user.id !== me?.id);

  async function transfer() {
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      await api.transferGuild(guildId, target.user.id);
      await refreshGuilds();
      setTarget(null);
      setConfirmName("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec du transfert.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Field label="Transférer la propriété">
      <p className="-mt-1 mb-3 text-sm text-muted">
        Désigne un autre membre comme propriétaire du serveur. Tu perdras tous les droits du
        propriétaire. <span className="text-dnd">Cette action est irréversible.</span>
      </p>
      <div className="flex flex-col gap-2">
        <select
          value={target?.user.id ?? ""}
          onChange={(e) => {
            const m = candidates.find((c) => c.user.id === e.target.value) ?? null;
            setTarget(m);
            setConfirmName("");
            setError(null);
          }}
          className="w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        >
          <option value="">Choisir un membre…</option>
          {candidates.map((m) => (
            <option key={m.user.id} value={m.user.id}>
              {m.nick || displayName(m.user)} (@{m.user.username})
            </option>
          ))}
        </select>
        {target && (
          <div className="rounded-lg bg-deepest p-3 ring-1 ring-dnd/40">
            <div className="mb-2 flex items-center gap-2 text-sm text-normal">
              <Crown size={15} className="text-[#f0b232]" />
              <span>
                <span className="font-semibold text-header">
                  {target.nick || displayName(target.user)}
                </span>{" "}
                deviendra propriétaire.
              </span>
            </div>
            <p className="mb-2 text-xs text-muted">
              Tape le nom du serveur (<span className="font-medium text-normal">{guild?.name}</span>)
              pour confirmer.
            </p>
            <input
              value={confirmName}
              onChange={(e) => setConfirmName(e.target.value)}
              placeholder="Nom du serveur"
              className="mb-2 w-full rounded-lg bg-modal px-3 py-2 text-sm text-normal outline-none ring-1 ring-line focus:ring-dnd"
            />
            {error && <p className="mb-2 text-sm text-dnd">{error}</p>}
            <button
              onClick={() => void transfer()}
              disabled={busy || confirmName.trim() !== guild?.name}
              className="pressable inline-flex items-center justify-center gap-2 rounded-lg bg-dnd px-4 py-2 text-sm font-semibold text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
            >
              {busy && <Spinner size={14} />}
              Transférer la propriété
            </button>
          </div>
        )}
      </div>
    </Field>
  );
}

function OverviewPage({ guildId }: { guildId: Snowflake }) {
  const guild = useStore((s) => s.guilds.find((g) => g.id === guildId));
  const memberCount = useStore((s) => s.membersByGuild[guildId]?.length);
  const refreshGuilds = useStore((s) => s.refreshGuilds);
  const channels = useStore((s) => s.channelsByGuild[guildId]);
  const me = useStore((s) => s.me);
  const isOwner = guild?.owner_id === me?.id;
  // Salons texte candidats au salon système.
  const textChannels = (channels ?? []).filter((c) => c.type === CH_TEXT);

  const [name, setName] = useState(guild?.name ?? "");
  const [description, setDescription] = useState(guild?.description ?? "");
  const [iconId, setIconId] = useState<string | null>(guild?.icon_id ?? null);
  const [bannerColor, setBannerColor] = useState<number | null>(guild?.banner_color ?? null);
  const [bannerId, setBannerId] = useState<string | null>(guild?.banner_id ?? null);
  const [games, setGames] = useState<string[]>(guild?.games ?? []);
  const [priv, setPriv] = useState<boolean>(!!guild?.private_profile);
  const [sysChannel, setSysChannel] = useState<string | null>(guild?.system_channel_id ?? null);
  const [afkChannel, setAfkChannel] = useState<string | null>(guild?.afk_channel_id ?? null);
  const [defaultNotif, setDefaultNotif] = useState<number>(guild?.default_message_notifications ?? 0);
  const [vanity, setVanity] = useState<string>(guild?.vanity_code ?? "");
  const voiceChannels = (channels ?? []).filter((c) => c.type === CH_VOICE);
  const [iconPreview, setIconPreview] = useState<string | null>(null);
  const [bannerPreview, setBannerPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [gameQuery, setGameQuery] = useState("");
  const [crop, setCrop] = useState<{ file: File; kind: "icon" | "banner" } | null>(null);
  const iconInput = useRef<HTMLInputElement>(null);
  const bannerInput = useRef<HTMLInputElement>(null);

  const dirty =
    name.trim() !== (guild?.name ?? "") ||
    (description.trim() || "") !== (guild?.description ?? "") ||
    iconId !== (guild?.icon_id ?? null) ||
    bannerColor !== (guild?.banner_color ?? null) ||
    bannerId !== (guild?.banner_id ?? null) ||
    priv !== !!guild?.private_profile ||
    sysChannel !== (guild?.system_channel_id ?? null) ||
    afkChannel !== (guild?.afk_channel_id ?? null) ||
    defaultNotif !== (guild?.default_message_notifications ?? 0) ||
    vanity !== (guild?.vanity_code ?? "") ||
    games.join(",") !== (guild?.games ?? []).join(",");

  function reset() {
    setName(guild?.name ?? "");
    setDescription(guild?.description ?? "");
    setIconId(guild?.icon_id ?? null);
    setBannerColor(guild?.banner_color ?? null);
    setBannerId(guild?.banner_id ?? null);
    setGames(guild?.games ?? []);
    setPriv(!!guild?.private_profile);
    setSysChannel(guild?.system_channel_id ?? null);
    setAfkChannel(guild?.afk_channel_id ?? null);
    setDefaultNotif(guild?.default_message_notifications ?? 0);
    setVanity(guild?.vanity_code ?? "");
    setIconPreview(null);
    setBannerPreview(null);
    setError(null);
  }

  async function upload(file: File, kind: "icon" | "banner") {
    setBusy(true);
    setError(null);
    try {
      const { image_id } = await api.uploadGuildImage(guildId, file);
      const url = URL.createObjectURL(file);
      if (kind === "icon") {
        setIconId(image_id);
        setIconPreview(url);
      } else {
        setBannerId(image_id);
        setBannerPreview(url);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Téléversement échoué.");
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.updateGuild(guildId, {
        name: name.trim(),
        description: description.trim() || null,
        icon_id: iconId ?? "",
        banner_color: bannerColor,
        banner_id: bannerId ?? "",
        games,
        private_profile: priv,
        system_channel_id: sysChannel ?? "0", // "0" = désactiver
        afk_channel_id: afkChannel ?? "0",
        default_message_notifications: defaultNotif,
        vanity_code: vanity.trim(),
      });
      await refreshGuilds();
      setIconPreview(null);
      setBannerPreview(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    } finally {
      setBusy(false);
    }
  }

  function toggleGame(key: string) {
    setGames((g) => (g.includes(key) ? g.filter((k) => k !== key) : g.length >= 12 ? g : [...g, key]));
  }

  const iconUrl = iconPreview ?? (iconId ? mediaUrl(`/api/guilds/${guildId}/icon?v=${guild?.icon_id ?? iconId}`) : null);
  const bannerUrl =
    bannerPreview ?? (bannerId ? mediaUrl(`/api/guilds/${guildId}/banner?v=${guild?.banner_id ?? bannerId}`) : null);
  const filteredGames = GAMES.filter((g) => g.name.toLowerCase().includes(gameQuery.toLowerCase()));

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        {/* Formulaire */}
        <div className="flex-1 overflow-y-auto px-10 py-12 scroll-thin">
          <div className="max-w-[640px]">
            <PageTitle
              title="Profil du serveur"
              desc="Personnalise la façon dont ton serveur apparaît dans les invitations et la découverte."
            />

            <Field label="Nom">
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={100}
                className="w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
              />
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* Icône */}
            <Field label="Icône">
              <p className="-mt-1 mb-3 text-sm text-muted">Nous recommandons une image d'au moins 512×512.</p>
              <div className="flex items-center gap-4">
                <div
                  className="flex h-[80px] w-[80px] shrink-0 items-center justify-center overflow-hidden rounded-2xl text-2xl font-bold text-white"
                  style={iconUrl ? undefined : { backgroundColor: colorFor(guildId) }}
                >
                  {iconUrl ? (
                    <img src={iconUrl} alt="" className="h-full w-full object-cover" />
                  ) : (
                    initials(name || "S")
                  )}
                </div>
                <input
                  ref={iconInput}
                  type="file"
                  accept="image/png,image/jpeg,image/gif,image/webp"
                  className="hidden"
                  onChange={(e) => {
                    const f = e.target.files?.[0];
                    if (f) { if (f.type === "image/gif") void upload(f, "icon"); else setCrop({ file: f, kind: "icon" }); }
                    if (iconInput.current) iconInput.current.value = "";
                  }}
                />
                <button
                  onClick={() => iconInput.current?.click()}
                  disabled={busy}
                  className="rounded-lg btn-accent px-4 py-2 text-sm font-semibold text-white transition disabled:opacity-50"
                >
                  Changer l'icône du serveur
                </button>
                {iconId && (
                  <button
                    onClick={() => {
                      setIconId(null);
                      setIconPreview(null);
                    }}
                    className="text-sm font-medium text-dnd hover:underline"
                  >
                    Supprimer l'icône
                  </button>
                )}
              </div>
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* Bannière */}
            <Field label="Bannière">
              <div className="flex flex-wrap gap-2">
                {BANNER_COLORS.map((c) => (
                  <button
                    key={c}
                    onClick={() => {
                      setBannerColor(c);
                      setBannerId(null);
                      setBannerPreview(null);
                    }}
                    className={`h-12 w-20 rounded-lg ring-2 transition ${
                      !bannerId && bannerColor === c ? "ring-white" : "ring-transparent hover:ring-white/30"
                    }`}
                    style={bannerStyle(c, null)}
                  />
                ))}
              </div>
              <input
                ref={bannerInput}
                type="file"
                accept="image/png,image/jpeg,image/gif,image/webp"
                className="hidden"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) { if (f.type === "image/gif") void upload(f, "banner"); else setCrop({ file: f, kind: "banner" }); }
                  if (bannerInput.current) bannerInput.current.value = "";
                }}
              />
              <div className="mt-3 flex items-center gap-3">
                <button
                  onClick={() => bannerInput.current?.click()}
                  disabled={busy}
                  className="flex items-center gap-2 rounded-lg bg-field px-4 py-2 text-sm font-medium text-normal transition hover:bg-white/10 disabled:opacity-50"
                >
                  <Upload size={16} /> Importer une image
                </button>
                {bannerId && (
                  <button
                    onClick={() => {
                      setBannerId(null);
                      setBannerPreview(null);
                    }}
                    className="text-sm font-medium text-dnd hover:underline"
                  >
                    Retirer l'image
                  </button>
                )}
              </div>
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* Description */}
            <Field label="Description">
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                rows={3}
                maxLength={300}
                placeholder="Présente ce serveur au monde entier."
                className="w-full resize-none rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent placeholder:text-muted focus:ring-accent"
              />
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* Jeux joués */}
            <Field label="Jeux joués">
              <p className="-mt-1 mb-3 text-sm text-muted">
                À quels jeux les membres de ton serveur jouent-ils ? (jusqu'à 12)
              </p>
              <div className="mb-3 flex items-center gap-2 rounded-lg bg-deepest px-3 py-2">
                <Search size={16} className="text-muted" />
                <input
                  value={gameQuery}
                  onChange={(e) => setGameQuery(e.target.value)}
                  placeholder="Rechercher un jeu…"
                  className="flex-1 bg-transparent text-sm text-normal outline-none placeholder:text-muted"
                />
              </div>
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                {filteredGames.map((g) => {
                  const on = games.includes(g.key);
                  return (
                    <button
                      key={g.key}
                      onClick={() => toggleGame(g.key)}
                      className={`relative flex items-center gap-2 overflow-hidden rounded-lg p-2 text-left ring-2 transition ${
                        on ? "ring-accent" : "ring-transparent hover:bg-white/5"
                      }`}
                    >
                      <span
                        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-xs font-bold text-white"
                        style={{ backgroundColor: colorFor(g.key) }}
                      >
                        {initials(g.name)}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-sm text-normal">{g.name}</span>
                      {on && <Check size={16} className="shrink-0 text-accent" />}
                    </button>
                  );
                })}
              </div>
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* Profil privé */}
            <div className="flex items-start justify-between gap-4">
              <div>
                <div className="flex items-center gap-2 text-sm font-bold uppercase tracking-wide text-subtext">
                  <Lock size={14} /> Profil privé
                </div>
                <p className="mt-1.5 max-w-[420px] text-sm text-muted">
                  Quand c'est activé, seuls les membres voient le profil du serveur. Les non-membres (ex. en
                  cliquant sur un emoji custom de ce serveur) verront un accès limité.
                </p>
              </div>
              <button
                onClick={() => setPriv((v) => !v)}
                role="switch"
                aria-checked={priv}
                className={`pressable mt-1 flex h-6 w-11 shrink-0 items-center rounded-full px-0.5 transition-colors duration-200 ${
                  priv ? "bg-online" : "bg-white/15"
                }`}
              >
                <span
                  className={`h-5 w-5 rounded-full bg-white shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${priv ? "translate-x-5" : "translate-x-0.5"}`}
                />
              </button>
            </div>

            <div className="my-6 h-px bg-white/5" />

            {/* Salon système : messages d'arrivée des nouveaux membres */}
            <Field label="Salon des messages système">
              <p className="-mt-1 mb-3 text-sm text-muted">
                Les messages d'arrivée des nouveaux membres y sont publiés.
              </p>
              <select
                value={sysChannel ?? ""}
                onChange={(e) => setSysChannel(e.target.value || null)}
                className="w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
              >
                <option value="">Désactivé (aucun message système)</option>
                {textChannels.map((c) => (
                  <option key={c.id} value={c.id}>
                    # {c.name}
                  </option>
                ))}
              </select>
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* Niveau de notification par défaut */}
            <Field label="Notifications par défaut">
              <p className="-mt-1 mb-3 text-sm text-muted">
                Niveau appliqué aux membres qui n'ont pas réglé le leur.
              </p>
              <select
                value={defaultNotif}
                onChange={(e) => setDefaultNotif(Number(e.target.value))}
                className="w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
              >
                <option value={0}>Tous les messages</option>
                <option value={1}>@mentions seulement</option>
              </select>
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* Salon AFK */}
            <Field label="Salon inactif (AFK)">
              <p className="-mt-1 mb-3 text-sm text-muted">
                Salon vocal vers lequel déplacer les membres inactifs.
              </p>
              <select
                value={afkChannel ?? ""}
                onChange={(e) => setAfkChannel(e.target.value || null)}
                className="w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
              >
                <option value="">Aucun</option>
                {voiceChannels.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </Field>

            <div className="my-6 h-px bg-white/5" />

            {/* URL vanity */}
            <Field label="URL personnalisée (vanity)">
              <p className="-mt-1 mb-3 text-sm text-muted">
                Code d'invitation permanent et mémorable. Lettres, chiffres et tirets.
              </p>
              <div className="flex items-center gap-2 rounded-lg bg-deepest px-3 py-2.5 ring-1 ring-transparent focus-within:ring-accent">
                <span className="text-sm text-muted">/invites/</span>
                <input
                  value={vanity}
                  onChange={(e) => setVanity(e.target.value.replace(/[^A-Za-z0-9-]/g, "").toLowerCase())}
                  maxLength={32}
                  placeholder="mon-serveur"
                  className="flex-1 bg-transparent text-normal outline-none placeholder:text-muted"
                />
              </div>
            </Field>

            {/* Transfert de propriété — propriétaire uniquement */}
            {isOwner && (
              <>
                <div className="my-6 h-px bg-white/5" />
                <TransferOwnership guildId={guildId} />
              </>
            )}

            {error && <p className="mt-5 text-sm text-dnd">{error}</p>}
            <div className="h-4" />
          </div>
        </div>

        {/* Aperçu live */}
        <div className="hidden w-[330px] shrink-0 border-l border-line p-6 xl:block">
          <div className="overflow-hidden rounded-2xl bg-floating shadow-pop ring-1 ring-cardline">
            <div className="h-[110px]" style={bannerStyle(bannerColor, bannerUrl)} />
            <div className="px-4 pb-4">
              <div className="-mt-8 mb-2 flex h-16 w-16 items-center justify-center overflow-hidden rounded-2xl border-[5px] border-floating text-lg font-bold text-white"
                style={iconUrl ? undefined : { backgroundColor: colorFor(guildId) }}
              >
                {iconUrl ? <img src={iconUrl} alt="" className="h-full w-full object-cover" /> : initials(name || "S")}
              </div>
              <div className="text-lg font-bold text-header">{name || "Serveur"}</div>
              <div className="mt-0.5 text-xs text-muted">
                {priv ? "Serveur privé · " : ""}
                {memberCount ?? "—"} membre{(memberCount ?? 0) > 1 ? "s" : ""}
              </div>
              {description.trim() && (
                <p className="mt-2 line-clamp-3 text-sm text-normal">{description}</p>
              )}
              {games.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {games.slice(0, 6).map((k) => (
                    <span
                      key={k}
                      title={gameName(k)}
                      className="flex h-6 w-6 items-center justify-center rounded text-[9px] font-bold text-white"
                      style={{ backgroundColor: colorFor(k) }}
                    >
                      {initials(gameName(k))}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Barre de sauvegarde collante */}
      {dirty && (
        <div className="mx-6 mb-6 flex items-center gap-3 rounded-xl bg-floating px-4 py-3 shadow-pop ring-1 ring-cardline">
          <span className="flex-1 text-sm text-normal">Attention, il reste des modifications non enregistrées !</span>
          <button onClick={reset} className="text-sm font-medium text-muted hover:text-normal hover:underline">
            Réinitialiser
          </button>
          <button
            onClick={() => void save()}
            disabled={busy || !name.trim()}
            className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-success px-4 py-1.5 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy && <Spinner size={14} />}
            Enregistrer les modifications
          </button>
        </div>
      )}

      {crop && (
        <ImageCropModal
          file={crop.file}
          aspect={crop.kind === "icon" ? 1 : 16 / 9}
          outWidth={crop.kind === "icon" ? 512 : 1280}
          outHeight={crop.kind === "icon" ? 512 : 720}
          round={crop.kind === "icon"}
          title={crop.kind === "icon" ? "Recadrer l'icône" : "Recadrer la bannière"}
          onCancel={() => setCrop(null)}
          onConfirm={(blob) => {
            const f = new File([blob], `${crop.kind}.webp`, { type: blob.type || "image/webp" });
            void upload(f, crop.kind);
            setCrop(null);
          }}
        />
      )}
    </div>
  );
}

// ───────────────────────────── Membres ─────────────────────────────

type SortKey = "joined_new" | "joined_old" | "created_new" | "created_old";
const SORT_LABELS: Record<SortKey, string> = {
  joined_new: "Membre depuis (le plus récent)",
  joined_old: "Membre depuis (le plus ancien)",
  created_new: "Compte créé (le plus récent)",
  created_old: "Compte créé (le plus ancien)",
};
const MEMBER_GRID =
  "grid grid-cols-[minmax(150px,1fr)_96px_92px_120px_minmax(110px,1.1fr)_36px] items-center gap-3";

function MembersPage({ guildId }: { guildId: Snowflake }) {
  const guild = useStore((s) => s.guilds.find((g) => g.id === guildId));
  const roles = useStore((s) => s.rolesByGuild[guildId]) ?? [];
  const canKick = useStore((s) => canIn(s, guildId, PERM.KICK_MEMBERS));
  const canBan = useStore((s) => canIn(s, guildId, PERM.BAN_MEMBERS));
  const canRoles = useStore((s) => canIn(s, guildId, PERM.MANAGE_ROLES));
  const [members, setMembers] = useState<Member[] | null>(null);
  const [q, setQ] = useState("");
  const [sort, setSort] = useState<SortKey>("joined_new");
  const [page, setPage] = useState(0);
  const [showInList, setShowInList] = useState(false);
  const PER = 12;
  const ownerId = guild?.owner_id;

  useEffect(() => {
    let alive = true;
    api.listMembers(guildId).then((m) => alive && setMembers(m)).catch(() => alive && setMembers([]));
    return () => {
      alive = false;
    };
  }, [guildId]);

  const roleOf = (id: string): Role | undefined => roles.find((r) => r.id === id);
  const assignable = roles.filter((r) => r.id !== guildId && !r.managed);

  const filtered = useMemo(() => {
    const ql = q.toLowerCase();
    const list = (members ?? []).filter(
      (m) =>
        (m.nick || displayName(m.user)).toLowerCase().includes(ql) ||
        m.user.username.toLowerCase().includes(ql),
    );
    list.sort((a, b) => {
      if (sort === "joined_new") return b.joined_at - a.joined_at;
      if (sort === "joined_old") return a.joined_at - b.joined_at;
      if (sort === "created_new") return snowflakeMs(b.user.id) - snowflakeMs(a.user.id);
      return snowflakeMs(a.user.id) - snowflakeMs(b.user.id);
    });
    return list;
  }, [members, q, sort]);

  const pageCount = Math.max(1, Math.ceil(filtered.length / PER));
  const safePage = Math.min(page, pageCount - 1);
  const items = filtered.slice(safePage * PER, safePage * PER + PER);

  function removeLocal(uid: string) {
    setMembers((p) => p?.filter((m) => m.user.id !== uid) ?? p);
  }
  async function toggleRole(uid: string, rid: string, has: boolean) {
    setMembers((p) =>
      p?.map((m) =>
        m.user.id === uid
          ? { ...m, roles: has ? m.roles.filter((r) => r !== rid) : [...m.roles, rid] }
          : m,
      ) ?? p,
    );
    try {
      await (has ? api.removeMemberRole(guildId, uid, rid) : api.addMemberRole(guildId, uid, rid));
    } catch {
      setMembers((p) =>
        p?.map((m) =>
          m.user.id === uid
            ? { ...m, roles: has ? [...m.roles, rid] : m.roles.filter((r) => r !== rid) }
            : m,
        ) ?? p,
      );
    }
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto px-8 py-12 scroll-thin">
      {/* Bascule (cosmétique, fidélité Discord). `pr-14` : dégage le bouton ✕/ÉCHAP en haut à droite. */}
      <div className="mb-5 flex items-start justify-between gap-6 pr-14">
        <div>
          <h2 className="font-bold text-header">Afficher les membres dans la liste des salons</h2>
          <p className="mt-1 max-w-[560px] text-sm text-muted">
            Affiche une page des membres dans la barre des salons pour repérer rapidement les nouveaux
            venus.
          </p>
        </div>
        <Switch on={showInList} onToggle={() => setShowInList((v) => !v)} />
      </div>
      <div className="mb-5 h-px bg-white/5" />

      <div className="mb-3 flex items-center justify-between gap-3">
        <h3 className="text-lg font-bold text-header">
          Membres{members ? <span className="ml-2 text-sm font-normal text-muted">{members.length}</span> : null}
        </h3>
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-2 rounded-lg bg-deepest px-3 py-1.5">
            <Search size={15} className="text-muted" />
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Rechercher un membre…"
              className="w-44 bg-transparent text-sm text-normal outline-none placeholder:text-muted"
            />
          </div>
          <SortMenu value={sort} onChange={setSort} />
        </div>
      </div>

      {members === null ? (
        <p className="text-sm text-muted">Chargement…</p>
      ) : (
        <>
          <div className="overflow-hidden rounded-xl border border-line">
            <div
              className={`${MEMBER_GRID} border-b border-line bg-deepest/40 px-3 py-2 text-[11px] font-bold uppercase tracking-wide text-muted`}
            >
              <span>Nom</span>
              <span>Membre depuis</span>
              <span>A rejoint</span>
              <span>Méthode</span>
              <span>Rôles</span>
              <span />
            </div>
            {items.map((m, i) => {
              const name = m.nick || displayName(m.user);
              const memberRoles = m.roles
                .map(roleOf)
                .filter((r): r is Role => !!r && r.id !== guildId)
                .sort((a, b) => b.position - a.position);
              const isOwner = m.user.id === ownerId;
              return (
                <div
                  key={m.user.id}
                  className={`${MEMBER_GRID} animate-row-in border-t border-line px-3 py-2 transition-colors hover:bg-hover`}
                  style={staggerDelay(i)}
                >
                  <div className="flex min-w-0 items-center gap-2.5">
                    <Avatar name={name} id={m.user.id} size={32} avatarId={m.user.avatar_id} />
                    <div className="min-w-0">
                      <div
                        className="truncate text-sm font-medium text-header"
                        style={memberRoles[0]?.color ? { color: roleColorHex(memberRoles[0].color) } : undefined}
                      >
                        {name}
                      </div>
                      <div className="truncate text-xs text-muted">
                        {m.user.username}
                        {isOwner ? " · propriétaire" : ""}
                      </div>
                    </div>
                  </div>
                  <span className="truncate text-sm text-muted">{timeAgo(m.joined_at)}</span>
                  <span className="truncate text-sm text-muted">{timeAgo(snowflakeMs(m.user.id))}</span>
                  <span className="truncate text-xs text-muted">
                    {m.joined_via ? (
                      <code className="rounded bg-deepest px-1.5 py-0.5 text-accent">{m.joined_via}</code>
                    ) : (
                      "—"
                    )}
                  </span>
                  <div className="flex min-w-0 flex-wrap gap-1">
                    {memberRoles.slice(0, 3).map((r) => (
                      <span
                        key={r.id}
                        className="flex items-center gap-1 rounded-full bg-deepest px-2 py-0.5 text-xs text-normal"
                      >
                        <span
                          className="h-2 w-2 rounded-full"
                          style={{ backgroundColor: r.color ? roleColorHex(r.color) : "#99aab5" }}
                        />
                        <span className="max-w-[90px] truncate">{r.name}</span>
                      </span>
                    ))}
                    {memberRoles.length > 3 && (
                      <span className="text-xs text-muted">+{memberRoles.length - 3}</span>
                    )}
                  </div>
                  {!isOwner && (canRoles || canKick || canBan) ? (
                    <MemberMenu
                      member={m}
                      assignable={assignable}
                      canRoles={canRoles}
                      canKick={canKick}
                      canBan={canBan}
                      onToggleRole={(rid, has) => void toggleRole(m.user.id, rid, has)}
                      onKick={() => void api.kickMember(guildId, m.user.id).then(() => removeLocal(m.user.id)).catch(() => {})}
                      onBan={() => void api.banMember(guildId, m.user.id).then(() => removeLocal(m.user.id)).catch(() => {})}
                    />
                  ) : (
                    <span />
                  )}
                </div>
              );
            })}
            {items.length === 0 && (
              <p className="px-3 py-8 text-center text-sm text-muted">Aucun membre.</p>
            )}
          </div>

          {pageCount > 1 && (
            <div className="mt-4 flex items-center justify-end gap-1 text-sm">
              <button
                onClick={() => setPage((p) => Math.max(0, p - 1))}
                disabled={safePage === 0}
                className="pressable rounded px-3 py-1 text-muted transition-colors hover:bg-hover hover:text-normal disabled:opacity-40"
              >
                ‹ Retour
              </button>
              {Array.from({ length: pageCount }, (_, i) => (
                <button
                  key={i}
                  onClick={() => setPage(i)}
                  className={`pressable h-7 w-7 rounded-full transition-colors ${
                    i === safePage ? "bg-accent text-white" : "text-muted hover:bg-hover hover:text-normal"
                  }`}
                >
                  {i + 1}
                </button>
              ))}
              <button
                onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
                disabled={safePage >= pageCount - 1}
                className="pressable rounded px-3 py-1 text-muted transition-colors hover:bg-hover hover:text-normal disabled:opacity-40"
              >
                Suivant ›
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function Switch({ on, onToggle }: { on: boolean; onToggle: () => void }) {
  return (
    <button
      onClick={onToggle}
      role="switch"
      aria-checked={on}
      className={`pressable mt-1 flex h-6 w-11 shrink-0 items-center rounded-full px-0.5 transition-colors duration-200 ${
        on ? "bg-online" : "bg-white/15"
      }`}
    >
      <span className={`h-5 w-5 rounded-full bg-white shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${on ? "translate-x-5" : "translate-x-0.5"}`} />
    </button>
  );
}

function SortMenu({ value, onChange }: { value: SortKey; onChange: (s: SortKey) => void }) {
  return (
    <Popover.Root>
      <Popover.Trigger className="flex items-center gap-1.5 rounded-lg bg-deepest px-3 py-1.5 text-sm text-normal outline-none hover:bg-white/5">
        <ArrowUpDown size={15} /> Trier
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          className={`z-[70] w-[260px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          {(Object.keys(SORT_LABELS) as SortKey[]).map((k) => (
            <Popover.Close
              key={k}
              onClick={() => onChange(k)}
              className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
            >
              {SORT_LABELS[k]}
              {value === k && <Check size={15} className="text-accent" />}
            </Popover.Close>
          ))}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function MemberMenu({
  member,
  assignable,
  canRoles,
  canKick,
  canBan,
  onToggleRole,
  onKick,
  onBan,
}: {
  member: Member;
  assignable: Role[];
  canRoles: boolean;
  canKick: boolean;
  canBan: boolean;
  onToggleRole: (roleId: string, has: boolean) => void;
  onKick: () => void;
  onBan: () => void;
}) {
  return (
    <Popover.Root>
      <Popover.Trigger className="flex h-7 w-7 items-center justify-center rounded text-muted outline-none hover:bg-white/10 hover:text-normal">
        <MoreVertical size={16} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={4}
          className={`z-[70] w-[230px] rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          {canRoles && assignable.length > 0 && (
            <>
              <div className="px-2 py-1 text-[11px] font-bold uppercase tracking-wide text-muted">Rôles</div>
              <div className="max-h-48 overflow-y-auto scroll-thin">
                {assignable.map((r) => {
                  const has = member.roles.includes(r.id);
                  return (
                    <button
                      key={r.id}
                      onClick={() => onToggleRole(r.id, has)}
                      className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
                    >
                      <span
                        className="h-2.5 w-2.5 shrink-0 rounded-full"
                        style={{ backgroundColor: r.color ? roleColorHex(r.color) : "#99aab5" }}
                      />
                      <span className="min-w-0 flex-1 truncate">{r.name}</span>
                      {has && <Check size={14} className="shrink-0 text-accent" />}
                    </button>
                  );
                })}
              </div>
              {(canKick || canBan) && <div className="my-1 h-px bg-white/10" />}
            </>
          )}
          {canKick && (
            <Popover.Close
              onClick={onKick}
              className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-sm text-dnd outline-none hover:bg-dnd/15"
            >
              Expulser {member.nick || displayName(member.user)}
            </Popover.Close>
          )}
          {canBan && (
            <Popover.Close
              onClick={onBan}
              className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-sm text-dnd outline-none hover:bg-dnd/15"
            >
              Bannir
            </Popover.Close>
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// ───────────────────────────── Invitations ─────────────────────────────

function InvitesPage({ guildId }: { guildId: Snowflake }) {
  const [invites, setInvites] = useState<Invite[] | null>(null);
  const [busy, setBusy] = useState(false);

  async function reload() {
    try {
      setInvites(await api.listInvites(guildId));
    } catch {
      setInvites([]);
    }
  }
  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  async function create() {
    setBusy(true);
    try {
      await api.createInvite(guildId, {});
      await reload();
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageTitle title="Invitations" desc="Liens d'invitation actifs vers ce serveur." />
      <button
        onClick={() => void create()}
        disabled={busy}
        className="pressable mb-5 inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-4 py-2 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50"
      >
        {busy && <Spinner size={14} />}
        Créer une invitation
      </button>
      {invites === null ? (
        <p className="text-sm text-muted">Chargement…</p>
      ) : invites.length === 0 ? (
        <p className="text-sm text-muted">Aucune invitation active.</p>
      ) : (
        <div className="overflow-hidden rounded-lg border border-line">
          {invites.map((inv, i) => (
            <div
              key={inv.code}
              className={`group flex animate-row-in items-center gap-3 px-3 py-2.5 transition-colors hover:bg-hover ${i > 0 ? "border-t border-line" : ""}`}
              style={staggerDelay(i)}
            >
              <code className="rounded bg-deepest px-2 py-1 text-sm text-accent">{inv.code}</code>
              <div className="flex-1 text-xs text-muted">
                {inv.uses}
                {inv.max_uses ? ` / ${inv.max_uses}` : ""} utilisation{inv.uses > 1 ? "s" : ""}
              </div>
              <div className="text-xs text-muted">{inv.expires_at ? "expire" : "permanent"}</div>
              <button
                title="Révoquer l'invitation"
                onClick={() => void api.revokeInvite(inv.code).then(reload).catch(() => {})}
                className="rounded p-1.5 text-muted opacity-0 transition-opacity hover:bg-dnd/15 hover:text-dnd group-hover:opacity-100"
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

// ───────────────────────────── Placeholders (squelettes) ─────────────────────────────

const PLACEHOLDER: Record<string, { title: string; icon: React.ReactNode; desc: string }> = {
  template: { title: "Modèle de serveur", icon: <Layers size={28} />, desc: "Crée un modèle réutilisable de ce serveur." },
};

function Placeholder({ page }: { page: PageId }) {
  const p = PLACEHOLDER[page] ?? { title: "Bientôt", icon: <CalendarClock size={28} />, desc: "" };
  return (
    <>
      <PageTitle title={p.title} desc={p.desc} />
      <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-line py-16 text-center">
        <span className="text-muted">{p.icon}</span>
        <p className="text-sm text-muted">Cette section arrivera prochainement.</p>
      </div>
    </>
  );
}
