// Client média WebRTC contre le nœud SFU Ozone.
// Connexion initiale : getUserMedia → RTCPeerConnection → offre (ICE complet, non-trickle)
// → POST /sfu/rooms/:channel/peers {sdp, token} → réponse SDP.
// Ensuite : un **WebSocket** persistant (/sfu/rooms/:channel/peers/:peer/signal) porte la
// **renégociation** (negotiation parfaite, client « poli ») : activer/désactiver la caméra ou le
// partage d'écran, et recevoir en direct les pistes des autres — **sans recharger le flux**.
// En cas d'échec de la signalisation, l'appelant retombe sur une reconnexion complète (repli).

// Connexion vocale ultra-rapide : **pas de STUN** → on n'attend pas un aller-retour vers un serveur
// externe ; les candidats **hôtes** (localhost / LAN) suffisent et sont disponibles quasi
// instantanément → join en ≤1 s. (Compromis assumé : pas de traversée NAT inter-réseaux ; à
// rétablir via STUN/TURN si déploiement hors LAN.)
const ICE_SERVERS: RTCIceServer[] = [];

// Cible de débit audio : Opus mono ~64 kb/s = qualité voix élevée à faible latence.
const OPUS_BITRATE = 64_000;
const VIDEO_BITRATE = 2_500_000;

// Contraintes micro/caméra : construites à CHAQUE acquisition depuis les préférences
// utilisateur (périphérique choisi + traitement du son) — cf. lib/mediaPrefs.ts.
import { applySink, audioConstraints, videoConstraints } from "./mediaPrefs";
import { httpBase, sfuWsBase } from "./instance";

// Règle l'encodeur Opus dans le SDP (offre/réponse) : FEC en bande, DTX, mono, débit cible.
export function tuneOpus(sdp: string): string {
  const m = sdp.match(/a=rtpmap:(\d+) opus\/48000/i);
  if (!m) return sdp;
  const pt = m[1];
  const params = `minptime=10;useinbandfec=1;usedtx=1;stereo=0;sprop-stereo=0;maxaveragebitrate=${OPUS_BITRATE};maxplaybackrate=48000`;
  const fmtpRe = new RegExp(`a=fmtp:${pt} [^\\r\\n]*`);
  if (fmtpRe.test(sdp)) return sdp.replace(fmtpRe, `a=fmtp:${pt} ${params}`);
  return sdp.replace(
    new RegExp(`(a=rtpmap:${pt} opus/48000[^\\r\\n]*\\r?\\n)`),
    `$1a=fmtp:${pt} ${params}\r\n`,
  );
}

// Attend la fin du rassemblement ICE (le SFU n'accepte pas le trickle). Résout immédiatement si
// déjà complet (renégociation : transport déjà établi). Sans STUN, les candidats hôtes sont prêts
// quasi instantanément → on plafonne l'attente à 600 ms pour garantir un join ≤1 s même si l'événement
// « complete » tarde. On résout aussi dès le **premier candidat hôte** + court délai de groupage.
function gatheringComplete(pc: RTCPeerConnection): Promise<void> {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    let settled = false;
    let guard: ReturnType<typeof setTimeout>;
    let grace: ReturnType<typeof setTimeout> | undefined;
    const finish = () => {
      if (settled) return;
      settled = true;
      clearTimeout(guard);
      if (grace) clearTimeout(grace);
      pc.removeEventListener("icegatheringstatechange", onChange);
      pc.removeEventListener("icecandidate", onCand);
      resolve();
    };
    const onChange = () => {
      if (pc.iceGatheringState === "complete") finish();
    };
    const onCand = (e: RTCPeerConnectionIceEvent) => {
      // Dès qu'on a au moins un candidat utilisable, un court délai suffit à grouper les autres
      // candidats hôtes (LAN) → on n'attend pas l'événement « complete » (parfois lent).
      if (e.candidate && !grace) grace = setTimeout(finish, 150);
    };
    pc.addEventListener("icegatheringstatechange", onChange);
    pc.addEventListener("icecandidate", onCand);
    guard = setTimeout(finish, 600); // garde-fou : join rapide garanti
  });
}

// Une piste vidéo distante, attribuée à son propriétaire (uid imposé par le SFU) et à sa nature.
export interface RemoteVideo {
  trackId: string;
  userId: string;
  kind: "cam" | "screen";
  stream: MediaStream;
}

export interface VoiceCallbacks {
  onVideoTrack?: (v: RemoteVideo) => void;
  onVideoEnded?: (trackId: string) => void;
  onSpeaking?: (userId: string, speaking: boolean) => void;
  /** La signalisation/renégociation a échoué → l'appelant doit reconnecter complètement (repli). */
  onNeedsReconnect?: () => void;
}

export interface ConnectOptions {
  withVideo?: boolean;
  selfId?: string;
  /** Flux de partage d'écran déjà acquis (getDisplayMedia) — publié en piste « screen ». */
  screen?: MediaStream | null;
}

// Détection de parole côté client : un AnalyserNode par flux audio (local + distants, étiquetés
// par uid), RMS échantillonné, seuil + hangover → émet les transitions « parle / se tait ».
class SpeakingDetector {
  private ctx: AudioContext | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private tick = 0;
  // Seuil RMS de détection (configurable). En mode auto, calibré en continu sur le bruit de fond.
  private threshold = 0.045;
  private auto = false;
  private entries = new Map<
    string,
    {
      analyser: AnalyserNode;
      src: MediaStreamAudioSourceNode;
      data: Uint8Array<ArrayBuffer>;
      on: boolean;
      last: number;
      noiseFloor: number; // plancher de bruit estimé (mode auto)
    }
  >();
  constructor(private cb: (userId: string, speaking: boolean) => void) {}

  /** Règle la sensibilité : `auto` calibre le seuil sur le bruit de fond, sinon seuil RMS fixe. */
  setSensitivity(opts: { auto: boolean; threshold: number }): void {
    this.auto = opts.auto;
    this.threshold = Math.max(0.005, Math.min(0.5, opts.threshold));
  }

  add(userId: string, stream: MediaStream): void {
    if (!userId || this.entries.has(userId) || stream.getAudioTracks().length === 0) return;
    if (!this.ctx) this.ctx = new AudioContext();
    void this.ctx.resume().catch(() => {});
    if (!this.timer) this.timer = setInterval(() => this.poll(), 120);
    const analyser = this.ctx.createAnalyser();
    analyser.fftSize = 512;
    const src = this.ctx.createMediaStreamSource(stream);
    src.connect(analyser); // tap passif (non relié à la sortie → pas de double son)
    const data = new Uint8Array(new ArrayBuffer(analyser.fftSize));
    this.entries.set(userId, { analyser, src, data, on: false, last: 0, noiseFloor: 0.01 });
  }

  remove(userId: string): void {
    const e = this.entries.get(userId);
    if (!e) return;
    try {
      e.src.disconnect();
    } catch {
      /* ignore */
    }
    if (e.on) this.cb(userId, false);
    this.entries.delete(userId);
  }

  private poll(): void {
    this.tick += 1;
    const now = this.tick * 120;
    for (const [uid, e] of this.entries) {
      e.analyser.getByteTimeDomainData(e.data);
      let sum = 0;
      for (let i = 0; i < e.data.length; i += 1) {
        const v = (e.data[i] - 128) / 128;
        sum += v * v;
      }
      const rms = Math.sqrt(sum / e.data.length);
      // Seuil effectif : fixe, ou « plancher de bruit + marge » en mode auto.
      let thr = this.threshold;
      if (this.auto) {
        // Le plancher suit le bruit : vite quand on se tait (0.05), lentement même en parlant
        // (0.004). Cette remontée lente est ESSENTIELLE : sans elle, un bruit de fond stationnaire
        // au-dessus du seuil maintiendrait e.on=true pour toujours et figerait le plancher bas
        // (gate verrouillé ouvert, micro transmis en continu). Le facteur lent évite que la
        // parole soutenue ne fasse grimper le seuil au point de se couper soi-même.
        e.noiseFloor += (rms - e.noiseFloor) * (e.on ? 0.004 : 0.05);
        thr = Math.max(0.02, e.noiseFloor * 2.4 + 0.012);
      }
      if (rms > thr) e.last = now; // au-dessus du seuil de parole
      const on = now - e.last < 350; // hangover : évite le clignotement
      if (on !== e.on) {
        e.on = on;
        this.cb(uid, on);
      }
    }
  }

  close(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
    this.entries.forEach((e) => {
      try {
        e.src.disconnect();
      } catch {
        /* ignore */
      }
    });
    this.entries.clear();
    void this.ctx?.close().catch(() => {});
    this.ctx = null;
  }
}

// Le SFU étiquette chaque flux relayé « <uid>.<kind> ». On en extrait l'attribution.
type StreamKind = "cam" | "screen" | "mic" | "screen_audio";
function parseStreamTag(streamId: string): { userId: string; kind: StreamKind } {
  const [userId, raw] = streamId.split(".");
  const kind: StreamKind =
    raw === "screen" || raw === "mic" || raw === "screen_audio" ? raw : "cam";
  return { userId: userId ?? "", kind };
}

export class VoiceConnection {
  private pc: RTCPeerConnection | null = null;
  private local: MediaStream | null = null;
  private peerId: string | null = null;
  private channelId = "";
  private token = "";
  private audioEls = new Map<string, HTMLAudioElement>();
  // Volume local par participant (0..1) + éléments audio rattachés (pour réglage en direct).
  private userVolumes = new Map<string, number>();
  private elsByUser = new Map<string, Set<HTMLAudioElement>>();
  // Volume du SON DU STREAM par participant (distinct du micro) + éléments audio rattachés.
  private streamVolumes = new Map<string, number>();
  private streamElsByUser = new Map<string, Set<HTMLAudioElement>>();
  private deafened = false;
  private cb: VoiceCallbacks;
  private speaking: SpeakingDetector;

  // Renégociation (negotiation parfaite, client poli).
  private ws: WebSocket | null = null;
  private manifest: Record<string, string> = {};
  private negReady = false; // ignore `negotiationneeded` pendant la connexion initiale
  private closing = false;
  private failed = false;
  // Pistes vidéo locales (pour (dés)activation sans reconnexion).
  private camTrack: MediaStreamTrack | null = null;
  private camSender: RTCRtpSender | null = null;
  // Clone non gaté du micro, dédié à la détection de voix (cf. connect()).
  private micMonitor: MediaStream | null = null;
  private screenStream: MediaStream | null = null;
  private screenSenders: RTCRtpSender[] = [];
  // Sender de la piste VIDÉO du partage (pour remplacer la source/qualité sans renégociation).
  private screenVideoSender: RTCRtpSender | null = null;
  // Mixeur audio sortant : micro → AudioContext → piste publiée. Permet d'y mélanger le
  // soundboard (BufferSource) sans seconde piste ni renégociation. Repli : micro brut si
  // l'AudioContext est indisponible.
  private mixCtx: AudioContext | null = null;
  private mixDest: MediaStreamAudioDestinationNode | null = null;
  private mixedTrack: MediaStreamTrack | null = null;
  private soundCache = new Map<string, AudioBuffer>();

  constructor(cb: VoiceCallbacks = {}) {
    this.cb = cb;
    this.speaking = new SpeakingDetector((uid, on) => this.cb.onSpeaking?.(uid, on));
  }

  get localStream(): MediaStream | null {
    return this.local;
  }

  /** La signalisation de renégociation est-elle saine (WS ouvert) ? */
  signalingHealthy(): boolean {
    return !!this.ws && this.ws.readyState === WebSocket.OPEN;
  }

  /** Ouvre le micro (+ caméra), publie un éventuel partage d'écran, négocie avec le SFU. */
  async connect(channelId: string, token: string, opts: ConnectOptions = {}): Promise<void> {
    const { withVideo = false, selfId = "", screen = null } = opts;
    this.channelId = channelId;
    this.token = token;
    this.manifest = {};
    this.local = await navigator.mediaDevices.getUserMedia({
      audio: audioConstraints(),
      video: withVideo ? videoConstraints() : false,
    });
    // Détection de voix sur un CLONE non gaté de la piste micro : le gate VAD coupe `enabled` sur
    // la piste publiée (this.local), mais le moniteur garde son propre flux actif, sinon le gate
    // s'auto-verrouillerait (silence → jamais de réouverture).
    if (selfId) {
      const micTrack0 = this.local.getAudioTracks()[0] ?? null;
      if (micTrack0) {
        this.micMonitor = new MediaStream([micTrack0.clone()]);
        this.speaking.add(selfId, this.micMonitor);
      }
    }

    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS, bundlePolicy: "max-bundle" });
    this.pc = pc;
    // Publie le micro VIA le mixeur (la sourdine coupe la piste micro brute en amont, donc le
    // soundboard continue de passer) ; à défaut de mixeur, le micro brut directement.
    const micTrack = this.local.getAudioTracks()[0] ?? null;
    this.buildMixer(micTrack);
    const outTrack = this.mixedTrack ?? micTrack;
    if (outTrack) {
      pc.addTrack(outTrack, this.local);
      this.manifest[outTrack.id] = "mic";
    }
    for (const t of this.local.getVideoTracks()) {
      this.camTrack = t;
      this.camSender = pc.addTrack(t, this.local);
      this.manifest[t.id] = "cam";
    }
    if (screen) {
      this.screenStream = screen;
      for (const t of screen.getVideoTracks()) {
        const sender = pc.addTrack(t, screen);
        this.screenSenders.push(sender);
        this.screenVideoSender = sender; // sinon le hot-swap de qualité serait cassé après rejoin
        this.manifest[t.id] = "screen";
      }
      // Publie AUSSI l'audio de la source (son de l'écran/fenêtre) — sinon il serait perdu à
      // chaque reconnexion/resync qui repasse le partage par connect() au lieu de addScreen().
      for (const t of screen.getAudioTracks()) {
        this.screenSenders.push(pc.addTrack(t, screen));
        this.manifest[t.id] = "screen_audio";
      }
    }

    pc.ontrack = (e) => this.onRemoteTrack(e);
    // Renégociation initiée par le client (ajout/retrait de piste). Ignorée tant que la connexion
    // initiale n'est pas établie (`negReady`).
    pc.onnegotiationneeded = () => void this.onNegotiationNeeded();

    const offerSdp = await this.createMungedOffer();
    this.tuneSenders();

    const resp = await fetch(`${httpBase()}/sfu/rooms/${channelId}/peers`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sdp: offerSdp, token, tracks: this.manifest }),
    });
    if (!resp.ok) {
      const detail = await resp.text().catch(() => "");
      throw new Error(`SFU ${resp.status} ${detail}`.trim());
    }
    const ans = (await resp.json()) as { peer_id: string; sdp: string };
    this.peerId = ans.peer_id;
    await pc.setRemoteDescription({ type: "answer", sdp: ans.sdp });

    // Connexion établie : active la renégociation et ouvre le canal de signalisation.
    this.negReady = true;
    this.openSignaling();
  }

  private onRemoteTrack(e: RTCTrackEvent): void {
    const track = e.track;
    const stream = e.streams[0] ?? new MediaStream([track]);
    if (track.kind === "audio") {
      try {
        (e.receiver as unknown as { playoutDelayHint?: number }).playoutDelayHint = 0;
      } catch {
        /* non supporté */
      }
      const { userId, kind } = parseStreamTag(e.streams[0]?.id ?? "");
      const isStreamAudio = kind === "screen_audio";
      const el = document.createElement("audio");
      el.autoplay = true;
      el.muted = this.deafened;
      // Le son du stream a son propre volume (réglable/coupable par le spectateur), distinct du micro.
      el.volume = isStreamAudio
        ? this.streamVolumes.get(userId) ?? 1
        : this.userVolumes.get(userId) ?? 1;
      el.srcObject = stream;
      applySink(el); // route vers la sortie audio choisie dans les réglages
      this.audioEls.set(track.id, el);
      if (userId) {
        const map = isStreamAudio ? this.streamElsByUser : this.elsByUser;
        const set = map.get(userId) ?? new Set();
        set.add(el);
        map.set(userId, set);
      }
      document.body.appendChild(el);
      void el.play().catch(() => {});
      // Seul le micro alimente la détection de parole (pas le son d'un jeu partagé).
      if (!isStreamAudio) this.speaking.add(userId, stream);
      track.onended = () => {
        el.srcObject = null;
        el.remove();
        this.audioEls.delete(track.id);
        (isStreamAudio ? this.streamElsByUser : this.elsByUser).get(userId)?.delete(el);
        if (!isStreamAudio) this.speaking.remove(userId);
      };
    } else {
      const { userId, kind } = parseStreamTag(e.streams[0]?.id ?? "");
      // Une piste vidéo ne porte jamais un tag audio (mic/screen_audio) : on restreint à cam/screen.
      const videoKind: "cam" | "screen" = kind === "screen" ? "screen" : "cam";
      this.cb.onVideoTrack?.({ trackId: track.id, userId, kind: videoKind, stream });
      track.onended = () => this.cb.onVideoEnded?.(track.id);
    }
  }

  // ───────────────────────────── Signalisation / renégociation ─────────────────────────────

  private openSignaling(): void {
    // Base WS du SFU : origine (web), :8081 direct (dev Vite), ou instance configurée (.exe).
    const base = sfuWsBase();
    const url = `${base}/sfu/rooms/${this.channelId}/peers/${this.peerId}/signal?token=${encodeURIComponent(this.token)}`;
    let ws: WebSocket;
    try {
      ws = new WebSocket(url);
    } catch {
      return; // pas de renégociation → l'appelant utilisera le repli au besoin
    }
    this.ws = ws;
    ws.onmessage = (e) => void this.handleSignal(typeof e.data === "string" ? e.data : "");
    ws.onclose = () => {
      if (!this.closing && this.ws === ws) this.fail();
    };
    ws.onerror = () => {
      /* onclose suit ; le repli est déclenché là */
    };
  }

  private wsSend(obj: unknown): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(obj));
  }

  private async createMungedOffer(): Promise<string> {
    const pc = this.pc!;
    const offer = await pc.createOffer();
    await pc.setLocalDescription({ type: "offer", sdp: tuneOpus(offer.sdp ?? "") });
    await gatheringComplete(pc);
    return pc.localDescription?.sdp ?? offer.sdp ?? "";
  }

  private async createMungedAnswer(): Promise<string> {
    const pc = this.pc!;
    const answer = await pc.createAnswer();
    await pc.setLocalDescription({ type: "answer", sdp: tuneOpus(answer.sdp ?? "") });
    await gatheringComplete(pc);
    return pc.localDescription?.sdp ?? answer.sdp ?? "";
  }

  private async onNegotiationNeeded(): Promise<void> {
    if (!this.negReady || this.closing || !this.pc) return;
    // Le client n'offre que s'il est en état stable (évite de bousculer une négociation serveur
    // en cours ; il ré-offrira au besoin quand `negotiationneeded` se redéclenchera).
    if (this.pc.signalingState !== "stable") return;
    try {
      const sdp = await this.createMungedOffer();
      this.wsSend({ t: "offer", sdp, tracks: { ...this.manifest } });
    } catch {
      this.fail();
    }
  }

  private async handleSignal(text: string): Promise<void> {
    const pc = this.pc;
    if (!pc || this.closing) return;
    let msg: { t?: string; sdp?: string };
    try {
      msg = JSON.parse(text);
    } catch {
      return;
    }
    try {
      if (msg.t === "offer") {
        // Negotiation parfaite : le client est poli → en cas de collision il accepte l'offre
        // serveur (rollback implicite de Chrome dans l'état have-local-offer) puis répond.
        await pc.setRemoteDescription({ type: "offer", sdp: msg.sdp });
        const sdp = await this.createMungedAnswer();
        this.wsSend({ t: "answer", sdp });
      } else if (msg.t === "answer") {
        if (pc.signalingState === "have-local-offer") {
          await pc.setRemoteDescription({ type: "answer", sdp: msg.sdp });
        }
      }
    } catch {
      this.fail();
    }
  }

  private fail(): void {
    if (this.closing || this.failed) return;
    this.failed = true;
    this.cb.onNeedsReconnect?.();
  }

  // ───────────────────────────── (Dés)activation caméra / écran ─────────────────────────────

  /** Active la caméra **sans reconnexion** (ajout de piste + renégociation). Renvoie le flux local. */
  async enableCamera(): Promise<MediaStream> {
    if (!this.pc || !this.local) throw new Error("vocal non connecté");
    if (!this.signalingHealthy()) throw new Error("signalisation indisponible");
    if (this.camTrack) return this.local;
    const vs = await navigator.mediaDevices.getUserMedia({ video: videoConstraints() });
    const vt = vs.getVideoTracks()[0];
    if (!vt) throw new Error("aucune caméra");
    this.camTrack = vt;
    this.local.addTrack(vt);
    this.manifest[vt.id] = "cam";
    this.camSender = this.pc.addTrack(vt, this.local); // → onnegotiationneeded
    this.tuneSenders();
    return this.local;
  }

  /** Désactive la caméra **sans reconnexion** (retrait de piste + renégociation + unpublish). */
  async disableCamera(): Promise<void> {
    if (!this.pc) return;
    if (this.camSender) {
      try {
        this.pc.removeTrack(this.camSender);
      } catch {
        /* ignore */
      }
      this.camSender = null;
    }
    if (this.camTrack) {
      const id = this.camTrack.id;
      this.local?.removeTrack(this.camTrack);
      this.camTrack.stop();
      delete this.manifest[id];
      this.camTrack = null;
      this.wsSend({ t: "unpublish", id }); // retire la copie relayée chez les autres
    }
  }

  /**
   * Publie un partage d'écran (flux fourni par l'appelant) **sans reconnexion**. Publie la piste
   * vidéo (`screen`) et, si le flux porte de l'audio (son de la fenêtre/écran), la piste audio
   * (`screen_audio`) — distincte du micro pour que les spectateurs la règlent séparément.
   */
  async addScreen(stream: MediaStream): Promise<void> {
    if (!this.pc) throw new Error("vocal non connecté");
    if (!this.signalingHealthy()) throw new Error("signalisation indisponible");
    this.screenStream = stream;
    for (const t of stream.getVideoTracks()) {
      this.manifest[t.id] = "screen";
      const sender = this.pc.addTrack(t, stream);
      this.screenSenders.push(sender);
      this.screenVideoSender = sender;
    }
    for (const t of stream.getAudioTracks()) {
      this.manifest[t.id] = "screen_audio";
      this.screenSenders.push(this.pc.addTrack(t, stream));
    }
    this.tuneSenders();
  }

  /**
   * Remplace la piste vidéo du partage **sans renégociation** (`sender.replaceTrack`) : sert au
   * hot-swap de qualité/fps (nouvelle capture aux nouvelles contraintes) ou de source (autre
   * fenêtre/écran). L'appelant fournit la nouvelle piste vidéo et stoppe l'ancienne.
   * Retourne false si le remplacement direct est impossible (l'appelant fera alors un cycle
   * remove/add, qui renégocie).
   */
  async replaceScreenVideo(newTrack: MediaStreamTrack): Promise<boolean> {
    if (!this.pc || !this.screenVideoSender || !this.screenStream) return false;
    const old = this.screenVideoSender.track;
    try {
      await this.screenVideoSender.replaceTrack(newTrack);
    } catch {
      return false;
    }
    // Met à jour le manifeste + le flux local (le tag relayé reste « screen »).
    if (old) {
      delete this.manifest[old.id];
      this.screenStream.removeTrack(old);
      old.stop();
    }
    this.manifest[newTrack.id] = "screen";
    this.screenStream.addTrack(newTrack);
    this.tuneSenders();
    return true;
  }

  /** Le partage d'écran porte-t-il actuellement une piste audio (son de la source) ? */
  hasScreenAudio(): boolean {
    return !!this.screenStream && this.screenStream.getAudioTracks().length > 0;
  }

  /** Retire le partage d'écran **sans reconnexion** (l'appelant stoppe les pistes du flux). */
  async removeScreen(): Promise<void> {
    if (!this.pc) return;
    for (const s of this.screenSenders) {
      try {
        this.pc.removeTrack(s);
      } catch {
        /* ignore */
      }
    }
    this.screenSenders = [];
    this.screenVideoSender = null;
    if (this.screenStream) {
      // Vidéo ET audio du partage (les deux ont été publiés).
      for (const t of this.screenStream.getTracks()) {
        delete this.manifest[t.id];
        this.wsSend({ t: "unpublish", id: t.id });
      }
      this.screenStream = null;
    }
  }

  // Plafonne le débit par piste et marque l'audio en priorité réseau haute → la voix passe avant
  // la vidéo en cas de congestion (latence/qualité voix préservées).
  private tuneSenders(): void {
    if (!this.pc) return;
    for (const sender of this.pc.getSenders()) {
      const kind = sender.track?.kind;
      if (!kind) continue;
      try {
        const p = sender.getParameters();
        if (!p.encodings || p.encodings.length === 0) p.encodings = [{}];
        if (kind === "audio") {
          p.encodings[0].maxBitrate = OPUS_BITRATE;
          p.encodings[0].priority = "high";
          (p.encodings[0] as { networkPriority?: RTCPriorityType }).networkPriority = "high";
        } else {
          p.encodings[0].maxBitrate = VIDEO_BITRATE;
        }
        void sender.setParameters(p).catch(() => {});
      } catch {
        /* selon navigateur */
      }
    }
  }

  // ───────────────────────────── Mixeur sortant / soundboard ─────────────────────────────

  // Construit le graphe « micro → destination publiable ». Sans micro, le mixeur sert quand
  // même de support au soundboard (la piste publiée ne porte alors que les sons joués).
  private buildMixer(micTrack: MediaStreamTrack | null): void {
    try {
      const ctx = new AudioContext();
      this.mixCtx = ctx;
      void ctx.resume().catch(() => {});
      this.mixDest = ctx.createMediaStreamDestination();
      if (micTrack) {
        const src = ctx.createMediaStreamSource(new MediaStream([micTrack]));
        src.connect(this.mixDest);
      }
      this.mixedTrack = this.mixDest.stream.getAudioTracks()[0] ?? null;
    } catch {
      this.mixCtx = null;
      this.mixDest = null;
      this.mixedTrack = null;
    }
  }

  /** Joue un son du soundboard : transmis aux autres (mixé dans la piste publiée) + écho local. */
  async playSound(url: string, volume = 1): Promise<void> {
    const ctx = this.mixCtx;
    if (!ctx) throw new Error("mixage audio indisponible");
    void ctx.resume().catch(() => {});
    let buf = this.soundCache.get(url);
    if (!buf) {
      const resp = await fetch(url);
      if (!resp.ok) throw new Error(`audio ${resp.status}`);
      buf = await ctx.decodeAudioData(await resp.arrayBuffer());
      this.soundCache.set(url, buf);
    }
    const src = ctx.createBufferSource();
    src.buffer = buf;
    const gain = ctx.createGain();
    gain.gain.value = Math.min(Math.max(volume, 0), 1);
    src.connect(gain);
    if (this.mixDest) gain.connect(this.mixDest); // vers les autres participants
    if (!this.deafened) gain.connect(ctx.destination); // retour local (on s'entend jouer le son)
    src.start();
  }

  setMuted(muted: boolean): void {
    // Coupe le micro BRUT (en amont du mixeur) : la voix se tait, le soundboard passe encore.
    this.local?.getAudioTracks().forEach((t) => (t.enabled = !muted));
  }

  /** Règle la sensibilité de détection de la voix (seuil RMS manuel ou calibrage auto). */
  setSensitivity(opts: { auto: boolean; threshold: number }): void {
    this.speaking.setSensitivity(opts);
  }

  setDeafened(deaf: boolean): void {
    this.deafened = deaf;
    this.audioEls.forEach((el) => (el.muted = deaf));
  }

  /** Volume LOCAL d'un participant (0..1) — appliqué en direct et aux pistes futures. */
  setUserVolume(userId: string, volume: number): void {
    const v = Math.min(Math.max(volume, 0), 1);
    this.userVolumes.set(userId, v);
    this.elsByUser.get(userId)?.forEach((el) => (el.volume = v));
  }

  /** Pré-charge les volumes par participant (persistés par l'appelant). */
  seedUserVolumes(volumes: Record<string, number>): void {
    for (const [uid, v] of Object.entries(volumes)) {
      this.userVolumes.set(uid, Math.min(Math.max(v, 0), 1));
    }
  }

  /** Pré-charge les volumes de STREAM par participant (persistés par l'appelant) — appliqués aux
   *  pistes `screen_audio` reçues, sinon le réglage sauvegardé serait ignoré à chaque (re)connexion. */
  seedStreamVolumes(volumes: Record<string, number>): void {
    for (const [uid, v] of Object.entries(volumes)) {
      this.streamVolumes.set(uid, Math.min(Math.max(v, 0), 1));
    }
  }

  /** Volume du SON DU STREAM d'un participant (0..1) — côté spectateur, distinct du micro. */
  setStreamVolume(userId: string, volume: number): void {
    const v = Math.min(Math.max(volume, 0), 1);
    this.streamVolumes.set(userId, v);
    this.streamElsByUser.get(userId)?.forEach((el) => (el.volume = v));
  }

  /** Reroute EN DIRECT tous les flux audio distants vers la sortie choisie (réglages). */
  applyOutputDevice(): void {
    this.audioEls.forEach((el) => applySink(el));
  }

  hasVideo(): boolean {
    return !!this.camTrack;
  }

  /** Ferme proprement : retire le pair du SFU, coupe les pistes, ferme la connexion. */
  async close(): Promise<void> {
    this.closing = true;
    if (this.ws) {
      try {
        this.ws.close();
      } catch {
        /* ignore */
      }
      this.ws = null;
    }
    if (this.peerId) {
      try {
        await fetch(
          `${httpBase()}/sfu/rooms/${this.channelId}/peers/${this.peerId}?token=${encodeURIComponent(this.token)}`,
          { method: "DELETE" },
        );
      } catch {
        /* ignore */
      }
    }
    this.speaking.close();
    this.micMonitor?.getTracks().forEach((t) => t.stop());
    this.micMonitor = null;
    this.local?.getTracks().forEach((t) => t.stop());
    this.camTrack?.stop();
    this.mixedTrack?.stop();
    void this.mixCtx?.close().catch(() => {});
    this.mixCtx = null;
    this.mixDest = null;
    this.mixedTrack = null;
    this.soundCache.clear();
    this.audioEls.forEach((el) => {
      el.srcObject = null;
      el.remove();
    });
    this.audioEls.clear();
    this.elsByUser.clear();
    this.streamElsByUser.clear();
    this.pc?.close();
    this.pc = null;
    this.local = null;
    this.peerId = null;
    this.camTrack = null;
    this.camSender = null;
    this.screenStream = null;
    this.screenSenders = [];
    this.screenVideoSender = null;
  }
}
