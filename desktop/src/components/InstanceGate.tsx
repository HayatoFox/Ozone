import { useState } from "react";
import { appFetch, setInstanceUrl } from "../lib/instance";
import { Spinner } from "./ui/Spinner";

// Écran de saisie de l'URL d'instance — affiché UNIQUEMENT dans le build empaqueté (.exe Tauri),
// où le front n'a pas d'origine API (cf. lib/instance.ts `needsInstanceUrl`). L'utilisateur saisit
// l'adresse de son serveur Ozone ; on la valide (ping de l'instance) puis on recharge l'app pour
// rebooter sur la bonne cible.
export function InstanceGate() {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function connect() {
    const raw = url.trim();
    if (!raw) return;
    // Normalise : ajoute https:// si aucun schéma fourni.
    const full = /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;
    const base = full.replace(/\/+$/, "");
    setBusy(true);
    setError(null);
    // Détecte le préfixe REST : déploiement standard (front + reverse-proxy → `/api`), ou API nue
    // servie directement (préfixe vide). On teste `/api/instance` puis `/instance` en repli.
    let prefix: string | null = null;
    let lastDetail = "";
    for (const p of ["/api", ""]) {
      try {
        const res = await appFetch(`${base}${p}/instance`, { method: "GET" });
        if (res.ok) {
          prefix = p;
          break;
        }
        lastDetail = `HTTP ${res.status} sur ${p || "/"}/instance`;
      } catch (e) {
        // Échec réseau / CSP / TLS : on conserve le détail pour le diagnostic.
        lastDetail = e instanceof Error ? `${e.name}: ${e.message}` : "échec réseau";
      }
    }
    if (prefix === null) {
      setError(
        `Impossible de joindre cette instance (${lastDetail || "aucune réponse"}). ` +
          "Vérifie l'adresse (ex. https://chat.exemple.fr).",
      );
      setBusy(false);
      return;
    }
    setInstanceUrl(base, prefix);
    location.reload(); // le boot repart en ciblant cette instance
  }

  return (
    <div className="aurora-halo flex h-full w-full flex-col items-center justify-center gap-6 bg-deepest px-6">
      <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-aurora text-xl font-bold text-white shadow-lg">
        Oz
      </div>
      <div className="w-full max-w-sm rounded-2xl bg-modal p-6 ring-1 ring-cardline surface-card">
        <h1 className="text-lg font-bold text-header">Connexion à une instance</h1>
        <p className="mt-1 text-sm text-muted">
          Entre l'adresse de ton serveur Ozone pour t'y connecter.
        </p>
        <input
          autoFocus
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void connect();
          }}
          placeholder="https://chat.exemple.fr"
          spellCheck={false}
          autoCapitalize="off"
          className="field-focus mt-4 w-full rounded-lg bg-deepest px-3 py-2.5 text-normal outline-none ring-1 ring-transparent placeholder:text-muted focus:ring-accent"
        />
        {error && <p className="mt-2 text-sm text-dnd">{error}</p>}
        <button
          onClick={() => void connect()}
          disabled={busy || !url.trim()}
          className="pressable mt-4 inline-flex w-full items-center justify-center gap-2 rounded-lg btn-accent py-2.5 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy && <Spinner size={16} />}
          Se connecter
        </button>
      </div>
    </div>
  );
}
