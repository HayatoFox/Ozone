// Page « Configuration de Sécurité » des paramètres de serveur : règles d'auto-modération.
// Mots filtrés (block/alert) et anti-spam de mentions, avec salon d'alerte optionnel.

import { useEffect, useState } from "react";
import { Plus, ShieldCheck, Trash2 } from "lucide-react";
import { api } from "../api";
import { useStore } from "../store";
import { CH_TEXT, type AutomodRule, type Snowflake } from "../types";
import { Spinner } from "./ui/Spinner";
import { ListSkeleton } from "./ui/Skeleton";
import { staggerDelay } from "../lib/anim";

function errText(e: unknown): string {
  return e instanceof Error ? e.message : "Échec.";
}

export function AutomodPage({ guildId }: { guildId: Snowflake }) {
  const [rules, setRules] = useState<AutomodRule[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const textChannels = useStore((s) =>
    (s.channelsByGuild[guildId] ?? []).filter((c) => c.type === CH_TEXT),
  );

  async function reload() {
    try {
      setRules(await api.listAutomodRules(guildId));
    } catch (e) {
      setError(errText(e));
    }
  }
  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl bg-modal ring-1 ring-cardline surface-card">
      <div className="flex items-center justify-between border-b border-line px-6 py-4">
        <div>
          <h2 className="flex items-center gap-2 text-xl font-bold text-header">
            <ShieldCheck size={20} className="text-online" /> Configuration de Sécurité
          </h2>
          <p className="mt-0.5 text-sm text-muted">
            Filtre automatiquement les messages selon des règles que tu définis.
          </p>
        </div>
        <button
          onClick={() => setCreating((v) => !v)}
          className="flex items-center gap-1.5 rounded-lg btn-accent px-3 py-1.5 text-sm font-medium text-white"
        >
          <Plus size={16} /> Nouvelle règle
        </button>
      </div>

      {creating && (
        <RuleForm
          guildId={guildId}
          channels={textChannels}
          onDone={async () => {
            setCreating(false);
            await reload();
          }}
        />
      )}

      <div className="flex-1 overflow-y-auto p-4 scroll-thin">
        {error ? (
          <p className="text-sm text-dnd">{error}</p>
        ) : rules === null ? (
          <ListSkeleton rows={4} />
        ) : rules.length === 0 ? (
          <p className="text-sm text-muted">
            Aucune règle. Crée-en une pour bloquer des mots ou limiter le spam de mentions.
          </p>
        ) : (
          <div className="flex flex-col gap-2">
            {rules.map((r, i) => (
              <div key={r.id} className="animate-row-in" style={staggerDelay(i)}>
                <RuleCard rule={r} guildId={guildId} onChanged={reload} />
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function RuleCard({
  rule,
  guildId,
  onChanged,
}: {
  rule: AutomodRule;
  guildId: Snowflake;
  onChanged: () => void | Promise<void>;
}) {
  async function toggle() {
    try {
      await api.updateAutomodRule(guildId, rule.id, { enabled: !rule.enabled });
      await onChanged();
    } catch {
      /* ignore */
    }
  }
  return (
    <div className="rounded-lg bg-sidebar p-3">
      <div className="flex items-center gap-3">
        <button
          onClick={() => void toggle()}
          role="switch"
          aria-checked={rule.enabled}
          title={rule.enabled ? "Désactiver" : "Activer"}
          className={`pressable flex h-5 w-9 shrink-0 items-center rounded-full px-0.5 transition-colors duration-200 ${
            rule.enabled ? "bg-online" : "bg-white/15"
          }`}
        >
          <span
            className={`h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] ${rule.enabled ? "translate-x-[18px]" : "translate-x-0.5"}`}
          />
        </button>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-semibold text-header">{rule.name}</div>
          <div className="text-xs text-muted">
            {rule.trigger_type === "keyword"
              ? `Mots filtrés (${rule.keywords.length}) · ${rule.action === "block" ? "bloque" : "alerte"}`
              : `Anti-spam : ≥ ${rule.mention_limit} mentions · ${rule.action === "block" ? "bloque" : "alerte"}`}
          </div>
        </div>
        <button
          onClick={() => void api.deleteAutomodRule(guildId, rule.id).then(onChanged).catch(() => {})}
          title="Supprimer"
          className="shrink-0 text-muted hover:text-dnd"
        >
          <Trash2 size={16} />
        </button>
      </div>
      {rule.trigger_type === "keyword" && rule.keywords.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1">
          {rule.keywords.map((k) => (
            <span key={k} className="rounded bg-deepest px-1.5 py-0.5 text-xs text-muted">
              {k}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function RuleForm({
  guildId,
  channels,
  onDone,
}: {
  guildId: Snowflake;
  channels: { id: Snowflake; name: string }[];
  onDone: () => void | Promise<void>;
}) {
  const [name, setName] = useState("");
  const [trigger, setTrigger] = useState<"keyword" | "mention_spam">("keyword");
  const [keywordsRaw, setKeywordsRaw] = useState("");
  const [mentionLimit, setMentionLimit] = useState(5);
  const [action, setAction] = useState<"block" | "alert">("block");
  const [alertChannel, setAlertChannel] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function submit() {
    if (name.trim().length < 1) {
      setErr("Nom requis.");
      return;
    }
    const keywords = keywordsRaw
      .split(",")
      .map((k) => k.trim())
      .filter(Boolean);
    if (trigger === "keyword" && keywords.length === 0) {
      setErr("Ajoute au moins un mot filtré (séparés par des virgules).");
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      await api.createAutomodRule(guildId, {
        name: name.trim(),
        trigger_type: trigger,
        keywords,
        mention_limit: mentionLimit,
        action,
        alert_channel_id: alertChannel || null,
      });
      await onDone();
    } catch (e) {
      setErr(errText(e));
      setBusy(false);
    }
  }

  return (
    <div className="animate-accordion space-y-3 border-b border-line bg-deepest/40 px-6 py-4">
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        maxLength={60}
        placeholder="Nom de la règle (ex. « Gros mots »)"
        className="w-full rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
      />
      <div className="flex gap-2">
        <select
          value={trigger}
          onChange={(e) => setTrigger(e.target.value as "keyword" | "mention_spam")}
          className="flex-1 rounded-lg bg-deepest px-3 py-2 text-sm text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        >
          <option value="keyword">Mots filtrés</option>
          <option value="mention_spam">Anti-spam de mentions</option>
        </select>
        <select
          value={action}
          onChange={(e) => setAction(e.target.value as "block" | "alert")}
          className="flex-1 rounded-lg bg-deepest px-3 py-2 text-sm text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        >
          <option value="block">Bloquer le message</option>
          <option value="alert">Alerter seulement</option>
        </select>
      </div>
      {trigger === "keyword" ? (
        <input
          value={keywordsRaw}
          onChange={(e) => setKeywordsRaw(e.target.value)}
          placeholder="Mots interdits, séparés par des virgules"
          className="w-full rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
        />
      ) : (
        <div className="flex items-center gap-2 text-sm text-muted">
          <span>Bloquer à partir de</span>
          <input
            type="number"
            min={1}
            max={50}
            value={mentionLimit}
            onChange={(e) => setMentionLimit(Math.max(1, Number(e.target.value) || 1))}
            className="w-20 rounded-lg bg-deepest px-3 py-2 text-center text-normal outline-none ring-1 ring-transparent focus:ring-accent"
          />
          <span>mentions dans un message</span>
        </div>
      )}
      <select
        value={alertChannel}
        onChange={(e) => setAlertChannel(e.target.value)}
        className="w-full rounded-lg bg-deepest px-3 py-2 text-sm text-normal outline-none ring-1 ring-transparent focus:ring-accent"
      >
        <option value="">Salon d'alerte (optionnel)</option>
        {channels.map((c) => (
          <option key={c.id} value={c.id}>
            # {c.name}
          </option>
        ))}
      </select>
      {err && <p className="text-sm text-dnd">{err}</p>}
      <div className="flex justify-end">
        <button
          onClick={() => void submit()}
          disabled={busy}
          className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-4 py-1.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-60"
        >
          {busy && <Spinner size={14} />}
          Créer la règle
        </button>
      </div>
    </div>
  );
}
