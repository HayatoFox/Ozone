import { describe, expect, it } from "vitest";
import { renderMarkdown, type MentionCtx } from "./markdown";

// Inspecte l'arbre React renvoyé (sans rendu DOM) : collecte les balises, les href et le texte.
interface Acc {
  tags: string[];
  hrefs: string[];
  text: string[];
}

/* eslint-disable @typescript-eslint/no-explicit-any */
function collect(node: any, acc: Acc): void {
  if (node == null || typeof node === "boolean") return;
  if (Array.isArray(node)) {
    node.forEach((n) => collect(n, acc));
    return;
  }
  if (typeof node === "string" || typeof node === "number") {
    acc.text.push(String(node));
    return;
  }
  const t = node.type;
  const name = typeof t === "string" ? t : (t?.name ?? "?");
  acc.tags.push(name);
  if (name === "a" && node.props?.href) acc.hrefs.push(node.props.href);
  collect(node.props?.children, acc);
}

function inspect(input: string, ctx: MentionCtx = {}): Acc {
  const acc: Acc = { tags: [], hrefs: [], text: [] };
  collect(renderMarkdown(input, ctx), acc);
  return acc;
}

describe("markdown — sécurité", () => {
  it("neutralise les liens javascript: (lien masqué)", () => {
    const a = inspect("clique [ici](javascript:alert(1))");
    // Aucun <a> émis ; le texte brut est conservé.
    expect(a.hrefs).toHaveLength(0);
    expect(a.text.join("")).toContain("javascript:alert(1)");
  });

  it("neutralise les liens data: (lien masqué)", () => {
    const a = inspect("[x](data:text/html,<script>alert(1)</script>)");
    expect(a.hrefs).toHaveLength(0);
  });

  it("autorise les liens http(s) masqués", () => {
    const a = inspect("[Ozone](https://example.com/page)");
    expect(a.hrefs).toEqual(["https://example.com/page"]);
  });

  it("transforme une URL nue en lien http(s) sûr", () => {
    const a = inspect("voir https://example.com ici");
    expect(a.hrefs).toEqual(["https://example.com"]);
  });

  it("n'émet jamais de balise non prévue", () => {
    const a = inspect("# titre\n**gras** _ital_ ~~barré~~ `code` ||spoil||\n> cite\n- item");
    const allowed = new Set([
      "div", "p", "span", "strong", "em", "u", "s", "code", "pre", "a", "ul", "ol", "li", "img",
      "Spoiler", "Mention", "Quote", "CodeBlock", "Fragment", "?",
    ]);
    for (const tag of a.tags) expect(allowed.has(tag)).toBe(true);
  });

  it("rend un emoji custom en <img> vers /api/emojis (pas de script)", () => {
    const a = inspect("salut <:wave:123> !");
    expect(a.tags).toContain("img");
    expect(a.tags).not.toContain("script");
  });
});

describe("markdown — formatage", () => {
  it("rend le gras", () => {
    expect(inspect("**salut**").tags).toContain("strong");
  });

  it("rend le souligné avec __ (et pas du gras)", () => {
    const a = inspect("__souligné__");
    expect(a.tags).toContain("u");
    expect(a.tags).not.toContain("strong");
  });

  it("n'interprète pas le contenu d'un code inline", () => {
    const a = inspect("`**pas gras**`");
    expect(a.tags).toContain("code");
    expect(a.tags).not.toContain("strong");
    expect(a.text.join("")).toContain("**pas gras**");
  });

  it("résout une mention utilisateur via le contexte", () => {
    const ctx: MentionCtx = { user: (id) => (id === "123" ? { name: "alice" } : undefined) };
    const a = inspect("salut <@123> !", ctx);
    expect(a.tags).toContain("Mention");
    expect(a.text.join("")).toContain("@alice");
  });

  it("résout une mention salon", () => {
    const ctx: MentionCtx = { channel: (id) => (id === "9" ? { name: "général" } : undefined) };
    expect(inspect("<#9>", ctx).text.join("")).toContain("#général");
  });

  it("rend un bloc de code (sans interpréter)", () => {
    const a = inspect("```js\nconst x = **1**;\n```");
    expect(a.tags).toContain("CodeBlock");
    expect(a.tags).not.toContain("strong");
  });

  it("ne boucle pas sur une entrée pathologique", () => {
    expect(() => inspect("*".repeat(200) + "_".repeat(200) + "`".repeat(50))).not.toThrow();
  });
});
