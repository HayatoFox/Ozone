import { useCallback, useEffect, useRef, useState } from "react";
import { Bell, Headphones, KeyRound, LogOut, Mail, Palette, ShieldCheck, Trash2, Upload, UserRound, X } from "lucide-react";
import { api, ApiError } from "../api";
import { roleColorHex, useStore, type MessageDisplay, type Theme } from "../store";
import type { UserProfile } from "../types";
import { colorFor, displayName, initials } from "../lib/format";
import { Modal } from "./ServerRail";
import { ImageCropModal } from "./ImageCropModal";
import { VoiceVideoSection } from "./VoiceVideoSettings";
import { InstanceAdminSection } from "./InstanceAdminSection";
import { Spinner } from "./ui/Spinner";

type Section = "account" | "profile" | "voice" | "appearance" | "notifications" | "admin";

export function Settings({ onClose }: { onClose: () => void }) {
  const [section, setSection] = useState<Section>("account");
  // Fermeture différée : on joue l'animation de sortie avant de démonter (cf. Modal partagé).
  const [closing, setClosing] = useState(false);
  const requestClose = useCallback(() => setClosing(true), []);
  // Feature-détection : la section « Instance » n'apparaît que si le compte est admin d'instance.
  const [isInstanceAdmin, setIsInstanceAdmin] = useState(false);
  useEffect(() => {
    api
      .adminConfig()
      .then(() => setIsInstanceAdmin(true))
      .catch(() => {});
  }, []);

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
        className={`relative flex h-[85vh] max-h-[800px] w-[68vw] min-w-[760px] max-w-[1100px] overflow-hidden rounded-xl border border-line bg-modal shadow-2xl ${
          closing ? "animate-pop-out" : "animate-pop-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Colonne de navigation */}
        <div className="flex w-[220px] shrink-0 flex-col overflow-y-auto bg-modal-nav py-6 pl-4 pr-2 scroll-thin">
          <div className="mb-1 px-2.5 text-xs font-bold uppercase tracking-wide text-channel">
            Paramètres utilisateur
          </div>
          <NavItem icon={<UserRound size={16} />} active={section === "account"} onClick={() => setSection("account")}>
            Mon compte
          </NavItem>
          <NavItem icon={<IdIcon />} active={section === "profile"} onClick={() => setSection("profile")}>
            Profil
          </NavItem>
          <div className="my-2 h-px bg-line" />
          <div className="mb-1 px-2.5 text-xs font-bold uppercase tracking-wide text-channel">
            Paramètres de l'app
          </div>
          <NavItem icon={<Headphones size={16} />} active={section === "voice"} onClick={() => setSection("voice")}>
            Voix et vidéo
          </NavItem>
          <NavItem icon={<Palette size={16} />} active={section === "appearance"} onClick={() => setSection("appearance")}>
            Apparence
          </NavItem>
          <NavItem icon={<Bell size={16} />} active={section === "notifications"} onClick={() => setSection("notifications")}>
            Notifications
          </NavItem>
          {isInstanceAdmin && (
            <>
              <div className="my-2 h-px bg-line" />
              <div className="mb-1 px-2.5 text-xs font-bold uppercase tracking-wide text-channel">
                Instance
              </div>
              <NavItem icon={<ShieldCheck size={16} />} active={section === "admin"} onClick={() => setSection("admin")}>
                Administration
              </NavItem>
            </>
          )}
        </div>

        {/* Contenu — keyé sur la section pour rejouer une entrée douce à chaque changement. */}
        <div className="relative flex-1 overflow-y-auto px-8 py-8 scroll-thin">
          <div key={section} className="max-w-[680px] animate-msg-in pr-8">
            {section === "account" && <AccountSection />}
            {section === "profile" && <ProfileSection />}
            {section === "voice" && <VoiceVideoSection />}
            {section === "appearance" && <AppearanceSection />}
            {section === "notifications" && <NotificationsSection />}
            {section === "admin" && <InstanceAdminSection />}
          </div>
        </div>

        {/* Fermeture */}
        <button
          onClick={requestClose}
          title="Fermer (Échap)"
          className="pressable absolute right-4 top-4 flex h-8 w-8 items-center justify-center rounded-full text-interactive-normal transition hover:rotate-90 hover:bg-hover hover:text-interactive-hover"
        >
          <X size={18} />
        </button>
      </div>
    </div>
  );
}

function IdIcon() {
  // Petite icône « carte de profil » (Lucide n'a pas IdCard partout).
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <circle cx="9" cy="11" r="2" />
      <path d="M14 10h4M14 14h4M6.5 16a2.5 2.5 0 0 1 5 0" />
    </svg>
  );
}

function NavItem({
  icon,
  active,
  onClick,
  children,
}: {
  icon: React.ReactNode;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`pressable relative mb-0.5 flex w-full items-center gap-2 rounded px-2.5 py-1.5 text-left text-[15px] transition-colors duration-150 ${
        active
          ? "bg-selected text-interactive-active"
          : "text-channel hover:translate-x-0.5 hover:bg-hover hover:text-interactive-hover"
      }`}
    >
      {/* Liseré d'accent qui grandit sur l'item actif. */}
      <span
        className={`absolute left-0 top-1/2 w-1 -translate-y-1/2 rounded-r-full bg-accent transition-all duration-200 ${
          active ? "h-5 opacity-100" : "h-0 opacity-0"
        }`}
      />
      {icon}
      {children}
    </button>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="mb-5 text-xl font-bold text-header">{children}</h2>;
}

function SubTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="mb-2 text-xs font-bold uppercase tracking-wide text-subtext">{children}</h3>
  );
}

// ───────────────────────────── Compte ─────────────────────────────

function AccountSection() {
  const me = useStore((s) => s.me);
  const logout = useStore((s) => s.logout);
  const [modal, setModal] = useState<null | "password" | "email" | "delete">(null);
  if (!me) return null;

  return (
    <>
      <SectionTitle>Mon compte</SectionTitle>

      <div className="mb-6 rounded-lg bg-sidebar p-4">
        <ReadField label="Nom d'utilisateur" value={me.username} />
        {me.email && <ReadField label="E-mail" value={me.email} />}
      </div>

      <SubTitle>Mot de passe et sécurité</SubTitle>
      <div className="mb-6 flex flex-col gap-2">
        <button
          onClick={() => setModal("password")}
          className="flex items-center gap-2 self-start rounded-lg btn-accent px-4 py-2 text-sm font-medium text-white"
        >
          <KeyRound size={16} />
          Modifier le mot de passe
        </button>
        <button
          onClick={() => setModal("email")}
          className="flex items-center gap-2 self-start rounded bg-field px-4 py-2 text-sm font-medium text-normal hover:bg-hover"
        >
          <Mail size={16} />
          Modifier l'e-mail
        </button>
      </div>

      <div className="my-6 h-px bg-line" />

      <button
        onClick={logout}
        className="mr-3 inline-flex items-center gap-2 rounded bg-field px-4 py-2 text-sm font-medium text-normal hover:bg-hover"
      >
        <LogOut size={16} />
        Se déconnecter
      </button>
      <button
        onClick={() => setModal("delete")}
        className="inline-flex items-center gap-2 rounded border border-dnd px-4 py-2 text-sm font-medium text-dnd hover:bg-dnd hover:text-white"
      >
        <Trash2 size={16} />
        Supprimer le compte
      </button>

      {modal === "password" && <ChangePasswordModal onClose={() => setModal(null)} />}
      {modal === "email" && <ChangeEmailModal onClose={() => setModal(null)} />}
      {modal === "delete" && <DeleteAccountModal onClose={() => setModal(null)} />}
    </>
  );
}

function ReadField({ label, value }: { label: string; value: string }) {
  return (
    <div className="mb-1 last:mb-0">
      <div className="mb-1 text-xs font-bold uppercase tracking-wide text-subtext">{label}</div>
      <div className="rounded bg-deepest px-3 py-2 text-normal">{value}</div>
    </div>
  );
}

function ChangePasswordModal({ onClose }: { onClose: () => void }) {
  const logout = useStore((s) => s.logout);
  const [cur, setCur] = useState("");
  const [next, setNext] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true);
    setErr(null);
    try {
      await api.changePassword({ current_password: cur, new_password: next });
      // Le serveur révoque les sessions → on se déconnecte proprement.
      logout();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : "Échec.");
      setBusy(false);
    }
  }

  return (
    <AuthModal
      title="Modifier le mot de passe"
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      error={err}
      submitLabel="Changer"
      disabled={!cur || next.length < 1}
    >
      <PwField label="Mot de passe actuel" value={cur} onChange={setCur} autoFocus />
      <PwField label="Nouveau mot de passe" value={next} onChange={setNext} />
      <p className="text-xs text-muted">Tu seras déconnecté après le changement.</p>
    </AuthModal>
  );
}

function ChangeEmailModal({ onClose }: { onClose: () => void }) {
  const [pw, setPw] = useState("");
  const [email, setEmail] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true);
    setErr(null);
    try {
      await api.changeEmail({ password: pw, new_email: email });
      const me = await api.me();
      useStore.setState({ me });
      onClose();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : "Échec.");
      setBusy(false);
    }
  }

  return (
    <AuthModal
      title="Modifier l'e-mail"
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      error={err}
      submitLabel="Changer"
      disabled={!pw || !email}
    >
      <PwField label="Mot de passe" value={pw} onChange={setPw} autoFocus />
      <TextField label="Nouvel e-mail" type="email" value={email} onChange={setEmail} />
    </AuthModal>
  );
}

function DeleteAccountModal({ onClose }: { onClose: () => void }) {
  const logout = useStore((s) => s.logout);
  const [pw, setPw] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true);
    setErr(null);
    try {
      await api.deleteAccount({ password: pw });
      logout();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : "Échec.");
      setBusy(false);
    }
  }

  return (
    <AuthModal
      title="Supprimer le compte"
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      error={err}
      submitLabel="Supprimer définitivement"
      danger
      disabled={!pw}
    >
      <p className="text-sm text-muted">
        Cette action est <span className="font-semibold text-dnd">irréversible</span>. Confirme avec
        ton mot de passe.
      </p>
      <PwField label="Mot de passe" value={pw} onChange={setPw} autoFocus />
    </AuthModal>
  );
}

function AuthModal({
  title,
  children,
  onClose,
  onSubmit,
  busy,
  error,
  submitLabel,
  danger,
  disabled,
}: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  onSubmit: () => void | Promise<void>;
  busy: boolean;
  error: string | null;
  submitLabel: string;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <Modal onClose={onClose}>
      <div className="w-[440px] rounded-xl bg-modal p-6 ring-1 ring-cardline surface-card">
        <h2 className="mb-4 text-xl font-bold text-header">{title}</h2>
        <div className="space-y-3">{children}</div>
        {error && <p className="mt-3 text-sm text-dnd">{error}</p>}
        <div className="mt-5 flex justify-end gap-3">
          <button onClick={onClose} className="px-4 py-2 text-sm text-normal hover:underline">
            Annuler
          </button>
          <button
            onClick={() => void onSubmit()}
            disabled={busy || disabled}
            className={`pressable inline-flex items-center justify-center gap-2 rounded-lg px-5 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-50 ${
              danger ? "bg-dnd hover:opacity-90" : "btn-accent"
            }`}
          >
            {busy && <Spinner size={14} />}
            {submitLabel}
          </button>
        </div>
      </div>
    </Modal>
  );
}

function PwField(p: { label: string; value: string; onChange: (v: string) => void; autoFocus?: boolean }) {
  return <TextField {...p} type="password" />;
}

function TextField({
  label,
  value,
  onChange,
  type = "text",
  autoFocus,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  type?: string;
  autoFocus?: boolean;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">{label}</span>
      <input
        type={type}
        value={value}
        autoFocus={autoFocus}
        onChange={(e) => onChange(e.target.value)}
        className="w-full rounded-lg bg-deepest px-3 py-2 text-normal outline-none ring-1 ring-transparent focus:ring-accent"
      />
    </label>
  );
}

// ───────────────────────────── Profil ─────────────────────────────

const ACCENT_PRESETS = [
  0x6a5bff, 0x3ba55c, 0xfcb833, 0xda3e44, 0xeb459e, 0xe67e22, 0x3498db, 0x9b59b6, 0x1abc9c, 0x607d8b,
];

function ProfileSection() {
  const me = useStore((s) => s.me);
  const [name, setName] = useState(me?.display_name ?? "");
  const [pronouns, setPronouns] = useState("");
  const [bio, setBio] = useState("");
  const [accent, setAccent] = useState<number>(0x6a5bff);
  const [avatarId, setAvatarId] = useState<string | null>(null);
  const [bannerId, setBannerId] = useState<string | null>(null);
  // Aperçus locaux (object URLs) : l'image fraîchement rognée n'est servie par le serveur
  // qu'après « Enregistrer » — on prévisualise donc le blob local en attendant.
  const [avatarPreview, setAvatarPreview] = useState<string | null>(null);
  const [bannerPreview, setBannerPreview] = useState<string | null>(null);
  const [crop, setCrop] = useState<{ file: File; kind: "avatar" | "banner" } | null>(null);
  const avatarInput = useRef<HTMLInputElement>(null);
  const bannerInput = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Préremplit depuis le profil complet (bio/pronoms/accent ne sont pas dans `me`).
  // Tant que le préchargement n'a pas RÉUSSI, on n'autorise pas l'enregistrement : sinon
  // un échec réseau laisserait les champs à vide et un « Enregistrer » écraserait le profil.
  useEffect(() => {
    if (!me) return;
    let alive = true;
    api
      .userProfile(me.id)
      .then((p: UserProfile) => {
        if (!alive) return;
        setName(p.display_name ?? "");
        setPronouns(p.pronouns ?? "");
        setBio(p.bio ?? "");
        setAvatarId(p.avatar_id ?? null);
        setBannerId(p.banner_id ?? null);
        if (p.accent_color != null) setAccent(p.accent_color);
        setLoaded(true);
      })
      .catch(() => {
        if (alive) setError("Profil non chargé — réessaie avant d'enregistrer.");
      });
    return () => {
      alive = false;
    };
  }, [me]);

  if (!me) return null;

  async function save() {
    setBusy(true);
    setSaved(false);
    setError(null);
    try {
      const updated = await api.updateProfile({
        display_name: name.trim() || null,
        pronouns: pronouns.trim() || null,
        bio: bio.trim() || null,
        accent_color: accent,
        // Chaîne vide = effacer côté serveur.
        avatar_id: avatarId ?? "",
        banner_id: bannerId ?? "",
      });
      useStore.setState({ me: updated });
      setSaved(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Échec de l'enregistrement.");
    } finally {
      setBusy(false);
    }
  }

  async function uploadCropped(blob: Blob, kind: "avatar" | "banner") {
    setError(null);
    try {
      const f = new File([blob], `${kind}.webp`, { type: blob.type || "image/webp" });
      const { image_id } = await api.uploadUserImage(f);
      const url = URL.createObjectURL(blob);
      if (kind === "avatar") {
        setAvatarId(image_id);
        setAvatarPreview(url);
      } else {
        setBannerId(image_id);
        setBannerPreview(url);
      }
      setSaved(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Téléversement échoué.");
    }
  }

  // GIF animé : on saute le recadrage (le canvas aplatirait l'animation) → upload brut.
  function pickImage(f: File, kind: "avatar" | "banner") {
    if (f.type === "image/gif") void uploadCropped(f, kind);
    else setCrop({ file: f, kind });
  }

  const avatarUrl =
    avatarPreview ?? (avatarId && me ? `/api/users/${me.id}/avatar?v=${avatarId}` : null);
  const bannerUrl =
    bannerPreview ?? (bannerId && me ? `/api/users/${me.id}/banner?v=${bannerId}` : null);

  const shownName = name.trim() || displayName(me);
  const accentHex = roleColorHex(accent);

  return (
    <>
      <SectionTitle>Profil</SectionTitle>
      <div className="flex gap-8">
        {/* Formulaire */}
        <div className="flex-1 space-y-5">
          {/* Avatar */}
          <div>
            <span className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
              Avatar
            </span>
            <div className="flex items-center gap-4">
              <div
                className="flex h-20 w-20 shrink-0 items-center justify-center overflow-hidden rounded-full text-2xl font-semibold text-white"
                style={avatarUrl ? undefined : { backgroundColor: colorFor(me.id) }}
              >
                {avatarUrl ? (
                  <img src={avatarUrl} alt="" className="h-full w-full object-cover" />
                ) : (
                  initials(shownName)
                )}
              </div>
              <input
                ref={avatarInput}
                type="file"
                accept="image/png,image/jpeg,image/gif,image/webp"
                className="hidden"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) pickImage(f, "avatar");
                  if (avatarInput.current) avatarInput.current.value = "";
                }}
              />
              <button
                onClick={() => avatarInput.current?.click()}
                className="rounded-lg btn-accent px-4 py-2 text-sm font-semibold text-white"
              >
                Changer l'avatar
              </button>
              {avatarId && (
                <button
                  onClick={() => {
                    setAvatarId(null);
                    setAvatarPreview(null);
                    setSaved(false);
                  }}
                  className="text-sm font-medium text-dnd hover:underline"
                >
                  Supprimer
                </button>
              )}
            </div>
          </div>

          {/* Bannière de profil */}
          <div>
            <span className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
              Bannière de profil
            </span>
            <input
              ref={bannerInput}
              type="file"
              accept="image/png,image/jpeg,image/gif,image/webp"
              className="hidden"
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) pickImage(f, "banner");
                if (bannerInput.current) bannerInput.current.value = "";
              }}
            />
            <div className="flex items-center gap-3">
              <button
                onClick={() => bannerInput.current?.click()}
                className="flex items-center gap-2 rounded-lg bg-field px-4 py-2 text-sm font-medium text-normal transition hover:bg-white/10"
              >
                <Upload size={16} /> Importer une image
              </button>
              {bannerId && (
                <button
                  onClick={() => {
                    setBannerId(null);
                    setBannerPreview(null);
                    setSaved(false);
                  }}
                  className="text-sm font-medium text-dnd hover:underline"
                >
                  Retirer l'image
                </button>
              )}
            </div>
          </div>

          <TextField label="Nom d'affichage" value={name} onChange={(v) => { setName(v); setSaved(false); }} />
          <TextField label="Pronoms" value={pronouns} onChange={(v) => { setPronouns(v); setSaved(false); }} />
          <label className="block">
            <span className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
              À propos de moi
            </span>
            <textarea
              value={bio}
              onChange={(e) => { setBio(e.target.value); setSaved(false); }}
              rows={4}
              maxLength={300}
              placeholder="Parle un peu de toi…"
              className="w-full resize-none rounded bg-deepest px-3 py-2 text-normal outline-none focus:ring-1 focus:ring-accent"
            />
            <span className="mt-1 block text-right text-xs text-muted">{bio.length}/300</span>
          </label>
          <div>
            <span className="mb-1.5 block text-xs font-bold uppercase tracking-wide text-subtext">
              Couleur d'accent
            </span>
            <div className="flex flex-wrap gap-2">
              {ACCENT_PRESETS.map((c) => (
                <button
                  key={c}
                  onClick={() => { setAccent(c); setSaved(false); }}
                  className={`pressable h-8 w-8 rounded-full transition hover:scale-110 ${accent === c ? "ring-2 ring-white ring-offset-2 ring-offset-chat" : ""}`}
                  style={{ backgroundColor: roleColorHex(c) }}
                />
              ))}
            </div>
          </div>

          <div className="flex items-center gap-3">
            <button
              onClick={() => void save()}
              disabled={busy || !loaded}
              className="pressable inline-flex items-center justify-center gap-2 rounded-lg btn-accent px-5 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-60"
            >
              {busy && <Spinner size={14} />}
              Enregistrer
            </button>
            {saved && <span className="text-sm text-online">Profil mis à jour.</span>}
            {error && <span className="text-sm text-dnd">{error}</span>}
          </div>
        </div>

        {/* Aperçu live */}
        <div className="w-[300px] shrink-0">
          <span className="mb-2 block text-xs font-bold uppercase tracking-wide text-subtext">Aperçu</span>
          <div className="overflow-hidden rounded-xl bg-floating shadow-pop ring-1 ring-cardline">
            {bannerUrl ? (
              <img src={bannerUrl} alt="" className="h-[72px] w-full object-cover" />
            ) : (
              <div className="h-[72px]" style={{ background: `linear-gradient(150deg, ${accentHex}, #0c0c0e)` }} />
            )}
            <div className="px-4 pb-4">
              <div className="-mt-10 mb-2">
                <div
                  className="flex h-20 w-20 items-center justify-center overflow-hidden rounded-full border-[6px] border-floating text-2xl font-semibold text-white"
                  style={avatarUrl ? undefined : { backgroundColor: colorFor(me.id) }}
                >
                  {avatarUrl ? (
                    <img src={avatarUrl} alt="" className="h-full w-full object-cover" />
                  ) : (
                    initials(shownName)
                  )}
                </div>
              </div>
              <div className="rounded-lg bg-deepest p-3">
                <div className="text-lg font-bold text-header">{shownName}</div>
                <div className="text-sm text-muted">@{me.username}</div>
                {pronouns.trim() && <div className="mt-0.5 text-xs text-muted">{pronouns}</div>}
                {bio.trim() && (
                  <>
                    <div className="my-2 h-px bg-white/10" />
                    <div className="text-xs font-bold uppercase tracking-wide text-subtext">À propos</div>
                    <p className="mt-1 whitespace-pre-wrap break-words text-sm text-normal">{bio}</p>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      {crop && (
        <ImageCropModal
          file={crop.file}
          aspect={crop.kind === "avatar" ? 1 : 5 / 2}
          outWidth={crop.kind === "avatar" ? 512 : 850}
          outHeight={crop.kind === "avatar" ? 512 : 340}
          round={crop.kind === "avatar"}
          title={crop.kind === "avatar" ? "Recadrer l'avatar" : "Recadrer la bannière"}
          onCancel={() => setCrop(null)}
          onConfirm={(blob) => {
            void uploadCropped(blob, crop.kind);
            setCrop(null);
          }}
        />
      )}
    </>
  );
}

// ───────────────────────────── Apparence ─────────────────────────────

function AppearanceSection() {
  const theme = useStore((s) => s.theme);
  const setTheme = useStore((s) => s.setTheme);
  const customTheme = useStore((s) => s.customTheme);
  const display = useStore((s) => s.messageDisplay);
  const setDisplay = useStore((s) => s.setMessageDisplay);
  const mediaEmbeds = useStore((s) => s.mediaEmbeds);
  const setMediaEmbeds = useStore((s) => s.setMediaEmbeds);

  const customGrad = `linear-gradient(135deg, ${customTheme.gradient[0]}, ${customTheme.gradient[1]})`;
  const themes: { id: Theme; label: string; swatch?: string; gradient?: string }[] = [
    { id: "dark", label: "Sombre", swatch: "#161616" },
    { id: "light", label: "Clair", swatch: "#ffffff" },
    { id: "midnight", label: "Midnight", swatch: "#000000" },
    { id: "custom", label: "Perso", gradient: customGrad },
  ];
  const modes: { id: MessageDisplay; label: string; desc: string }[] = [
    { id: "cozy", label: "Cosy", desc: "Avatars et espacement aérés." },
    { id: "compact", label: "Compact", desc: "Une ligne par message, dense." },
  ];

  return (
    <>
      <SectionTitle>Apparence</SectionTitle>

      <SubTitle>Thème</SubTitle>
      <div className="mb-4 flex flex-wrap gap-3">
        {themes.map((t) => (
          <button
            key={t.id}
            onClick={() => setTheme(t.id)}
            className={`pressable flex items-center gap-2 rounded-lg border-2 px-4 py-3 transition-all hover:ring-1 hover:ring-accent/30 ${
              theme === t.id ? "border-accent" : "border-transparent hover:border-white/20"
            }`}
            style={t.gradient ? { backgroundImage: t.gradient } : { backgroundColor: t.swatch }}
          >
            <span className="h-5 w-5 rounded-full border border-black/20 bg-blurple" />
            <span className="text-sm font-medium" style={{ color: t.id === "light" ? "#060607" : "#fff" }}>
              {t.label}
            </span>
          </button>
        ))}
      </div>

      {theme === "custom" && <GradientBuilder />}

      <SubTitle>Affichage des messages</SubTitle>
      <div className="flex flex-col gap-2">
        {modes.map((m) => (
          <button
            key={m.id}
            onClick={() => setDisplay(m.id)}
            className={`pressable flex items-center justify-between rounded-lg border-2 bg-sidebar px-4 py-3 text-left transition-all hover:ring-1 hover:ring-accent/30 ${
              display === m.id ? "border-accent" : "border-transparent hover:border-white/10"
            }`}
          >
            <div>
              <div className="font-medium text-header">{m.label}</div>
              <div className="text-sm text-muted">{m.desc}</div>
            </div>
            <span
              className={`h-4 w-4 rounded-full border-2 transition-transform ${
                display === m.id ? "scale-100 border-accent bg-accent" : "scale-90 border-muted"
              }`}
            />
          </button>
        ))}
      </div>

      <h3 className="mb-2 mt-8 text-xs font-bold uppercase tracking-wide text-subtext">Aperçus média</h3>
      <label className="flex cursor-pointer items-start justify-between gap-4 rounded-lg bg-sidebar px-4 py-3 transition-colors hover:bg-hover">
        <span>
          <span className="block font-medium text-header">Afficher les liens média externes</span>
          <span className="block text-sm text-muted">
            Affiche images/vidéos des liens directs. Confidentialité : récupère le média depuis un
            serveur externe (expose votre adresse IP). Désactivé par défaut.
          </span>
        </span>
        <input
          type="checkbox"
          checked={mediaEmbeds}
          onChange={(e) => setMediaEmbeds(e.target.checked)}
          className="mt-1 h-4 w-4 shrink-0 accent-[color:var(--accent)]"
        />
      </label>
    </>
  );
}

// ───────────────────────────── Notifications ─────────────────────────────

function NotificationsSection() {
  const enabled = useStore((s) => s.desktopNotifications);
  const setEnabled = useStore((s) => s.setDesktopNotifications);
  const sounds = useStore((s) => s.notifSounds);
  const setSounds = useStore((s) => s.setNotifSounds);
  const perm = typeof Notification !== "undefined" ? Notification.permission : "unsupported";

  return (
    <>
      <SectionTitle>Notifications</SectionTitle>

      <SubTitle>Notifications bureau</SubTitle>
      <label className="mb-3 flex cursor-pointer items-start justify-between gap-4 rounded-lg bg-sidebar px-4 py-3 transition-colors hover:bg-hover">
        <span>
          <span className="block font-medium text-header">Activer les notifications bureau</span>
          <span className="block text-sm text-muted">
            Affiche une notification système pour les <span className="text-normal">mentions</span> et
            les <span className="text-normal">messages privés</span> quand la fenêtre est en arrière-plan.
          </span>
        </span>
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => void setEnabled(e.target.checked)}
          className="mt-1 h-4 w-4 shrink-0 accent-[color:var(--accent)]"
        />
      </label>

      {perm === "denied" && (
        <p className="mb-3 text-sm text-dnd">
          Le navigateur bloque les notifications pour ce site. Autorise-les dans les réglages du
          navigateur (cadenas dans la barre d'adresse) puis réactive ici.
        </p>
      )}
      {perm === "unsupported" && (
        <p className="mb-3 text-sm text-muted">Ce navigateur ne supporte pas les notifications.</p>
      )}

      <SubTitle>Sons</SubTitle>
      <label className="mb-3 flex cursor-pointer items-start justify-between gap-4 rounded-lg bg-sidebar px-4 py-3 transition-colors hover:bg-hover">
        <span>
          <span className="block font-medium text-header">Son de notification</span>
          <span className="block text-sm text-muted">
            Joue un bref signal sonore à la réception d'une mention ou d'un message privé.
          </span>
        </span>
        <input
          type="checkbox"
          checked={sounds}
          onChange={(e) => setSounds(e.target.checked)}
          className="mt-1 h-4 w-4 shrink-0 accent-[color:var(--accent)]"
        />
      </label>

      <SubTitle>Mettre en sourdine</SubTitle>
      <p className="rounded-lg bg-sidebar px-4 py-3 text-sm text-muted">
        Clique droit sur un <span className="text-normal">serveur</span> ou un{" "}
        <span className="text-normal">salon</span> pour le mettre en sourdine — les salons/serveurs
        en sourdine ne déclenchent pas de notification et n'affichent pas d'indicateur de non-lu.
      </p>
    </>
  );
}

// ───────────────────────────── Constructeur de dégradé ─────────────────────────────

const GRADIENTS: { name: string; from: string; to: string }[] = [
  { name: "Aurore", from: "#5865f2", to: "#c850c0" },
  { name: "Océan", from: "#0f2027", to: "#2c5364" },
  { name: "Crépuscule", from: "#355c7d", to: "#c06c84" },
  { name: "Forêt", from: "#0b486b", to: "#3b8686" },
  { name: "Braise", from: "#420516", to: "#a8324a" },
  { name: "Violet", from: "#41295a", to: "#2f0743" },
  { name: "Nuit", from: "#0f0c29", to: "#302b63" },
  { name: "Menthe", from: "#114357", to: "#3aa17e" },
];

function GradientBuilder() {
  const custom = useStore((s) => s.customTheme);
  const setCustom = useStore((s) => s.setCustomTheme);
  const isActive = (from: string, to: string) =>
    custom.gradient[0].toLowerCase() === from && custom.gradient[1].toLowerCase() === to;

  return (
    <div className="mb-8 rounded-lg bg-sidebar p-4">
      <div className="mb-2 text-xs font-bold uppercase tracking-wide text-subtext">Dégradés</div>
      <div className="mb-4 flex flex-wrap gap-2">
        {GRADIENTS.map((g) => (
          <button
            key={g.name}
            title={g.name}
            onClick={() => setCustom({ ...custom, gradient: [g.from, g.to] })}
            className={`pressable h-12 w-16 rounded-lg border-2 transition hover:scale-110 ${
              isActive(g.from, g.to) ? "border-white" : "border-transparent hover:border-white/30"
            }`}
            style={{ backgroundImage: `linear-gradient(135deg, ${g.from}, ${g.to})` }}
          />
        ))}
      </div>

      <div className="mb-4 flex flex-wrap gap-5">
        <ColorPick
          label="Début"
          value={custom.gradient[0]}
          onChange={(v) => setCustom({ ...custom, gradient: [v, custom.gradient[1]] })}
        />
        <ColorPick
          label="Fin"
          value={custom.gradient[1]}
          onChange={(v) => setCustom({ ...custom, gradient: [custom.gradient[0], v] })}
        />
      </div>

      <div className="mb-2 text-xs font-bold uppercase tracking-wide text-subtext">Couleur d'accent</div>
      <div className="flex flex-wrap gap-2">
        {ACCENT_PRESETS.map((c) => (
          <button
            key={c}
            onClick={() => setCustom({ ...custom, accent: c })}
            className={`pressable h-8 w-8 rounded-full transition hover:scale-110 ${
              custom.accent === c ? "ring-2 ring-white ring-offset-2 ring-offset-sidebar" : ""
            }`}
            style={{ backgroundColor: roleColorHex(c) }}
          />
        ))}
      </div>
    </div>
  );
}

function ColorPick({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <label className="flex items-center gap-2">
      <input
        type="color"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-9 w-9 cursor-pointer rounded border border-line bg-transparent"
      />
      <span className="text-sm text-normal">{label}</span>
    </label>
  );
}
