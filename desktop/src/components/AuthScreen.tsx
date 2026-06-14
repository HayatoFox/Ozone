import { useState } from "react";
import { ArrowLeft } from "lucide-react";
import { api, ApiError, setTokens } from "../api";
import { useStore } from "../store";
import type { InstanceInfo } from "../types";
import { Spinner } from "./ui/Spinner";

type Mode = "login" | "register";

export function AuthScreen({ instance }: { instance: InstanceInfo | null }) {
  const afterAuth = useStore((s) => s.afterAuth);
  const [mode, setMode] = useState<Mode>("login");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Champs
  const [login, setLogin] = useState("");
  const [username, setUsername] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayNameField, setDisplayNameField] = useState("");
  const [gatePassword, setGatePassword] = useState("");
  const [inviteCode, setInviteCode] = useState("");

  const gateRequired = instance?.access_gate.required ?? false;
  const policy = instance?.registration_policy ?? "open";
  const canRegister = policy !== "closed";

  async function obtainGateToken(): Promise<string | null> {
    if (!gateRequired) return null;
    const res = await api.gate(gatePassword);
    return res.gate_token;
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const gate_token = await obtainGateToken();
      const tokens =
        mode === "login"
          ? await api.login({ login, password, gate_token })
          : await api.register({
              username,
              email,
              password,
              display_name: displayNameField || null,
              gate_token,
              invite_code: policy === "invite" ? inviteCode || null : null,
            });
      setTokens(tokens);
      const me = await api.me();
      useStore.setState({ me, authed: true });
      await afterAuth();
    } catch (err) {
      if (err instanceof ApiError) setError(err.message);
      else setError("Connexion au serveur impossible.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="aurora-halo flex h-full w-full items-center justify-center bg-deepest">
      <div className="w-full max-w-md animate-pop-in rounded-2xl bg-chat p-8 shadow-2xl ring-1 ring-cardline surface-card">
        <div className="mb-6 text-center">
          <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-aurora text-lg font-bold text-white shadow-md ring-1 ring-white/20 [box-shadow:0_8px_24px_color-mix(in_srgb,var(--aurora-a)_45%,transparent)]">
            Oz
          </div>
          <h1 className="text-2xl font-bold text-header">
            {mode === "login" ? "Content de te revoir !" : "Créer un compte"}
          </h1>
          <p className="mt-1 text-sm text-muted">
            {instance?.name ?? "Ozone"}
            {instance?.description ? ` — ${instance.description}` : ""}
          </p>
        </div>

        <form onSubmit={submit} className="space-y-4">
          {mode === "login" ? (
            <Field
              label="E-mail ou nom d'utilisateur"
              value={login}
              onChange={setLogin}
              autoFocus
            />
          ) : (
            <>
              <Field label="Nom d'utilisateur" value={username} onChange={setUsername} autoFocus />
              <Field label="E-mail" type="email" value={email} onChange={setEmail} />
              <Field
                label="Nom d'affichage (optionnel)"
                value={displayNameField}
                onChange={setDisplayNameField}
                required={false}
              />
              {policy === "invite" && (
                <Field label="Code d'invitation d'instance" value={inviteCode} onChange={setInviteCode} />
              )}
            </>
          )}

          <Field label="Mot de passe" type="password" value={password} onChange={setPassword} />

          {gateRequired && (
            <Field
              label="Mot de passe de l'instance"
              type="password"
              value={gatePassword}
              onChange={setGatePassword}
            />
          )}

          {error && <p className="text-sm font-medium text-dnd">{error}</p>}

          <button
            type="submit"
            disabled={busy}
            className="pressable inline-flex w-full items-center justify-center gap-2 rounded-lg btn-accent py-2.5 font-medium text-white disabled:cursor-not-allowed disabled:opacity-60"
          >
            {busy && <Spinner size={16} />}
            {mode === "login" ? "Connexion" : "Continuer"}
          </button>

          {mode === "login" ? (
            canRegister && (
              <p className="text-sm text-muted">
                Besoin d'un compte ?{" "}
                <button
                  type="button"
                  className="text-link hover:underline"
                  onClick={() => {
                    setMode("register");
                    setError(null);
                  }}
                >
                  S'inscrire
                </button>
              </p>
            )
          ) : (
            <p className="text-sm text-muted">
              <button
                type="button"
                className="inline-flex items-center gap-1 text-link hover:underline"
                onClick={() => {
                  setMode("login");
                  setError(null);
                }}
              >
                <ArrowLeft size={14} />
                Retour à la connexion
              </button>
            </p>
          )}
        </form>
      </div>
    </div>
  );
}

function Field({
  label,
  value,
  onChange,
  type = "text",
  autoFocus = false,
  required = true,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  type?: string;
  autoFocus?: boolean;
  required?: boolean;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-muted">
        {label}
      </span>
      <input
        type={type}
        value={value}
        autoFocus={autoFocus}
        required={required}
        onChange={(e) => onChange(e.target.value)}
        className="w-full rounded-[3px] border border-line bg-deepest px-3 py-2.5 text-normal outline-none focus:border-blurple"
      />
    </label>
  );
}
