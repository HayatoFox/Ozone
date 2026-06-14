import { useEffect, useMemo, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import {
  Check,
  ChevronDown,
  Folder,
  Hash,
  Lock,
  Plus,
  Search,
  Settings as SettingsIcon,
  Shield,
  Trash2,
  Volume2,
  X,
} from "lucide-react";
import { api } from "../api";
import { canIn, permsIn, roleColorHex, useStore } from "../store";
import { PERM, PERMISSIONS, type PermDef } from "../lib/permissions";
import { displayName } from "../lib/format";
import { OVERLAY_ANIM } from "../lib/anim";
import {
  CH_CATEGORY,
  CH_VOICE,
  type Channel,
  type Member,
  type PermissionOverwrite,
  type Role,
  type Snowflake,
} from "../types";
import { Avatar } from "./Avatar";
import { Spinner } from "./ui/Spinner";

const VIEW_CHANNEL = 1n << 10n;

// ───────────────────────────── Options ─────────────────────────────

const SLOWMODE: { v: number; label: string }[] = [
  { v: 0, label: "Désactivé" },
  { v: 5, label: "5 s" },
  { v: 10, label: "10 s" },
  { v: 15, label: "15 s" },
  { v: 30, label: "30 s" },
  { v: 60, label: "1 min" },
  { v: 120, label: "2 min" },
  { v: 300, label: "5 min" },
  { v: 600, label: "10 min" },
  { v: 900, label: "15 min" },
  { v: 1800, label: "30 min" },
  { v: 3600, label: "1 h" },
  { v: 7200, label: "2 h" },
  { v: 21600, label: "6 h" },
];
const ARCHIVE: { v: number; label: string }[] = [
  { v: 60, label: "1 heure" },
  { v: 1440, label: "24 heures" },
  { v: 4320, label: "3 jours" },
  { v: 10080, label: "1 semaine" },
];
const REGIONS: { v: string; label: string }[] = [
  { v: "", label: "Automatique" },
  { v: "rotterdam", label: "Europe occidentale" },
  { v: "paris", label: "Paris" },
  { v: "us-east", label: "États-Unis Est" },
  { v: "us-central", label: "États-Unis Central" },
  { v: "us-west", label: "États-Unis Ouest" },
  { v: "singapore", label: "Singapour" },
  { v: "brazil", label: "Brésil" },
  { v: "japan", label: "Japon" },
];
const MIN_BITRATE = 8_000;
const MAX_BITRATE = 512_000;

type PageId = "overview" | "permissions";

// ───────────────────────────── Shell ─────────────────────────────

export function ChannelSettings({
  channelId,
  guildId,
  onClose,
}: {
  channelId: Snowflake;
  guildId: Snowflake;
  onClose: () => void;
}) {
  const channel = useStore((s) => s.channelsByGuild[guildId]?.find((c) => c.id === channelId));
  const canManage = useStore((s) => canIn(s, guildId, PERM.MANAGE_CHANNELS));
  const canRoles = useStore((s) => canIn(s, guildId, PERM.MANAGE_ROLES));
  const [page, setPage] = useState<PageId>("overview");
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  if (!channel) return null;
  const isVoice = channel.type === CH_VOICE;
  const isCategory = channel.type === CH_CATEGORY;
  const TypeIcon = isCategory ? Folder : isVoice ? Volume2 : Hash;

  const nav: { id: PageId; label: string; show: boolean }[] = [
    { id: "overview", label: "Vue d'ensemble", show: true },
    { id: "permissions", label: "Permissions", show: canRoles },
  ];
  const visible = nav.filter((n) => n.show);
  const active = visible.some((n) => n.id === page) ? page : "overview";

  async function destroy() {
    try {
      await api.deleteChannel(channelId);
      onClose();
    } catch {
      setConfirmDelete(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex animate-overlay-in items-center justify-center bg-black/60 p-6"
      onClick={onClose}
    >
      <div
        className="relative flex h-[85vh] max-h-[820px] w-[72vw] min-w-[820px] max-w-[1180px] animate-pop-in overflow-hidden rounded-xl border border-line bg-modal shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Navigation */}
        <div className="flex w-[232px] shrink-0 flex-col overflow-y-auto bg-modal-nav py-6 pl-4 pr-2 scroll-thin">
          <div className="mb-2 flex items-center gap-1.5 truncate px-2.5 text-xs font-bold uppercase tracking-wide text-channel">
            <TypeIcon size={13} className="shrink-0" />
            <span className="truncate">{channel.name}</span>
          </div>
          {visible.map((n) => (
            <button
              key={n.id}
              onClick={() => setPage(n.id)}
              className={`mb-0.5 flex w-full items-center gap-2 rounded px-2.5 py-1.5 text-left text-sm transition-colors ${
                active === n.id ? "bg-selected text-header" : "text-channel hover:bg-hover hover:text-normal"
              }`}
            >
              <span className="shrink-0 text-muted">
                {n.id === "overview" ? <SettingsIcon size={16} /> : <Shield size={16} />}
              </span>
              <span className="truncate">{n.label}</span>
            </button>
          ))}
          {canManage && (
            <>
              <div className="my-2 h-px bg-white/5" />
              <button
                onClick={() => setConfirmDelete(true)}
                className="flex w-full items-center justify-between rounded px-2.5 py-1.5 text-left text-sm text-dnd transition-colors hover:bg-dnd/15"
              >
                <span className="truncate">
                  {isCategory ? "Supprimer la catégorie" : "Supprimer le salon"}
                </span>
                <Trash2 size={15} className="shrink-0" />
              </button>
            </>
          )}
        </div>

        {/* Contenu */}
        <div className="relative flex-1 overflow-hidden bg-modal">
          {active === "overview" && <OverviewPage channel={channel} />}
          {active === "permissions" && <ChannelPermissionsPage channel={channel} guildId={guildId} />}
        </div>

        {/* Fermer */}
        <button
          onClick={onClose}
          className="absolute right-5 top-5 z-10 flex flex-col items-center text-muted transition-colors hover:text-header"
          title="Fermer (Échap)"
        >
          <span className="flex h-9 w-9 items-center justify-center rounded-full border border-muted/40">
            <X size={18} />
          </span>
          <span className="mt-1 text-[11px] font-semibold">ÉCHAP</span>
        </button>
      </div>

      {confirmDelete && (
        <div
          className="absolute inset-0 z-[60] flex items-center justify-center bg-black/60"
          onClick={(e) => {
            e.stopPropagation();
            setConfirmDelete(false);
          }}
        >
          <div
            className="w-[440px] rounded-xl border border-line bg-modal p-6 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-2 text-lg font-bold text-header">Supprimer « {channel.name} »</h3>
            <p className="mb-5 text-sm text-muted">
              {isCategory
                ? "Les salons de cette catégorie ne seront pas supprimés mais sortiront de la catégorie."
                : "Es-tu sûr de vouloir supprimer ce salon ? Cette action est définitive."}
            </p>
            <div className="flex justify-end gap-3">
              <button
                onClick={() => setConfirmDelete(false)}
                className="px-4 py-2 text-sm text-normal hover:underline"
              >
                Annuler
              </button>
              <button
                onClick={() => void destroy()}
                className="rounded-lg bg-dnd px-4 py-2 text-sm font-semibold text-white hover:opacity-90"
              >
                Supprimer
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ───────────────────────────── Vue d'ensemble ─────────────────────────────

function OverviewPage({ channel }: { channel: Channel }) {
  const isVoice = channel.type === CH_VOICE;
  const isCategory = channel.type === CH_CATEGORY;

  const [name, setName] = useState(channel.name);
  const [topic, setTopic] = useState(channel.topic ?? "");
  const [slow, setSlow] = useState(channel.rate_limit_per_user);
  const [nsfw, setNsfw] = useState(channel.nsfw);
  const [archive, setArchive] = useState(channel.default_auto_archive ?? 4320);
  const [bitrate, setBitrate] = useState(channel.bitrate ?? 64000);
  const [userLimit, setUserLimit] = useState(channel.user_limit ?? 0);
  const [vqm, setVqm] = useState(channel.video_quality_mode ?? 1);
  const [region, setRegion] = useState(channel.rtc_region ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Re-synchronise les champs si le salon change (event CHANNEL_UPDATE en direct).
  useEffect(() => {
    setName(channel.name);
    setTopic(channel.topic ?? "");
    setSlow(channel.rate_limit_per_user);
    setNsfw(channel.nsfw);
    setArchive(channel.default_auto_archive ?? 4320);
    setBitrate(channel.bitrate ?? 64000);
    setUserLimit(channel.user_limit ?? 0);
    setVqm(channel.video_quality_mode ?? 1);
    setRegion(channel.rtc_region ?? "");
  }, [channel]);

  const dirty =
    name.trim() !== channel.name ||
    (topic.trim() || "") !== (channel.topic ?? "") ||
    slow !== channel.rate_limit_per_user ||
    nsfw !== channel.nsfw ||
    archive !== (channel.default_auto_archive ?? 4320) ||
    bitrate !== (channel.bitrate ?? 64000) ||
    userLimit !== (channel.user_limit ?? 0) ||
    vqm !== (channel.video_quality_mode ?? 1) ||
    region !== (channel.rtc_region ?? "");

  function reset() {
    setName(channel.name);
    setTopic(channel.topic ?? "");
    setSlow(channel.rate_limit_per_user);
    setNsfw(channel.nsfw);
    setArchive(channel.default_auto_archive ?? 4320);
    setBitrate(channel.bitrate ?? 64000);
    setUserLimit(channel.user_limit ?? 0);
    setVqm(channel.video_quality_mode ?? 1);
    setRegion(channel.rtc_region ?? "");
    setError(null);
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.updateChannel(channel.id, {
        name: name.trim(),
        topic: isCategory || isVoice ? undefined : topic.trim() || null,
        rate_limit_per_user: isCategory ? undefined : slow,
        nsfw: isCategory ? undefined : nsfw,
        default_auto_archive: !isVoice && !isCategory ? archive : undefined,
        bitrate: isVoice ? bitrate : undefined,
        user_limit: isVoice ? userLimit : undefined,
        video_quality_mode: isVoice ? vqm : undefined,
        rtc_region: isVoice ? region : undefined,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-10 py-14 scroll-thin">
        <div className="mx-auto max-w-[740px]">
          <h2 className="mb-6 text-xl font-bold text-header">Vue d'ensemble</h2>

          <Field label="Nom du salon">
            <div className="flex items-center gap-2 rounded-lg bg-deepest px-3 ring-1 ring-transparent focus-within:ring-accent">
              {!isCategory && (
                <span className="text-muted">{isVoice ? <Volume2 size={18} /> : <Hash size={18} />}</span>
              )}
              <input
                value={name}
                maxLength={100}
                onChange={(e) => setName(e.target.value)}
                className="w-full bg-transparent py-2.5 text-normal outline-none"
              />
            </div>
          </Field>

          {!isVoice && !isCategory && (
            <>
              <Field label="Sujet du salon">
                <textarea
                  value={topic}
                  onChange={(e) => setTopic(e.target.value)}
                  rows={3}
                  maxLength={1024}
                  placeholder="Apprends à tout le monde comment utiliser ce salon !"
                  className="w-full resize-none rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent placeholder:text-muted focus:ring-accent"
                />
                <div className="mt-1 text-right text-xs text-muted">{1024 - topic.length}</div>
              </Field>
              <Divider />
            </>
          )}

          {!isCategory && (
            <Field label="Mode lent">
              <Select
                value={slow}
                options={SLOWMODE}
                onChange={setSlow}
              />
              <Hint>
                Les membres devront patienter entre deux messages, sauf s'ils possèdent la permission
                Gérer les messages ou Gérer le salon.
              </Hint>
            </Field>
          )}

          {!isCategory && (
            <Toggle
              label="Salon soumis à une limite d'âge"
              desc="Les utilisateurs devront confirmer qu'ils ont l'âge légal pour voir le contenu de ce salon."
              on={nsfw}
              onToggle={() => setNsfw((v) => !v)}
            />
          )}

          {/* ── Vocal ── */}
          {isVoice && (
            <>
              <Divider />
              <Field label="Débit binaire">
                <input
                  type="range"
                  min={MIN_BITRATE}
                  max={MAX_BITRATE}
                  step={1000}
                  value={bitrate}
                  onChange={(e) => setBitrate(Number(e.target.value))}
                  className="w-full accent-blurple"
                />
                <div className="mt-1 flex items-center justify-between text-xs text-muted">
                  <span>8 kbps</span>
                  <span className="font-semibold text-normal">{Math.round(bitrate / 1000)} kbps</span>
                  <span>512 kbps</span>
                </div>
                <Hint>
                  TOUS LES OCTETS ! Au-delà de 96 kbps, la qualité grimpe — utile en LAN, plus exigeant
                  pour les petites connexions.
                </Hint>
              </Field>

              <Field label="Qualité de la vidéo">
                <Radio
                  value={vqm}
                  options={[
                    { v: 1, label: "Automatique" },
                    { v: 2, label: "720p" },
                  ]}
                  onChange={setVqm}
                />
              </Field>

              <Field label="Limite d'utilisateurs">
                <input
                  type="range"
                  min={0}
                  max={99}
                  step={1}
                  value={userLimit}
                  onChange={(e) => setUserLimit(Number(e.target.value))}
                  className="w-full accent-blurple"
                />
                <div className="mt-1 flex items-center justify-between text-xs text-muted">
                  <span>∞</span>
                  <span className="font-semibold text-normal">
                    {userLimit === 0 ? "Aucune limite" : `${userLimit} utilisateur${userLimit > 1 ? "s" : ""}`}
                  </span>
                  <span>99</span>
                </div>
              </Field>

              <Field label="Imposer la région">
                <Select
                  value={region}
                  options={REGIONS}
                  onChange={setRegion}
                />
              </Field>
            </>
          )}

          {/* ── Texte : masquage des fils ── */}
          {!isVoice && !isCategory && (
            <>
              <Divider />
              <Field label="Masquer après une période d'inactivité">
                <Select value={archive} options={ARCHIVE} onChange={setArchive} />
                <Hint>
                  Les nouveaux fils n'apparaîtront plus dans la liste des salons après cette période
                  d'inactivité.
                </Hint>
              </Field>
            </>
          )}

          {error && <p className="mt-5 text-sm text-dnd">{error}</p>}
          <div className="h-4" />
        </div>
      </div>

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
            {busy && <Spinner size={14} />}Enregistrer
          </button>
        </div>
      )}
    </div>
  );
}

// ───────────────────────────── Permissions ─────────────────────────────

const byKey = new Map(PERMISSIONS.map((p) => [p.key, p]));

// Libellés/descriptions SPÉCIFIQUES AU SALON : au niveau d'un salon, certains bits ont un sens
// différent du niveau serveur. Notamment MANAGE_ROLES = « Gérer les permissions » (de CE salon),
// et VIEW_CHANNEL = « Voir LE salon » (et non « les salons »).
const CHANNEL_OVERRIDES: Record<string, { label: string; desc: string }> = {
  VIEW_CHANNEL: {
    label: "Voir le salon",
    desc: "Permet aux membres de voir ce salon. En le désactivant pour @everyone, le salon devient privé.",
  },
  MANAGE_CHANNELS: {
    label: "Gérer le salon",
    desc: "Permet de modifier le nom et les paramètres de ce salon, et de le supprimer.",
  },
  MANAGE_ROLES: {
    label: "Gérer les permissions",
    desc: "Permet de changer les permissions de ce salon.",
  },
  MANAGE_WEBHOOKS: {
    label: "Gérer les webhooks",
    desc: "Permet de créer, modifier et supprimer des webhooks dans ce salon.",
  },
};

// Récupère un PermDef en appliquant le libellé/description propre au contexte salon.
const pick = (keys: string[]): PermDef[] =>
  keys
    .map((k) => {
      const base = byKey.get(k);
      if (!base) return undefined;
      const ov = CHANNEL_OVERRIDES[k];
      return ov ? { ...base, label: ov.label, desc: ov.desc } : base;
    })
    .filter((p): p is PermDef => !!p);

function permGroups(type: number): { label: string; perms: PermDef[] }[] {
  const general = pick(["VIEW_CHANNEL", "MANAGE_CHANNELS", "MANAGE_ROLES", "MANAGE_WEBHOOKS"]);
  const membership = pick(["CREATE_INSTANT_INVITE"]);
  if (type === CH_VOICE) {
    return [
      { label: "Permissions générales de salon", perms: general },
      { label: "Permissions des membres", perms: membership },
      { label: "Permissions vocales", perms: pick(["CONNECT", "SPEAK", "MUTE_MEMBERS", "DEAFEN_MEMBERS", "MOVE_MEMBERS"]) },
    ];
  }
  const text = pick([
    "SEND_MESSAGES",
    "ADD_REACTIONS",
    "EMBED_LINKS",
    "ATTACH_FILES",
    "READ_MESSAGE_HISTORY",
    "MENTION_EVERYONE",
    "MANAGE_MESSAGES",
    "PIN_MESSAGES",
  ]);
  return [
    { label: "Permissions générales de salon", perms: general },
    { label: "Permissions des membres", perms: membership },
    { label: "Permissions des salons textuels", perms: text },
  ];
}

type Target = { id: Snowflake; type: number; name: string; color?: number };

function ChannelPermissionsPage({ channel, guildId }: { channel: Channel; guildId: Snowflake }) {
  const roles = useStore((s) => s.rolesByGuild[guildId]) ?? [];
  const actorPerms = useStore((s) => permsIn(s, guildId));
  const [overwrites, setOverwrites] = useState<PermissionOverwrite[] | null>(null);
  const [extraTargets, setExtraTargets] = useState<Target[]>([]);
  const [members, setMembers] = useState<Member[] | null>(null);
  const [sel, setSel] = useState<Snowflake>(guildId); // @everyone par défaut
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setOverwrites(null);
    api
      .listOverwrites(channel.id)
      .then((o) => alive && setOverwrites(o))
      .catch(() => alive && setOverwrites([]));
    return () => {
      alive = false;
    };
  }, [channel.id]);

  const roleById = useMemo(() => new Map(roles.map((r) => [r.id, r])), [roles]);
  const everyone = roleById.get(guildId);

  // Liste des cibles affichées : @everyone, puis rôles/membres ayant une surcharge ou ajoutés.
  const targets: Target[] = useMemo(() => {
    const map = new Map<Snowflake, Target>();
    if (everyone) map.set(guildId, { id: guildId, type: 0, name: "@everyone", color: everyone.color });
    for (const o of overwrites ?? []) {
      if (map.has(o.id)) continue;
      if (o.type === 0) {
        const r = roleById.get(o.id);
        if (r) map.set(o.id, { id: o.id, type: 0, name: r.name, color: r.color });
      } else {
        const m = members?.find((x) => x.user.id === o.id);
        map.set(o.id, { id: o.id, type: 1, name: m ? m.nick || displayName(m.user) : "Membre" });
      }
    }
    for (const t of extraTargets) if (!map.has(t.id)) map.set(t.id, t);
    return [...map.values()];
  }, [overwrites, extraTargets, roleById, members, everyone, guildId]);

  const owOf = (id: Snowflake) => overwrites?.find((o) => o.id === id);
  const allowOf = (id: Snowflake) => {
    try {
      return BigInt(owOf(id)?.allow ?? "0");
    } catch {
      return 0n;
    }
  };
  const denyOf = (id: Snowflake) => {
    try {
      return BigInt(owOf(id)?.deny ?? "0");
    } catch {
      return 0n;
    }
  };

  const selTarget = targets.find((t) => t.id === sel) ?? targets[0];
  const isPrivate = (denyOf(guildId) & VIEW_CHANNEL) === VIEW_CHANNEL;

  async function persist(target: Target, allow: bigint, deny: bigint) {
    // Mise à jour optimiste locale.
    setOverwrites((cur) => {
      const next = (cur ?? []).filter((o) => o.id !== target.id);
      if (allow !== 0n || deny !== 0n) {
        next.push({ id: target.id, type: target.type, allow: allow.toString(), deny: deny.toString() });
      }
      return next;
    });
    try {
      if (allow === 0n && deny === 0n) {
        await api.deleteOverwrite(channel.id, target.id);
      } else {
        await api.setOverwrite(channel.id, target.id, {
          type: target.type,
          allow: allow.toString(),
          deny: deny.toString(),
        });
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec.");
      // Rechargement pour resynchroniser en cas d'échec.
      api.listOverwrites(channel.id).then(setOverwrites).catch(() => {});
    }
  }

  function setState(target: Target, bit: bigint, state: "allow" | "deny" | "neutral") {
    let allow = allowOf(target.id) & ~bit;
    let deny = denyOf(target.id) & ~bit;
    if (state === "allow") allow |= bit;
    else if (state === "deny") deny |= bit;
    void persist(target, allow, deny);
  }

  function togglePrivate() {
    if (!everyone) return;
    const t: Target = { id: guildId, type: 0, name: "@everyone", color: everyone.color };
    const allow = allowOf(guildId) & ~VIEW_CHANNEL;
    let deny = denyOf(guildId);
    deny = isPrivate ? deny & ~VIEW_CHANNEL : deny | VIEW_CHANNEL;
    void persist(t, allow, deny);
  }

  const groups = permGroups(channel.type);
  // On ne peut éditer que des permissions qu'on possède soi-même (le serveur l'impose aussi).
  const canEditBit = (bit: bigint) =>
    (actorPerms & PERM.ADMINISTRATOR) === PERM.ADMINISTRATOR || (actorPerms & bit) === bit;

  return (
    <div className="flex h-full flex-col overflow-y-auto px-10 py-12 scroll-thin">
      <div className="mx-auto w-full max-w-[860px]">
        <h2 className="text-xl font-bold text-header">Permissions du salon</h2>
        <p className="mt-1 text-sm text-muted">
          Utilise les permissions pour personnaliser qui peut faire quoi dans ce salon.
        </p>

        {/* Salon privé */}
        <div className="mt-6 flex items-start justify-between gap-4 rounded-lg bg-deepest/60 p-4 ring-1 ring-white/[0.04]">
          <div className="flex gap-3">
            <Lock size={18} className="mt-0.5 shrink-0 text-muted" />
            <div>
              <div className="text-sm font-semibold text-header">Salon privé</div>
              <p className="mt-0.5 text-sm text-muted">
                En définissant un salon comme « privé », seuls les rôles et membres sélectionnés pourront le voir.
              </p>
            </div>
          </div>
          <Switch on={isPrivate} onToggle={togglePrivate} />
        </div>

        {/* Synchronisation avec la catégorie parente (uniquement si le salon en a une). */}
        {channel.parent_id && (
          <div className="mt-3 flex items-start justify-between gap-4 rounded-lg bg-deepest/60 p-4 ring-1 ring-white/[0.04]">
            <div>
              <div className="text-sm font-semibold text-header">Synchroniser avec la catégorie</div>
              <p className="mt-0.5 text-sm text-muted">
                Remplace les permissions de ce salon par celles de sa catégorie.
              </p>
            </div>
            <button
              onClick={() => {
                void api
                  .syncChannelPermissions(channel.id)
                  .then(() => api.listOverwrites(channel.id).then(setOverwrites))
                  .catch(() => {});
              }}
              className="shrink-0 rounded-lg bg-field px-4 py-2 text-sm font-medium text-normal transition hover:bg-white/10"
            >
              Synchroniser
            </button>
          </div>
        )}

        <div className="my-6 h-px bg-white/5" />

        <div className="mb-3 text-sm font-bold uppercase tracking-wide text-subtext">Permissions avancées</div>

        {overwrites === null ? (
          <p className="text-sm text-muted">Chargement…</p>
        ) : (
          <div className="flex gap-6">
            {/* Colonne cibles */}
            <div className="w-[200px] shrink-0">
              <div className="mb-1 flex items-center justify-between px-1 text-[11px] font-bold uppercase tracking-wide text-muted">
                <span>Rôles / Membres</span>
                <AddTargetButton
                  roles={roles.filter((r) => !targets.some((t) => t.id === r.id))}
                  guildId={guildId}
                  members={members}
                  loadMembers={() => {
                    if (!members) api.listMembers(guildId).then(setMembers).catch(() => setMembers([]));
                  }}
                  existing={targets.map((t) => t.id)}
                  onAdd={(t) => {
                    setExtraTargets((cur) => (cur.some((x) => x.id === t.id) ? cur : [...cur, t]));
                    setSel(t.id);
                  }}
                />
              </div>
              <div className="space-y-0.5">
                {targets.map((t) => (
                  <button
                    key={t.id}
                    onClick={() => setSel(t.id)}
                    className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm ${
                      selTarget?.id === t.id ? "bg-selected text-header" : "text-channel hover:bg-hover hover:text-normal"
                    }`}
                  >
                    {t.type === 0 ? (
                      <span
                        className="h-2.5 w-2.5 shrink-0 rounded-full"
                        style={{ backgroundColor: t.color ? roleColorHex(t.color) : "#99aab5" }}
                      />
                    ) : (
                      <Avatar name={t.name} id={t.id} size={18} />
                    )}
                    <span className="min-w-0 flex-1 truncate">{t.name}</span>
                  </button>
                ))}
              </div>
            </div>

            {/* Colonne permissions */}
            <div className="min-w-0 flex-1">
              {selTarget &&
                groups.map((g) => (
                  <div key={g.label} className="mb-6">
                    <h3 className="mb-3 text-base font-semibold text-header">{g.label}</h3>
                    {g.perms.map((p) => {
                      const allowOn = (allowOf(selTarget.id) & p.bit) === p.bit;
                      const denyOn = (denyOf(selTarget.id) & p.bit) === p.bit;
                      const editable = canEditBit(p.bit);
                      return (
                        <div key={p.key} className="border-t border-white/5 py-3 first:border-t-0">
                          <div className="flex items-start justify-between gap-4">
                            <div className="min-w-0">
                              <div className="text-sm font-medium text-normal">{p.label}</div>
                              <p className="mt-0.5 text-xs text-muted">{p.desc}</p>
                            </div>
                            <TriState
                              allow={allowOn}
                              deny={denyOn}
                              disabled={!editable}
                              onSet={(state) => setState(selTarget, p.bit, state)}
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                ))}
            </div>
          </div>
        )}
        {error && <p className="mt-3 text-sm text-dnd">{error}</p>}
        <div className="h-4" />
      </div>
    </div>
  );
}

// Bouton tri-état ✕ / / / ✓ (refuser / neutre / autoriser).
function TriState({
  allow,
  deny,
  disabled,
  onSet,
}: {
  allow: boolean;
  deny: boolean;
  disabled?: boolean;
  onSet: (state: "allow" | "deny" | "neutral") => void;
}) {
  const neutral = !allow && !deny;
  return (
    <div
      className={`flex shrink-0 overflow-hidden rounded-md bg-deepest ring-1 ring-white/[0.06] ${
        disabled ? "opacity-40" : ""
      }`}
    >
      <Seg active={deny} activeCls="bg-dnd text-white" onClick={() => !disabled && onSet("deny")}>
        <X size={16} />
      </Seg>
      <Seg active={neutral} activeCls="bg-white/10 text-header" onClick={() => !disabled && onSet("neutral")}>
        <span className="text-base leading-none">/</span>
      </Seg>
      <Seg active={allow} activeCls="bg-online text-white" onClick={() => !disabled && onSet("allow")}>
        <Check size={16} />
      </Seg>
    </div>
  );
}

function Seg({
  children,
  active,
  activeCls,
  onClick,
}: {
  children: React.ReactNode;
  active: boolean;
  activeCls: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex h-8 w-10 items-center justify-center transition-colors ${
        active ? activeCls : "text-muted hover:bg-white/5 hover:text-normal"
      }`}
    >
      {children}
    </button>
  );
}

function AddTargetButton({
  roles,
  members,
  guildId,
  loadMembers,
  existing,
  onAdd,
}: {
  roles: Role[];
  members: Member[] | null;
  guildId: Snowflake;
  loadMembers: () => void;
  existing: Snowflake[];
  onAdd: (t: Target) => void;
}) {
  const [q, setQ] = useState("");
  const ql = q.toLowerCase().trim();
  const filteredRoles = roles.filter((r) => r.name.toLowerCase().includes(ql));
  const filteredMembers = (members ?? [])
    .filter((m) => !existing.includes(m.user.id))
    .filter((m) => (m.nick || displayName(m.user)).toLowerCase().includes(ql))
    .slice(0, 20);

  return (
    <Popover.Root onOpenChange={(o) => o && loadMembers()}>
      <Popover.Trigger
        className="flex h-5 w-5 items-center justify-center rounded text-muted outline-none hover:bg-white/10 hover:text-normal"
        title="Ajouter un rôle ou un membre"
      >
        <Plus size={14} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          className={`z-[70] max-h-[360px] w-[260px] overflow-hidden rounded-xl bg-floating p-2 shadow-pop ring-1 ring-line ${OVERLAY_ANIM}`}
        >
          <div className="mb-2 flex items-center gap-2 rounded bg-deepest px-2 py-1.5">
            <Search size={14} className="text-muted" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Rôle ou membre…"
              className="w-full bg-transparent text-sm text-normal outline-none placeholder:text-muted"
            />
          </div>
          <div className="max-h-[280px] overflow-y-auto scroll-thin">
            {filteredRoles.length > 0 && (
              <div className="px-1 pb-1 pt-1 text-[11px] font-bold uppercase tracking-wide text-muted">Rôles</div>
            )}
            {filteredRoles.map((r) => (
              <Popover.Close
                key={r.id}
                onClick={() => onAdd({ id: r.id, type: 0, name: r.id === guildId ? "@everyone" : r.name, color: r.color })}
                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
              >
                <span
                  className="h-2.5 w-2.5 shrink-0 rounded-full"
                  style={{ backgroundColor: r.color ? roleColorHex(r.color) : "#99aab5" }}
                />
                <span className="truncate">{r.id === guildId ? "@everyone" : r.name}</span>
              </Popover.Close>
            ))}
            {filteredMembers.length > 0 && (
              <div className="px-1 pb-1 pt-2 text-[11px] font-bold uppercase tracking-wide text-muted">Membres</div>
            )}
            {filteredMembers.map((m) => (
              <Popover.Close
                key={m.user.id}
                onClick={() => onAdd({ id: m.user.id, type: 1, name: m.nick || displayName(m.user) })}
                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
              >
                <Avatar name={m.nick || displayName(m.user)} id={m.user.id} size={20} avatarId={m.user.avatar_id} />
                <span className="truncate">{m.nick || displayName(m.user)}</span>
              </Popover.Close>
            ))}
            {filteredRoles.length === 0 && filteredMembers.length === 0 && (
              <p className="px-2 py-3 text-center text-xs text-muted">Aucun résultat.</p>
            )}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// ───────────────────────────── Primitives ─────────────────────────────

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      <label className="mb-2 block text-xs font-bold uppercase tracking-wide text-subtext">{label}</label>
      {children}
    </div>
  );
}
function Divider() {
  return <div className="my-6 h-px bg-white/5" />;
}
function Hint({ children }: { children: React.ReactNode }) {
  return <p className="mt-2 text-xs text-muted">{children}</p>;
}

function Select<T extends string | number>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { v: T; label: string }[];
  onChange: (v: T) => void;
}) {
  const current = options.find((o) => o.v === value);
  return (
    <Popover.Root>
      <Popover.Trigger className="flex w-full items-center justify-between rounded-lg bg-deepest px-3 py-2.5 text-left text-sm text-normal outline-none ring-1 ring-transparent hover:bg-white/5 data-[state=open]:ring-accent">
        <span>{current?.label ?? "—"}</span>
        <ChevronDown size={16} className="text-muted" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={4}
          className={`z-[70] max-h-[300px] w-[var(--radix-popover-trigger-width)] overflow-y-auto rounded-xl bg-floating p-1.5 shadow-pop ring-1 ring-line scroll-thin ${OVERLAY_ANIM}`}
        >
          {options.map((o) => (
            <Popover.Close
              key={String(o.v)}
              onClick={() => onChange(o.v)}
              className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-sm text-normal outline-none hover:bg-hover"
            >
              {o.label}
              {o.v === value && <Check size={15} className="text-accent" />}
            </Popover.Close>
          ))}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function Radio<T extends string | number>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { v: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="space-y-2">
      {options.map((o) => (
        <button
          key={String(o.v)}
          onClick={() => onChange(o.v)}
          className="flex w-full items-center gap-3 text-left"
        >
          <span
            className={`flex h-5 w-5 items-center justify-center rounded-full border-2 ${
              o.v === value ? "border-blurple" : "border-muted/50"
            }`}
          >
            {o.v === value && <span className="h-2.5 w-2.5 rounded-full bg-blurple" />}
          </span>
          <span className="text-sm text-normal">{o.label}</span>
        </button>
      ))}
    </div>
  );
}

function Toggle({
  label,
  desc,
  on,
  onToggle,
}: {
  label: string;
  desc: string;
  on: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="mb-5 flex items-start justify-between gap-4">
      <div>
        <div className="text-sm font-bold uppercase tracking-wide text-subtext">{label}</div>
        <p className="mt-1.5 max-w-[460px] text-sm text-muted">{desc}</p>
      </div>
      <Switch on={on} onToggle={onToggle} />
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
