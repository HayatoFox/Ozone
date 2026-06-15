import { describe, expect, it } from "vitest";
import { deriveAuthKeys, randomSaltHex, unwrapPrivateKey, wrapPrivateKey } from "./e2ee";

// Cœur crypto de la persistance E2EE : dérivation auth/KEK depuis (mot de passe, sel) + (dé)emballage
// de la clé privée. Garantit que le serveur (qui ne voit que `authSecret` + l'escrow) ne peut pas déballer.

const SALT_A = "a".repeat(64);
const SALT_B = "b".repeat(64);

describe("randomSaltHex", () => {
  it("produit 64 caractères hex, différents à chaque appel", () => {
    const a = randomSaltHex();
    expect(a).toMatch(/^[0-9a-f]{64}$/);
    expect(a).not.toBe(randomSaltHex());
  });
});

describe("deriveAuthKeys", () => {
  it("est déterministe pour (mot de passe, sel)", async () => {
    const a = await deriveAuthKeys("Sup3r-Ozone-Pw", SALT_A);
    const b = await deriveAuthKeys("Sup3r-Ozone-Pw", SALT_A);
    expect(a.authSecret).toBe(b.authSecret);
    expect(a.authSecret.length).toBeGreaterThan(20); // base64 d'un hash 256 bits
  });

  it("dépend du mot de passe ET du sel", async () => {
    const base = await deriveAuthKeys("Sup3r-Ozone-Pw", SALT_A);
    expect((await deriveAuthKeys("autre-mdp", SALT_A)).authSecret).not.toBe(base.authSecret);
    expect((await deriveAuthKeys("Sup3r-Ozone-Pw", SALT_B)).authSecret).not.toBe(base.authSecret);
  });
});

describe("wrap/unwrap de la clé privée", () => {
  it("aller-retour avec la bonne KEK (autre appareil, même mdp+sel), échec avec la mauvaise", async () => {
    const pair = await crypto.subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, [
      "deriveKey",
    ]);
    const privJwk = await crypto.subtle.exportKey("jwk", pair.privateKey);

    const good = await deriveAuthKeys("bon-mdp-123", SALT_A);
    const wrapped = await wrapPrivateKey(good.kek, privJwk);
    expect(wrapped).toContain("|");

    // Re-dérivation depuis le même (mdp, sel) — comme sur un autre appareil après prelogin → récupère la clé.
    const otherDevice = await deriveAuthKeys("bon-mdp-123", SALT_A);
    const out = await unwrapPrivateKey(otherDevice.kek, wrapped);
    expect(out.d).toBe(privJwk.d);
    expect(out.crv).toBe("P-256");

    // Mauvais mot de passe → KEK différente → rejet (auth tag GCM) : le serveur ne peut pas déballer.
    const bad = await deriveAuthKeys("mauvais-mdp", SALT_A);
    await expect(unwrapPrivateKey(bad.kek, wrapped)).rejects.toThrow();
  });
});
