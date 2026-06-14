import { describe, expect, it } from "vitest";
import { hasPerm, togglePerm } from "./permissions";

describe("permissions — bitfield u64 en chaîne (BigInt)", () => {
  it("teste un bit bas", () => {
    expect(hasPerm("8", 1n << 3n)).toBe(true); // ADMINISTRATOR
    expect(hasPerm("0", 1n << 3n)).toBe(false);
  });

  it("teste un bit haut au-delà de la précision Number", () => {
    const bit = 1n << 51n; // PIN_MESSAGES
    const perms = bit.toString();
    expect(hasPerm(perms, bit)).toBe(true);
    // Un bit voisin ne doit pas être confondu (la précision Number échouerait ici).
    expect(hasPerm(perms, 1n << 50n)).toBe(false);
    expect(hasPerm(perms, 1n << 52n)).toBe(false);
  });

  it("active puis désactive un bit sans toucher aux autres", () => {
    let p = "8"; // ADMINISTRATOR
    p = togglePerm(p, 1n << 51n, true); // + PIN_MESSAGES
    expect(hasPerm(p, 1n << 3n)).toBe(true);
    expect(hasPerm(p, 1n << 51n)).toBe(true);
    p = togglePerm(p, 1n << 3n, false); // - ADMINISTRATOR
    expect(hasPerm(p, 1n << 3n)).toBe(false);
    expect(hasPerm(p, 1n << 51n)).toBe(true);
  });

  it("gère une chaîne vide", () => {
    expect(hasPerm("", 1n << 0n)).toBe(false);
    expect(togglePerm("", 1n << 0n, true)).toBe("1");
  });
});
