import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, getAccessToken, setAuthLostHandler, setTokens } from "./api";
import type { TokenPair } from "./types";

// Stub localStorage (env node) — api.ts y lit/écrit le couple de jetons.
function installLocalStorage(): void {
  const store = new Map<string, string>();
  (globalThis as { localStorage?: Storage }).localStorage = {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
    clear: () => store.clear(),
    key: () => null,
    length: 0,
  } as Storage;
}

const pair = (access: string, refresh = "refresh-1"): TokenPair => ({
  access_token: access,
  refresh_token: refresh,
  token_type: "Bearer",
  expires_in: 600,
});

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

describe("request — rafraîchissement automatique sur 401", () => {
  beforeEach(() => {
    installLocalStorage();
    setTokens(pair("expired"));
    setAuthLostHandler(null);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    setTokens(null);
  });

  it("rafraîchit le jeton sur 401 puis rejoue la requête une fois", async () => {
    const calls: string[] = [];
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      calls.push(url);
      if (url.includes("/users/@me") && calls.filter((u) => u.includes("/users/@me")).length === 1) {
        return jsonResponse(401, { message: "expired" }); // 1er appel : jeton périmé
      }
      if (url.includes("/auth/token/refresh")) {
        return jsonResponse(200, pair("fresh", "refresh-2")); // rafraîchissement OK
      }
      return jsonResponse(200, { id: "1", username: "alice", display_name: null, avatar_id: null });
    });
    vi.stubGlobal("fetch", fetchMock);

    const me = await api.me();
    expect(me).toMatchObject({ id: "1", username: "alice" });
    // 3 appels : @me (401) → refresh (200) → @me rejoué (200).
    expect(calls.filter((u) => u.includes("/users/@me"))).toHaveLength(2);
    expect(calls.some((u) => u.includes("/auth/token/refresh"))).toBe(true);
    // Le nouveau jeton d'accès est bien en place.
    expect(getAccessToken()).toBe("fresh");
  });

  it("ne réessaie qu'une fois ; si le rafraîchissement échoue, signale la perte d'auth et lève", async () => {
    const authLost = vi.fn();
    setAuthLostHandler(authLost);
    let meCalls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/users/@me")) {
        meCalls += 1;
        return jsonResponse(401, { message: "expired" });
      }
      if (url.includes("/auth/token/refresh")) {
        return jsonResponse(401, { message: "refresh invalide" }); // rafraîchissement KO
      }
      return jsonResponse(200, {});
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(api.me()).rejects.toThrow();
    expect(authLost).toHaveBeenCalledTimes(1);
    // @me appelé une seule fois (pas de rejeu puisque le refresh a échoué).
    expect(meCalls).toBe(1);
  });

  it("un échec RÉSEAU du rafraîchissement ne déconnecte PAS (session persistante)", async () => {
    const authLost = vi.fn();
    setAuthLostHandler(authLost);
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/auth/token/refresh")) throw new TypeError("Failed to fetch"); // panne réseau
      return jsonResponse(401, { message: "expired" });
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(api.me()).rejects.toThrow();
    expect(authLost).not.toHaveBeenCalled(); // pas de déconnexion sur erreur réseau
    expect(getAccessToken()).toBe("expired"); // jeton CONSERVÉ (réessai ultérieur)
  });

  it("mutualise les rafraîchissements concurrents en un seul appel /refresh", async () => {
    let refreshCalls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/auth/token/refresh")) {
        refreshCalls += 1;
        return jsonResponse(200, pair("fresh"));
      }
      // 401 tant que le jeton courant n'est pas le jeton rafraîchi ; 200 ensuite.
      return getAccessToken() === "fresh"
        ? jsonResponse(200, [])
        : jsonResponse(401, { message: "expired" });
    });
    vi.stubGlobal("fetch", fetchMock);

    // Deux requêtes authentifiées en parallèle qui tombent toutes deux en 401.
    await Promise.all([api.listGuilds().catch(() => {}), api.listDMs().catch(() => {})]);
    expect(refreshCalls).toBe(1); // un SEUL rafraîchissement mutualisé
  });
});
