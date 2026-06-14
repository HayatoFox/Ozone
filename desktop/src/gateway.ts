// Client Gateway temps réel (WebSocket). Handshake IDENTIFY/HELLO/READY + RESUME,
// heartbeat (avec détection de connexion zombie), ré-émission des dispatch vers un callback.
// Cf. crates/ozone-proto/src/gateway.rs et crates/ozone-api/src/gateway.rs.

import type { GatewayFrame } from "./types";
import { gatewayWsUrl } from "./lib/instance";

const OP = {
  DISPATCH: 0,
  HEARTBEAT: 1,
  IDENTIFY: 2,
  VOICE_SPEAKING: 5, // uplink : je parle / je me tais (indicateur vocal temps réel)
  RESUME: 6,
  RECONNECT: 7,
  INVALID_SESSION: 9,
  HELLO: 10,
  HEARTBEAT_ACK: 11,
} as const;

export type GatewayEvent = { t: string; d: unknown };
type EventHandler = (ev: GatewayEvent) => void;
type StatusHandler = (status: GatewayStatus) => void;

export type GatewayStatus = "connecting" | "ready" | "resumed" | "disconnected";

// Crochets d'authentification : le jeton est lu à chaque (ré)IDENTIFY (jamais figé), et
// le rafraîchissement est tenté avant d'abandonner sur INVALID_SESSION.
export interface GatewayHooks {
  refreshAuth?: () => Promise<boolean>;
  onAuthLost?: () => void;
}

// URL WS de la Gateway : origine courante (web) ou instance configurée (.exe). Cf. lib/instance.
function gatewayUrl(): string {
  return gatewayWsUrl();
}

export class Gateway {
  private ws: WebSocket | null = null;
  private getToken: () => string | null;
  private onEvent: EventHandler;
  private onStatus: StatusHandler;
  private hooks: GatewayHooks;

  private heartbeatTimer: number | null = null;
  private reconnectTimer: number | null = null;
  private seq = 0;
  private sessionId: string | null = null;
  private closedByUser = false;
  private backoff = 1000;
  private invalidStreak = 0;
  private awaitingAck = false;

  constructor(
    getToken: () => string | null,
    onEvent: EventHandler,
    onStatus: StatusHandler,
    hooks: GatewayHooks = {},
  ) {
    this.getToken = getToken;
    this.onEvent = onEvent;
    this.onStatus = onStatus;
    this.hooks = hooks;
  }

  connect(): void {
    this.closedByUser = false;
    this.invalidStreak = 0;
    this.open();
  }

  private open(): void {
    this.onStatus("connecting");
    const ws = new WebSocket(gatewayUrl());
    this.ws = ws;

    ws.onmessage = (e) => this.handleMessage(e.data as string);
    ws.onclose = () => this.handleClose();
    ws.onerror = () => {
      /* close suivra */
    };
  }

  private identify(): void {
    this.send({ op: OP.IDENTIFY, d: { token: this.getToken() } });
  }

  private handleMessage(raw: string): void {
    let frame: GatewayFrame;
    try {
      frame = JSON.parse(raw) as GatewayFrame;
    } catch {
      return;
    }
    if (typeof frame.s === "number") this.seq = frame.s;

    switch (frame.op) {
      case OP.HELLO: {
        const interval = (frame.d as { heartbeat_interval: number }).heartbeat_interval;
        this.startHeartbeat(interval);
        // RESUME si on a une session, sinon IDENTIFY (jeton lu à l'instant).
        if (this.sessionId) {
          this.send({
            op: OP.RESUME,
            d: { token: this.getToken(), session_id: this.sessionId, seq: this.seq },
          });
        } else {
          this.identify();
        }
        break;
      }
      case OP.HEARTBEAT_ACK:
        this.awaitingAck = false;
        break;
      case OP.RECONNECT:
        this.ws?.close();
        break;
      case OP.INVALID_SESSION:
        this.handleInvalidSession();
        break;
      case OP.DISPATCH: {
        const t = frame.t ?? "";
        if (t === "READY") {
          const d = frame.d as { session_id: string };
          this.sessionId = d.session_id;
          this.backoff = 1000;
          this.invalidStreak = 0;
          this.onStatus("ready");
        } else if (t === "RESUMED") {
          this.backoff = 1000;
          this.invalidStreak = 0;
          this.onStatus("resumed");
        }
        this.onEvent({ t, d: frame.d });
        break;
      }
    }
  }

  // Session non reprenable. Première fois : re-IDENTIFY (peut être transitoire). Si ça
  // se répète, le jeton est probablement périmé → on le rafraîchit ; si impossible, on
  // ABANDONNE (pas de boucle serrée de ré-IDENTIFY).
  private handleInvalidSession(): void {
    this.sessionId = null;
    this.seq = 0;
    this.invalidStreak += 1;
    if (this.invalidStreak <= 1 || !this.hooks.refreshAuth) {
      this.identify();
      return;
    }
    void this.hooks.refreshAuth().then((ok) => {
      if (ok) {
        this.invalidStreak = 0;
        this.identify();
      } else {
        // Échec : fermer → reconnexion avec back-off (réseau transitoire). Si le jeton est
        // définitivement mort, refreshTokens() a déconnecté (logout → close), ce qui stoppe ici.
        this.ws?.close();
      }
    });
  }

  private startHeartbeat(interval: number): void {
    this.stopHeartbeat();
    this.awaitingAck = false;
    this.heartbeatTimer = window.setInterval(() => {
      // ACK manquant depuis le dernier battement ⇒ connexion zombie (réseau à demi ouvert)
      // → on ferme pour déclencher une reconnexion/RESUME plutôt que de rester muet.
      if (this.awaitingAck) {
        this.ws?.close();
        return;
      }
      this.awaitingAck = true;
      this.send({ op: OP.HEARTBEAT, d: this.seq });
    }, interval);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer !== null) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    this.awaitingAck = false;
  }

  private handleClose(): void {
    this.stopHeartbeat();
    this.onStatus("disconnected");
    if (this.closedByUser) return;
    // Reconnexion avec back-off (tente un RESUME si la session est encore vivante côté serveur).
    this.reconnectTimer = window.setTimeout(() => this.open(), this.backoff);
    this.backoff = Math.min(this.backoff * 2, 30000);
  }

  private send(frame: GatewayFrame): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(frame));
    }
  }

  /** Signale au serveur que je parle / me tais (relayé aux autres membres de mon salon vocal). */
  sendVoiceSpeaking(speaking: boolean): void {
    this.send({ op: OP.VOICE_SPEAKING, d: { speaking } });
  }

  close(): void {
    this.closedByUser = true;
    this.stopHeartbeat();
    if (this.reconnectTimer !== null) clearTimeout(this.reconnectTimer);
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      ws.onmessage = null;
      ws.onclose = null;
      ws.onerror = null;
      ws.close();
    }
  }
}
