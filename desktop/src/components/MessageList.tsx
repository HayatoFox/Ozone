import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, Hash, Paperclip, Pencil, Reply, SmilePlus, Trash2 } from "lucide-react";
import {
  idGt,
  memberTopColorRole,
  memberTopRoleColor,
  roleColorHex,
  roleNameStyle,
  type RoleNameStyle,
  useStore,
} from "../store";
import { CH_GROUP, MSG_MEMBER_JOIN, type Attachment, type Message, type Snowflake } from "../types";
import { Avatar } from "./Avatar";
import { colorFor, displayName, formatDayTime, formatHM, initials } from "../lib/format";
import { renderMarkdown, type MentionCtx } from "../lib/markdown";
import { EmojiPicker } from "./ui/EmojiPicker";
import { MessageContextMenu } from "./ui/MessageContextMenu";
import { UserPopover } from "./ProfilePopout";
import { PollView } from "./PollView";
import { AuthedImage, authedDownload } from "./AuthedMedia";

function isGrouped(prev: Message | undefined, cur: Message): boolean {
  if (!prev) return false;
  if (prev.author.id !== cur.author.id) return false;
  if (cur.reference_id) return false; // une réponse démarre toujours un nouveau bloc
  return cur.created_at - prev.created_at < 7 * 60 * 1000;
}

export function MessageList({
  messages,
  channelId,
  guildId,
  onReply,
  channelName,
  dm,
}: {
  messages: Message[];
  channelId: Snowflake;
  guildId?: Snowflake;
  onReply: (m: Message) => void;
  channelName?: string;
  dm?: boolean;
}) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const initialScroll = useRef(true);
  const display = useStore((s) => s.messageDisplay);
  const me = useStore((s) => s.me);
  const roles = useStore((s) => (guildId ? s.rolesByGuild[guildId] : undefined));
  const members = useStore((s) => (guildId ? s.membersByGuild[guildId] : undefined));
  const channels = useStore((s) => (guildId ? s.channelsByGuild[guildId] : undefined));
  const anchor = useStore((s) => s.unreadAnchor[channelId]);
  const [editingId, setEditingId] = useState<Snowflake | null>(null);
  const [lightbox, setLightbox] = useState<string | null>(null);

  // Index du premier message non-lu (frontière capturée à l'ouverture).
  const dividerId = anchor
    ? messages.find((m) => idGt(m.id, anchor))?.id
    : undefined;

  // Auto-défilement : toujours à l'ouverture du salon, ensuite SEULEMENT si on est déjà
  // proche du bas — sinon on ne vole pas la position de lecture de l'historique.
  useEffect(() => {
    const el = scrollRef.current;
    const nearBottom =
      !el || el.scrollHeight - el.scrollTop - el.clientHeight < 150;
    if (initialScroll.current || nearBottom) {
      bottomRef.current?.scrollIntoView({ behavior: initialScroll.current ? "auto" : "smooth" });
      initialScroll.current = false;
    }
  }, [messages.length]);

  // Styles de nom (dégradé/néon/vague) par auteur — séparés des mentions (unies).
  const styleMap = useMemo(() => {
    const m = new Map<string, RoleNameStyle>();
    for (const mem of members ?? []) {
      const ns = roleNameStyle(memberTopColorRole(roles ?? [], mem));
      if (ns) m.set(mem.user.id, ns);
    }
    return m;
  }, [members, roles]);

  const ctx = useMemo<MentionCtx>(() => {
    const userMap = new Map<string, { name: string; color?: string }>();
    for (const m of members ?? []) {
      userMap.set(m.user.id, {
        name: m.nick || displayName(m.user),
        color: memberTopRoleColor(roles ?? [], m) ?? undefined,
      });
    }
    const roleMap = new Map<string, { name: string; color?: string }>();
    for (const r of roles ?? []) {
      roleMap.set(r.id, { name: r.name, color: r.color ? roleColorHex(r.color) : undefined });
    }
    const chanMap = new Map<string, { name: string }>();
    for (const c of channels ?? []) chanMap.set(c.id, { name: c.name });
    return {
      user: (id) => userMap.get(id),
      role: (id) => roleMap.get(id),
      channel: (id) => chanMap.get(id),
    };
  }, [members, roles, channels]);

  const styleOf = (authorId: Snowflake) => styleMap.get(authorId);

  const shared = {
    channelId,
    guildId,
    ctx,
    isMine: (m: Message) => m.author.id === me?.id,
    editingId,
    setEditingId,
    onReply,
    onImage: setLightbox,
  };

  return (
    <div ref={scrollRef} className="flex-1 animate-overlay-in overflow-y-auto scroll-thin">
      <div className="flex min-h-full flex-col justify-end py-4">
        <ChannelWelcome name={channelName} dm={dm} channelId={channelId} />
        {messages.map((m, i) => {
          const grouped = isGrouped(messages[i - 1], m);
          const nameStyle = styleOf(m.author.id);
          const newDay = i === 0 || !sameDay(messages[i - 1].created_at, m.created_at);
          // Messages système (type 7 = arrivée) : ligne dédiée, sans menu contextuel.
          if (m.type === MSG_MEMBER_JOIN) {
            return (
              <div key={m.id}>
                {newDay && <DateDivider ms={m.created_at} />}
                {dividerId === m.id && <UnreadDivider />}
                <SystemJoinRow message={m} ctx={ctx} />
              </div>
            );
          }
          const row =
            display === "compact" ? (
              <CompactMessage message={m} nameStyle={nameStyle} {...shared} />
            ) : grouped && !newDay ? (
              <GroupedMessage message={m} {...shared} />
            ) : (
              <FullMessage message={m} nameStyle={nameStyle} {...shared} />
            );
          // Seul le DERNIER message (le nouvel arrivant) joue l'entrée, pas toute la liste.
          const isLast = i === messages.length - 1;
          return (
            <div key={m.id} className={isLast ? "animate-msg-in" : undefined}>
              {newDay && <DateDivider ms={m.created_at} />}
              {dividerId === m.id && <UnreadDivider />}
              <MessageContextMenu
                message={m}
                channelId={channelId}
                mine={m.author.id === me?.id}
                onReply={onReply}
                onEdit={setEditingId}
              >
                {row}
              </MessageContextMenu>
            </div>
          );
        })}
        <div ref={bottomRef} />
      </div>

      {lightbox && (
        <div
          className="fixed inset-0 z-50 flex animate-overlay-in items-center justify-center bg-black/80 p-8"
          onClick={() => setLightbox(null)}
        >
          <AuthedImage src={lightbox} alt="" className="max-h-full max-w-full animate-pop-in rounded-md" />
        </div>
      )}
    </div>
  );
}

// Carte d'embed riche (barre latérale colorée + titre/desc/champs/image/footer).
function EmbedCard({
  embed,
  onImage,
}: {
  embed: import("../types").MessageEmbed;
  onImage: (src: string) => void;
}) {
  const accent = embed.color != null ? roleColorHex(embed.color) : "var(--accent)";
  return (
    <div
      className="max-w-[520px] overflow-hidden rounded-md bg-deepest/60 ring-1 ring-white/[0.04]"
      style={{ borderLeft: `4px solid ${accent}` }}
    >
      <div className="px-3 py-2.5">
        {embed.title &&
          (embed.url ? (
            <a
              href={embed.url}
              target="_blank"
              rel="noreferrer"
              className="text-sm font-semibold text-link hover:underline"
            >
              {embed.title}
            </a>
          ) : (
            <div className="text-sm font-semibold text-header">{embed.title}</div>
          ))}
        {embed.description && (
          <p className="mt-1 whitespace-pre-wrap break-words text-sm text-normal">
            {embed.description}
          </p>
        )}
        {embed.fields && embed.fields.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-x-4 gap-y-2">
            {embed.fields.map((f, i) => (
              <div
                key={i}
                className={f.inline ? "min-w-[150px] flex-1" : "w-full"}
              >
                <div className="text-xs font-semibold text-header">{f.name}</div>
                <div className="whitespace-pre-wrap break-words text-xs text-normal">{f.value}</div>
              </div>
            ))}
          </div>
        )}
        {embed.image_url && (
          <img
            src={embed.image_url}
            alt=""
            onClick={() => embed.image_url && onImage(embed.image_url)}
            className="mt-2 max-h-72 cursor-pointer rounded object-contain opacity-0 transition-opacity duration-300 ease-out hover:brightness-105"
            draggable={false}
            onLoad={(e) => e.currentTarget.classList.remove("opacity-0")}
          />
        )}
        {embed.footer && <div className="mt-2 text-[11px] text-muted">{embed.footer}</div>}
      </div>
    </div>
  );
}

// Ligne système « X a rejoint le serveur » (flèche verte, façon Discord).
function SystemJoinRow({ message, ctx }: { message: Message; ctx: MentionCtx }) {
  const name = ctx.user?.(message.author.id)?.name ?? displayName(message.author);
  return (
    <div className="group flex items-center gap-3 px-4 py-1.5 hover:bg-black/10">
      <div className="flex w-10 shrink-0 justify-center">
        <ArrowRight size={18} className="text-online" />
      </div>
      <div className="min-w-0 text-sm text-muted">
        <UserPopover userId={message.author.id}>
          <span className="cursor-pointer font-medium text-header hover:underline">{name}</span>
        </UserPopover>{" "}
        a rejoint le serveur.
        <span className="ml-2 text-xs text-muted opacity-0 group-hover:opacity-100">
          {formatDayTime(message.created_at)}
        </span>
      </div>
    </div>
  );
}

function UnreadDivider() {
  return (
    <div className="relative my-1 flex animate-msg-in items-center px-4">
      <div className="h-px flex-1 bg-dnd" />
      <span className="rounded-full bg-dnd px-2 py-0.5 text-[10px] font-bold uppercase text-white shadow-[0_0_12px_rgb(218_62_68/0.5)]">
        Nouveaux messages
      </span>
    </div>
  );
}

function sameDay(a: number, b: number): boolean {
  const d1 = new Date(a);
  const d2 = new Date(b);
  return d1.toDateString() === d2.toDateString();
}

function DateDivider({ ms }: { ms: number }) {
  const label = new Date(ms).toLocaleDateString("fr-FR", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  });
  return (
    <div className="my-3 flex items-center px-4">
      <div className="h-px flex-1 bg-line" />
      {/* Pastille centrée (plus douce qu'un simple texte sur la ligne). */}
      <span className="mx-2 rounded-full bg-field px-3 py-0.5 text-[11px] font-semibold text-muted ring-1 ring-white/[0.04]">
        {label}
      </span>
      <div className="h-px flex-1 bg-line" />
    </div>
  );
}

// En-tête de début de salon / conversation (façon « Bienvenue dans #salon »).
// En MP : avatar du correspondant (1:1) ou pastille de groupe, plutôt qu'un « # » générique.
function ChannelWelcome({ name, dm, channelId }: { name?: string; dm?: boolean; channelId: Snowflake }) {
  const me = useStore((s) => s.me);
  const dmChannel = useStore((s) => (dm ? s.dms.find((d) => d.id === channelId) : undefined));
  const others = dmChannel?.recipients.filter((u) => u.id !== me?.id) ?? [];
  const isGroup = dmChannel?.type === CH_GROUP || others.length > 1;
  const partner = others[0];

  return (
    <div className="px-4 pb-2 pt-4">
      <div className="mb-2">
        {dm ? (
          isGroup ? (
            <span
              className="flex h-[68px] w-[68px] items-center justify-center rounded-full text-2xl font-bold text-white"
              style={{ backgroundColor: colorFor(channelId) }}
            >
              {initials(name || "Groupe")}
            </span>
          ) : (
            <Avatar name={name ?? "?"} id={partner?.id ?? channelId} size={68} avatarId={partner?.avatar_id} />
          )
        ) : (
          <div className="flex h-[68px] w-[68px] items-center justify-center rounded-2xl bg-field ring-1 ring-line surface-card">
            <Hash size={38} style={{ color: "var(--aurora-a)" }} />
          </div>
        )}
      </div>
      <h2 className="text-3xl font-bold text-header">
        {dm ? name : `Bienvenue dans #${name ?? "salon"} !`}
      </h2>
      <p className="mt-1 text-base text-muted">
        {dm
          ? `Ceci est le tout début de ta conversation avec ${name ?? ""}.`
          : `C'est le tout début du salon #${name ?? "salon"}.`}
      </p>
    </div>
  );
}

interface RowProps {
  message: Message;
  nameStyle?: RoleNameStyle;
  channelId: Snowflake;
  guildId?: Snowflake;
  ctx: MentionCtx;
  isMine: (m: Message) => boolean;
  editingId: Snowflake | null;
  setEditingId: (id: Snowflake | null) => void;
  onReply: (m: Message) => void;
  onImage: (url: string) => void;
}

// ───────────────────────────── Cozy ─────────────────────────────

// Barre d'accent à gauche qui se révèle au survol (repère de ligne « façon Linear »).
const ROW_HOVER =
  "transition-colors before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:rounded-r before:bg-accent before:opacity-0 before:transition-opacity hover:before:opacity-60";

function FullMessage(p: RowProps) {
  const { message, nameStyle } = p;
  return (
    <div className={`group relative mx-2 mt-3 flex gap-4 rounded-lg px-2 py-0.5 hover:bg-message-hover ${ROW_HOVER}`}>
      <Toolbar {...p} />
      <UserPopover userId={message.author.id} guildId={p.guildId}>
        <Avatar
          name={displayName(message.author)}
          id={message.author.id}
          size={40}
          ring="var(--bg-chat)"
          avatarId={message.author.avatar_id}
        />
      </UserPopover>
      <div className="min-w-0 flex-1">
        <ReplyRef message={message} ctx={p.ctx} />
        <div className="flex items-baseline gap-2">
          <UserPopover userId={message.author.id} guildId={p.guildId}>
            <span
              className={`font-medium text-header hover:underline ${nameStyle?.className ?? ""}`}
              style={nameStyle?.style}
            >
              {displayName(message.author)}
            </span>
          </UserPopover>
          <span className="text-xs text-muted">{formatDayTime(message.created_at)}</span>
        </div>
        <Body {...p} />
      </div>
    </div>
  );
}

function GroupedMessage(p: RowProps) {
  const { message } = p;
  return (
    <div className={`group relative mx-2 flex gap-4 rounded-lg px-2 py-0.5 hover:bg-message-hover ${ROW_HOVER}`}>
      <Toolbar {...p} />
      <div className="flex w-10 shrink-0 items-center justify-end">
        <span className="text-[10px] text-muted opacity-0 group-hover:opacity-100">
          {formatHM(message.created_at)}
        </span>
      </div>
      <div className="min-w-0 flex-1">
        <Body {...p} />
      </div>
    </div>
  );
}

// ───────────────────────────── Compact ─────────────────────────────

function CompactMessage(p: RowProps) {
  const { message, nameStyle } = p;
  return (
    <div className={`group relative mx-2 flex gap-2 rounded-lg px-2 py-0.5 leading-relaxed hover:bg-message-hover ${ROW_HOVER}`}>
      <Toolbar {...p} />
      <span className="mt-0.5 shrink-0 select-none text-xs text-muted">{formatHM(message.created_at)}</span>
      <span
        className={`shrink-0 font-medium text-header ${nameStyle?.className ?? ""}`}
        style={nameStyle?.style}
      >
        {displayName(message.author)}
      </span>
      <div className="min-w-0 flex-1">
        <Body {...p} />
      </div>
    </div>
  );
}

// ───────────────────────────── Barre d'actions ─────────────────────────────

function Toolbar({ message, channelId, guildId, isMine, setEditingId, onReply }: RowProps) {
  const deleteMessage = useStore((s) => s.deleteMessage);
  const toggleReaction = useStore((s) => s.toggleReaction);
  const customEmojis = useStore((s) => (guildId ? s.emojisByGuild[guildId] : undefined));
  const mine = isMine(message);
  return (
    <div className="pointer-events-none absolute -top-3 right-3 flex translate-y-1 scale-95 overflow-hidden rounded-lg bg-floating opacity-0 shadow-pop ring-1 ring-white/[0.06] transition-all duration-150 ease-out group-hover:pointer-events-auto group-hover:translate-y-0 group-hover:scale-100 group-hover:opacity-100">
      <EmojiPicker
        custom={customEmojis}
        trigger={
          <button
            title="Réagir"
            className="pressable p-1.5 text-interactive-normal hover:bg-white/5 hover:text-interactive-hover"
          >
            <SmilePlus size={16} />
          </button>
        }
        onPick={(emoji) => void toggleReaction(channelId, message.id, emoji, false)}
      />
      <ToolBtn title="Répondre" onClick={() => onReply(message)}>
        <Reply size={16} />
      </ToolBtn>
      {mine && (
        <ToolBtn title="Modifier" onClick={() => setEditingId(message.id)}>
          <Pencil size={16} />
        </ToolBtn>
      )}
      {mine && (
        <ToolBtn title="Supprimer" danger onClick={() => void deleteMessage(channelId, message.id)}>
          <Trash2 size={16} />
        </ToolBtn>
      )}
    </div>
  );
}

function ToolBtn({
  title,
  onClick,
  children,
  danger,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
  danger?: boolean;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className={`pressable p-1.5 text-interactive-normal hover:bg-white/5 ${
        danger ? "hover:text-dnd" : "hover:text-interactive-hover"
      }`}
    >
      {children}
    </button>
  );
}

// ───────────────────────────── Référence de réponse ─────────────────────────────

function ReplyRef({ message, ctx }: { message: Message; ctx: MentionCtx }) {
  if (!message.referenced_message) return null;
  const ref = message.referenced_message;
  const name = ctx.user?.(ref.author.id)?.name ?? displayName(ref.author);
  return (
    <div className="mb-0.5 flex items-center gap-1 text-xs text-muted">
      <span className="ml-[-1.25rem] mr-1 h-2 w-3 rounded-tl border-l-2 border-t-2 border-white/15" />
      <span className="font-medium text-subtext">{name}</span>
      <span className="truncate opacity-80">{ref.content || "pièce jointe"}</span>
    </div>
  );
}

// ───────────────────────────── Corps ─────────────────────────────

function Body(p: RowProps) {
  const { message, ctx, channelId, editingId, setEditingId, onImage } = p;
  const editMessage = useStore((s) => s.editMessage);
  const toggleReaction = useStore((s) => s.toggleReaction);

  if (editingId === message.id) {
    return (
      <EditBox
        initial={message.content}
        onCancel={() => setEditingId(null)}
        onSave={async (content) => {
          if (content.trim() && content !== message.content) {
            await editMessage(channelId, message.id, content.trim());
          }
          setEditingId(null);
        }}
      />
    );
  }

  return (
    <div className="text-normal">
      {message.content && renderMarkdown(message.content, ctx)}
      {message.edited_at && <span className="ml-1 text-[10px] text-muted">(modifié)</span>}

      {message.poll && <PollView poll={message.poll} channelId={channelId} />}

      {message.sticker && (
        <img
          src={`/api/stickers/${message.sticker.id}`}
          alt={message.sticker.name}
          title={message.sticker.name}
          className="mt-1 h-40 w-40 rounded-lg object-contain opacity-0 transition-opacity duration-300 ease-out"
          draggable={false}
          onLoad={(e) => e.currentTarget.classList.remove("opacity-0")}
        />
      )}

      {message.content && <MediaEmbeds content={message.content} />}

      {message.attachments.length > 0 && (
        <div className="mt-1 flex flex-col gap-1">
          {message.attachments.map((a) => (
            <AttachmentView key={a.id} att={a} onImage={onImage} />
          ))}
        </div>
      )}

      {message.embeds && message.embeds.length > 0 && (
        <div className="mt-1 flex flex-col gap-1.5">
          {message.embeds.map((em, i) => (
            <EmbedCard key={i} embed={em} onImage={onImage} />
          ))}
        </div>
      )}

      {message.reactions.length > 0 && (
        <div className="mt-1 flex flex-wrap gap-1">
          {message.reactions.map((r) => (
            <button
              key={r.emoji}
              onClick={() => void toggleReaction(channelId, message.id, r.emoji, r.me)}
              className={`pressable flex animate-pop-in items-center gap-1 rounded-full px-2 py-0.5 text-xs ring-1 transition-colors hover:scale-105 ${
                r.me
                  ? "bg-selected text-header ring-accent/60"
                  : "bg-active ring-transparent hover:ring-line"
              }`}
            >
              <ReactionEmoji emoji={r.emoji} />
              {/* Le compteur « rebondit » à chaque changement (clé = count). */}
              <span key={r.count} className="inline-block animate-pop-in text-muted">{r.count}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// Embeds média : URL d'image/vidéo nues rendues inline (opt-in via réglage, OFF par défaut —
// la récupération d'un média externe expose l'IP du spectateur, cf. R11). http(s) uniquement.
const MEDIA_RE =
  /(https?:\/\/[^\s<]+?\.(?:png|jpe?g|gif|webp|bmp|avif|mp4|webm|mov))(?:\?[^\s<]*)?/gi;

function MediaEmbeds({ content }: { content: string }) {
  const on = useStore((s) => s.mediaEmbeds);
  if (!on) return null;
  const urls = [...new Set(content.match(MEDIA_RE) ?? [])].slice(0, 4);
  if (!urls.length) return null;
  return (
    <div className="mt-1 flex flex-col gap-1">
      {urls.map((u) =>
        /\.(?:mp4|webm|mov)(?:\?|$)/i.test(u) ? (
          <video key={u} src={u} controls className="max-h-80 max-w-md rounded-md" />
        ) : (
          <img key={u} src={u} alt="" loading="lazy" className="max-h-80 max-w-md rounded-md" />
        ),
      )}
    </div>
  );
}

// Rend l'emoji d'une réaction : image si c'est un emoji custom `<a?:nom:id>`, sinon texte.
function ReactionEmoji({ emoji }: { emoji: string }) {
  const m = /^<a?:\w+:(\d+)>$/.exec(emoji);
  if (m) {
    return <img src={`/api/emojis/${m[1]}`} alt="" className="inline-block h-4 w-4 object-contain" />;
  }
  return <span>{emoji}</span>;
}

function AttachmentView({ att, onImage }: { att: Attachment; onImage: (url: string) => void }) {
  const url = `/api${att.url}`;
  // Convention spoiler : nom de fichier préfixé « SPOILER_ » → flou jusqu'au clic.
  const isSpoiler = att.filename.startsWith("SPOILER_");
  const [revealed, setRevealed] = useState(!isSpoiler);
  const cleanName = isSpoiler ? att.filename.slice("SPOILER_".length) : att.filename;

  if (isSpoiler && !revealed) {
    return (
      <button
        onClick={() => setRevealed(true)}
        className="pressable group/sp relative flex h-32 w-52 items-center justify-center overflow-hidden rounded-md bg-deepest ring-1 ring-line transition-shadow hover:ring-accent/40"
      >
        <span className="rounded-full bg-black/70 px-3 py-1 text-xs font-bold uppercase tracking-wide text-white transition-transform group-hover/sp:scale-105">
          Spoiler — cliquer pour révéler
        </span>
      </button>
    );
  }
  if (att.content_type.startsWith("image/")) {
    return (
      <AuthedImage
        src={url}
        alt={cleanName}
        onClick={() => onImage(url)}
        className={`max-h-80 max-w-md cursor-pointer rounded-md transition hover:brightness-105 ${isSpoiler ? "animate-overlay-in" : ""}`}
      />
    );
  }
  return (
    <button
      onClick={() => void authedDownload(url, cleanName)}
      className="flex w-fit items-center gap-2 rounded-lg bg-active px-3 py-2 text-sm text-link ring-1 ring-line transition-colors hover:bg-selected"
    >
      <Paperclip size={16} className="shrink-0" />
      {cleanName}
    </button>
  );
}

function EditBox({
  initial,
  onSave,
  onCancel,
}: {
  initial: string;
  onSave: (content: string) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [text, setText] = useState(initial);
  return (
    <div className="mt-1">
      <textarea
        autoFocus
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void onSave(text);
          } else if (e.key === "Escape") {
            onCancel();
          }
        }}
        rows={1}
        className="w-full resize-none rounded-lg bg-field px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent/60"
      />
      <div className="mt-1 text-xs text-muted">
        échap pour <button onClick={onCancel} className="text-link hover:underline">annuler</button> · entrée
        pour sauvegarder
      </div>
    </div>
  );
}
