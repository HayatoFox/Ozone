import { Fragment, useState, type ReactNode } from "react";
import { formatHM } from "./format";
import { mediaUrl } from "./instance";

// Renderer Markdown « façon Discord » — sortie **React** (jamais de HTML brut ⇒ sûr).
// Couvre le sous-ensemble Discord : gras/italique/souligné/barré, code inline & blocs,
// spoilers, citations, titres, listes, liens masqués (URL validées), mentions, timestamps.
// Les emoji custom et le « jumbo » seront ajoutés en Phase 3.

export interface MentionCtx {
  user?: (id: string) => { name: string; color?: string } | undefined;
  role?: (id: string) => { name: string; color?: string } | undefined;
  channel?: (id: string) => { name: string } | undefined;
}

// ───────────────────────────── Inline ─────────────────────────────

interface InlineRule {
  re: RegExp;
  render: (m: RegExpExecArray, ctx: MentionCtx, key: number) => ReactNode;
  /** Le contenu interne est-il re-parsé ? (faux pour le code). */
  recurse?: boolean;
}

// Validation d'URL : uniquement http(s) (bloque javascript:, data:…).
function safeUrl(url: string): string | null {
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:" ? url : null;
  } catch {
    return null;
  }
}

const RULES: InlineRule[] = [
  // Code inline (contenu non parsé) — priorité maximale.
  {
    re: /`([^`\n]+?)`/,
    render: (m, _c, k) => (
      <code key={k} className="rounded bg-black/30 px-1 py-0.5 font-mono text-[85%] text-normal">
        {m[1]}
      </code>
    ),
  },
  // Spoiler ||texte||
  {
    re: /\|\|([\s\S]+?)\|\|/,
    recurse: true,
    render: (m, c, k) => <Spoiler key={k}>{parseInline(m[1], c)}</Spoiler>,
  },
  // Gras-italique ***texte***
  {
    re: /\*\*\*([\s\S]+?)\*\*\*/,
    recurse: true,
    render: (m, c, k) => (
      <strong key={k} className="font-semibold">
        <em className="italic">{parseInline(m[1], c)}</em>
      </strong>
    ),
  },
  // Gras **texte**
  {
    re: /\*\*([\s\S]+?)\*\*/,
    recurse: true,
    render: (m, c, k) => (
      <strong key={k} className="font-semibold">
        {parseInline(m[1], c)}
      </strong>
    ),
  },
  // Souligné __texte__ (⚠️ Discord : __ = souligné, pas gras)
  {
    re: /__([\s\S]+?)__/,
    recurse: true,
    render: (m, c, k) => (
      <u key={k} className="underline">
        {parseInline(m[1], c)}
      </u>
    ),
  },
  // Barré ~~texte~~
  {
    re: /~~([\s\S]+?)~~/,
    recurse: true,
    render: (m, c, k) => (
      <s key={k} className="line-through">
        {parseInline(m[1], c)}
      </s>
    ),
  },
  // Italique *texte*
  {
    re: /\*([^*\n]+?)\*/,
    recurse: true,
    render: (m, c, k) => (
      <em key={k} className="italic">
        {parseInline(m[1], c)}
      </em>
    ),
  },
  // Italique _texte_ (évite les underscores en milieu de mot)
  {
    re: /(^|[^A-Za-z0-9_])_([^_\n]+?)_(?![A-Za-z0-9])/,
    recurse: true,
    render: (m, c, k) => (
      <Fragment key={k}>
        {m[1]}
        <em className="italic">{parseInline(m[2], c)}</em>
      </Fragment>
    ),
  },
  // Lien masqué [texte](url)
  {
    re: /\[([^\]\n]+)\]\(([^\s)]+)\)/,
    render: (m, c, k) => {
      const url = safeUrl(m[2]);
      if (!url) return <Fragment key={k}>{m[0]}</Fragment>;
      return (
        <a key={k} href={url} target="_blank" rel="noreferrer" className="text-link hover:underline">
          {parseInline(m[1], c)}
        </a>
      );
    },
  },
  // Mention utilisateur <@id> / <@!id>
  {
    re: /<@!?(\d+)>/,
    render: (m, c, k) => {
      const u = c.user?.(m[1]);
      return <Mention key={k}>@{u?.name ?? "inconnu"}</Mention>;
    },
  },
  // Mention rôle <@&id>
  {
    re: /<@&(\d+)>/,
    render: (m, c, k) => {
      const r = c.role?.(m[1]);
      return (
        <Mention key={k} color={r?.color}>
          @{r?.name ?? "rôle"}
        </Mention>
      );
    },
  },
  // Mention salon <#id>
  {
    re: /<#(\d+)>/,
    render: (m, c, k) => {
      const ch = c.channel?.(m[1]);
      return <Mention key={k}>#{ch?.name ?? "salon"}</Mention>;
    },
  },
  // @everyone / @here
  {
    re: /@(everyone|here)\b/,
    render: (m, _c, k) => <Mention key={k}>@{m[1]}</Mention>,
  },
  // Emoji custom <:nom:id> / <a:nom:id> → image servie par /api/emojis/:id
  {
    re: /<(a)?:(\w+):(\d+)>/,
    render: (m, _c, k) => (
      <img
        key={k}
        src={mediaUrl(`/api/emojis/${m[3]}`)}
        alt={`:${m[2]}:`}
        title={`:${m[2]}:`}
        loading="lazy"
        className="inline-block h-[1.375em] w-auto align-[-0.3em]"
      />
    ),
  },
  // Timestamp <t:unix:style>
  {
    re: /<t:(\d+)(?::([tTdDfFR]))?>/,
    render: (m, _c, k) => (
      <span key={k} className="rounded bg-black/20 px-1 text-normal" title="Horodatage">
        {formatTimestamp(Number(m[1]) * 1000, m[2])}
      </span>
    ),
  },
  // Lien nu http(s)://…
  {
    re: /https?:\/\/[^\s<]+/,
    render: (m, _c, k) => {
      const url = safeUrl(m[0]);
      if (!url) return <Fragment key={k}>{m[0]}</Fragment>;
      return (
        <a key={k} href={url} target="_blank" rel="noreferrer" className="text-link hover:underline">
          {m[0]}
        </a>
      );
    },
  },
];

export function parseInline(text: string, ctx: MentionCtx): ReactNode[] {
  const out: ReactNode[] = [];
  let rest = text;
  let k = 0; // clés locales (stables d'un rendu à l'autre pour un même contenu)
  while (rest.length) {
    let best: { rule: InlineRule; m: RegExpExecArray } | null = null;
    for (const rule of RULES) {
      const m = rule.re.exec(rest);
      if (m && (best === null || m.index < best.m.index)) {
        best = { rule, m };
        if (m.index === 0) break; // rien ne peut commencer plus tôt
      }
    }
    if (!best) {
      out.push(<Fragment key={k++}>{rest}</Fragment>);
      break;
    }
    if (best.m.index > 0) out.push(<Fragment key={k++}>{rest.slice(0, best.m.index)}</Fragment>);
    out.push(best.rule.render(best.m, ctx, k++));
    rest = rest.slice(best.m.index + best.m[0].length);
  }
  return out;
}

// ───────────────────────────── Blocs ─────────────────────────────

// Message composé uniquement d'emoji (unicode et/ou custom, peu nombreux) ⇒ rendu « jumbo ».
const EMOJI_ONLY =
  /^(?:\s|\p{Extended_Pictographic}️?|\p{Regional_Indicator}|[\u{1F3FB}-\u{1F3FF}‍])+$/u;
const CUSTOM_EMOJI_RE = /<a?:\w+:\d+>/g;
function isJumbo(content: string): boolean {
  const customCount = (content.match(CUSTOM_EMOJI_RE) ?? []).length;
  // Retire les tokens custom : le reste doit être vide ou de l'emoji unicode pur.
  const t = content.replace(CUSTOM_EMOJI_RE, "").trim();
  if (t && !EMOJI_ONLY.test(t)) return false;
  const unicodeCount = [...t.replace(/\s/g, "")].length;
  const total = unicodeCount + customCount;
  return total > 0 && total <= 12;
}

export function renderMarkdown(content: string, ctx: MentionCtx): ReactNode {
  if (isJumbo(content)) {
    return <div className="text-4xl leading-tight">{parseInline(content, ctx)}</div>;
  }
  const lines = content.split("\n");
  const blocks: ReactNode[] = [];
  let i = 0;
  let bk = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Bloc de code ```lang … ```
    const fence = /^```(\w+)?\s*$/.exec(line);
    if (fence) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) code.push(lines[i++]);
      if (i < lines.length) i++; // fermeture
      blocks.push(<CodeBlock key={bk++} code={code.join("\n")} lang={fence[1]} />);
      continue;
    }

    // Citation multi-ligne >>> (jusqu'à la fin)
    if (/^>>> /.test(line)) {
      const quoted = [line.slice(4), ...lines.slice(i + 1)].join("\n");
      blocks.push(<Quote key={bk++}>{renderMarkdown(quoted, ctx)}</Quote>);
      break;
    }
    // Citation > (lignes consécutives)
    if (/^> /.test(line)) {
      const q: string[] = [];
      while (i < lines.length && /^> /.test(lines[i])) q.push(lines[i++].slice(2));
      blocks.push(<Quote key={bk++}>{renderMarkdown(q.join("\n"), ctx)}</Quote>);
      continue;
    }

    // Titres # / ## / ###
    const h = /^(#{1,3}) (.+)$/.exec(line);
    if (h) {
      const level = h[1].length;
      const cls =
        level === 1 ? "text-2xl font-bold" : level === 2 ? "text-xl font-bold" : "text-lg font-semibold";
      blocks.push(
        <div key={bk++} className={`${cls} text-header my-1`}>
          {parseInline(h[2], ctx)}
        </div>,
      );
      i++;
      continue;
    }

    // Sous-texte -# …
    const sub = /^-# (.+)$/.exec(line);
    if (sub) {
      blocks.push(
        <div key={bk++} className="text-xs text-muted">
          {parseInline(sub[1], ctx)}
        </div>,
      );
      i++;
      continue;
    }

    // Listes (- / * / +) et ordonnées (1.)
    if (/^\s*[-*+] /.test(line) || /^\s*\d+\. /.test(line)) {
      const items: ReactNode[] = [];
      const ordered = /^\s*\d+\. /.test(line);
      while (i < lines.length && (/^\s*[-*+] /.test(lines[i]) || /^\s*\d+\. /.test(lines[i]))) {
        const text = lines[i].replace(/^\s*(?:[-*+]|\d+\.) /, "");
        items.push(<li key={items.length}>{parseInline(text, ctx)}</li>);
        i++;
      }
      blocks.push(
        ordered ? (
          <ol key={bk++} className="ml-5 list-decimal space-y-0.5">
            {items}
          </ol>
        ) : (
          <ul key={bk++} className="ml-5 list-disc space-y-0.5">
            {items}
          </ul>
        ),
      );
      continue;
    }

    // Paragraphe : regroupe les lignes jusqu'à une ligne « spéciale » ou vide.
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^```/.test(lines[i]) &&
      !/^>{1,3} /.test(lines[i]) &&
      !/^#{1,3} /.test(lines[i]) &&
      !/^-# /.test(lines[i]) &&
      !/^\s*[-*+] /.test(lines[i]) &&
      !/^\s*\d+\. /.test(lines[i])
    ) {
      para.push(lines[i++]);
    }
    if (para.length) {
      blocks.push(
        <p key={bk++} className="whitespace-pre-wrap break-words">
          {parseInline(para.join("\n"), ctx)}
        </p>,
      );
    } else {
      i++; // ligne vide
    }
  }

  return <div className="flex flex-col gap-1">{blocks}</div>;
}

// ───────────────────────────── Sous-composants ─────────────────────────────

function Spoiler({ children }: { children: ReactNode }) {
  const [revealed, setRevealed] = useState(false);
  return (
    <span
      onClick={() => setRevealed(true)}
      className={`cursor-pointer rounded ${
        revealed ? "bg-black/30" : "bg-black/60 text-transparent select-none"
      }`}
    >
      {children}
    </span>
  );
}

function Mention({ children, color }: { children: ReactNode; color?: string }) {
  return (
    <span
      className="rounded bg-selected px-1 font-medium text-link hover:bg-accent/40"
      style={color ? { color, backgroundColor: `${color}22` } : undefined}
    >
      {children}
    </span>
  );
}

function Quote({ children }: { children: ReactNode }) {
  return (
    <div className="flex gap-2">
      <div className="w-1 shrink-0 rounded-full bg-[#4e5058]" />
      <div className="text-normal">{children}</div>
    </div>
  );
}

function CodeBlock({ code, lang }: { code: string; lang?: string }) {
  return (
    <pre className="my-0.5 overflow-x-auto rounded-md border border-line bg-[#2b2d31] p-2 scroll-thin">
      {lang && <div className="mb-1 text-[10px] uppercase text-muted">{lang}</div>}
      <code className="font-mono text-[88%] text-[#dbdee1]">{code}</code>
    </pre>
  );
}

// ───────────────────────────── Timestamps ─────────────────────────────

function formatTimestamp(ms: number, style?: string): string {
  const d = new Date(ms);
  switch (style) {
    case "t":
      return formatHM(ms);
    case "T":
      return d.toLocaleTimeString("fr-FR");
    case "d":
      return d.toLocaleDateString("fr-FR");
    case "D":
      return d.toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" });
    case "F":
      return d.toLocaleString("fr-FR", {
        weekday: "long",
        day: "numeric",
        month: "long",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    case "R":
      return relativeTime(ms);
    case "f":
    default:
      return d.toLocaleString("fr-FR", {
        day: "numeric",
        month: "long",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
  }
}

function relativeTime(ms: number): string {
  const diff = ms - Date.now();
  const abs = Math.abs(diff);
  const units: [number, Intl.RelativeTimeFormatUnit][] = [
    [60_000, "minute"],
    [3_600_000, "hour"],
    [86_400_000, "day"],
    [2_592_000_000, "month"],
    [31_536_000_000, "year"],
  ];
  const rtf = new Intl.RelativeTimeFormat("fr-FR", { numeric: "auto" });
  let divisor = 1000;
  let unit: Intl.RelativeTimeFormatUnit = "second";
  for (const [threshold, u] of units) {
    if (abs < threshold) break;
    divisor = threshold;
    unit = u;
  }
  return rtf.format(Math.round(diff / divisor), unit);
}
