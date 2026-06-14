import { describe, expect, it } from "vitest";
import {
  guildHasUnread,
  guildMentionCount,
  idGt,
  isChannelUnread,
  memberTopRoleColor,
  reorderChannelPlan,
  roleColorHex,
} from "./store";
import { CH_CATEGORY, CH_TEXT, type Channel, type Member, type ReadState, type Role } from "./types";

describe("idGt — comparaison de Snowflakes (au-delà de la précision Number)", () => {
  it("compare correctement de grands ids identiques sauf les derniers chiffres", () => {
    // Ces deux valeurs sont égales en Number (perte de précision) mais distinctes en BigInt.
    expect(idGt("9007199254740993", "9007199254740992")).toBe(true);
    expect(idGt("9007199254740992", "9007199254740993")).toBe(false);
  });
  it("gère des longueurs différentes", () => {
    expect(idGt("100", "99")).toBe(true);
    expect(idGt("99", "100")).toBe(false);
  });
});

function ch(id: string, last?: string): Channel {
  return {
    id,
    guild_id: "1",
    type: CH_TEXT,
    name: "c" + id,
    topic: null,
    position: 0,
    parent_id: null,
    nsfw: false,
    rate_limit_per_user: 0,
    last_message_id: last ?? null,
  };
}

describe("isChannelUnread", () => {
  it("non-lu si dernier message > dernier lu", () => {
    const rs: ReadState = { channel_id: "5", last_read_id: "100", mention_count: 0 };
    expect(isChannelUnread("101", rs)).toBe(true);
    expect(isChannelUnread("100", rs)).toBe(false);
    expect(isChannelUnread("99", rs)).toBe(false);
  });
  it("non-lu si aucun marqueur mais des messages", () => {
    expect(isChannelUnread("100", undefined)).toBe(true);
  });
  it("lu si pas de message", () => {
    expect(isChannelUnread(null, undefined)).toBe(false);
  });
});

describe("guildHasUnread / guildMentionCount", () => {
  const readStates: Record<string, ReadState> = {
    a: { channel_id: "a", last_read_id: "10", mention_count: 0 },
    b: { channel_id: "b", last_read_id: "5", mention_count: 3 },
  };
  it("détecte un salon non-lu", () => {
    const channels = [ch("a", "10"), ch("b", "20")];
    expect(guildHasUnread(channels, readStates)).toBe(true);
  });
  it("ignore les catégories", () => {
    const cat: Channel = { ...ch("z", "999"), type: CH_CATEGORY };
    expect(guildHasUnread([cat], {})).toBe(false);
  });
  it("somme les mentions", () => {
    expect(guildMentionCount([ch("a"), ch("b")], readStates)).toBe(3);
  });
});

describe("roleColorHex / memberTopRoleColor", () => {
  it("formate une couleur u32 en hex", () => {
    expect(roleColorHex(0x5865f2)).toBe("#5865f2");
    expect(roleColorHex(0)).toBe("#000000");
  });
  it("choisit le rôle coloré de plus haute position", () => {
    const roles: Role[] = [
      { id: "1", guild_id: "1", name: "low", color: 0x111111, hoist: false, position: 1, permissions: "0", mentionable: false, managed: false },
      { id: "2", guild_id: "1", name: "high", color: 0x22ff22, hoist: false, position: 5, permissions: "0", mentionable: false, managed: false },
      { id: "3", guild_id: "1", name: "nocolor", color: 0, hoist: false, position: 9, permissions: "0", mentionable: false, managed: false },
    ];
    const member: Member = {
      user: { id: "u", username: "u", display_name: null, avatar_id: null },
      nick: null,
      roles: ["1", "2", "3"],
      joined_at: 0,
    };
    expect(memberTopRoleColor(roles, member)).toBe("#22ff22");
  });
  it("retourne null sans rôle coloré", () => {
    const member: Member = {
      user: { id: "u", username: "u", display_name: null, avatar_id: null },
      nick: null,
      roles: [],
      joined_at: 0,
    };
    expect(memberTopRoleColor([], member)).toBeNull();
  });
});

describe("reorderChannelPlan — glisser-déposer salons + catégories", () => {
  // A, B à la racine ; CAT { X, Y } ; CAT2 { Z }.
  function mk(id: string, type: number, parent: string | null, position: number): Channel {
    return {
      id,
      guild_id: "1",
      type,
      name: "c" + id,
      topic: null,
      position,
      parent_id: parent,
      nsfw: false,
      rate_limit_per_user: 0,
      last_message_id: null,
    };
  }
  const layout = (): Channel[] => [
    mk("A", CH_TEXT, null, 0),
    mk("B", CH_TEXT, null, 1),
    mk("CAT", CH_CATEGORY, null, 2),
    mk("X", CH_TEXT, "CAT", 3),
    mk("Y", CH_TEXT, "CAT", 4),
    mk("CAT2", CH_CATEGORY, null, 5),
    mk("Z", CH_TEXT, "CAT2", 6),
  ];
  const order = (plan: { id: string; parent_id: string | null }[]) => plan.map((p) => p.id);
  const parentOf = (plan: { id: string; parent_id: string | null }[], id: string) =>
    plan.find((p) => p.id === id)?.parent_id;

  it("réordonne deux salons racine (B avant A)", () => {
    const plan = reorderChannelPlan(layout(), "B", { id: "A", mode: "before" });
    expect(order(plan).slice(0, 2)).toEqual(["B", "A"]);
    expect(parentOf(plan, "B")).toBeNull();
  });

  it("déplace un salon dans une catégorie", () => {
    const plan = reorderChannelPlan(layout(), "A", { id: "CAT", mode: "into" });
    expect(parentOf(plan, "A")).toBe("CAT");
    // A se place après les enfants existants de CAT.
    expect(order(plan)).toEqual(["B", "CAT", "X", "Y", "A", "CAT2", "Z"]);
  });

  it("sort un salon d'une catégorie vers la racine", () => {
    const plan = reorderChannelPlan(layout(), "X", { id: "", mode: "root" });
    expect(parentOf(plan, "X")).toBeNull();
    expect(order(plan)).toEqual(["A", "B", "X", "CAT", "Y", "CAT2", "Z"]);
  });

  it("réordonne une catégorie en gardant ses enfants attachés", () => {
    const plan = reorderChannelPlan(layout(), "CAT2", { id: "CAT", mode: "before" });
    expect(order(plan)).toEqual(["A", "B", "CAT2", "Z", "CAT", "X", "Y"]);
    expect(parentOf(plan, "Z")).toBe("CAT2");
  });

  it("positions séquentielles sans trou", () => {
    const plan = reorderChannelPlan(layout(), "Y", { id: "A", mode: "before" });
    expect(plan.map((p) => p.position)).toEqual(plan.map((_, i) => i));
  });

  it("no-op si la cible est le salon glissé", () => {
    expect(reorderChannelPlan(layout(), "A", { id: "A", mode: "before" })).toEqual([]);
  });
});
