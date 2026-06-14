// Vérification adversariale du plan média SFU (standing rule).
// Le SFU doit refuser tout accès non autorisé au transport média.
import { createHmac } from "node:crypto";

const SFU = "http://127.0.0.1:8081";
const SECRET = "ozone-dev-voice-2026"; // secret partagé dev (API + SFU)
const ROOM = "999111222"; // faux channel_id

const b64url = (buf) =>
  Buffer.from(buf).toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

function jwt(secret, sub, kind, ttl = 3600) {
  const header = b64url('{"alg":"HS256","typ":"JWT"}');
  const iat = Math.floor(Date.now() / 1000);
  const payload = b64url(JSON.stringify({ sub, iat, exp: iat + ttl, kind }));
  const signing = `${header}.${payload}`;
  const sig = b64url(createHmac("sha256", secret).update(signing).digest());
  return `${signing}.${sig}`;
}

async function post(room, token) {
  const ac = new AbortController();
  const t = setTimeout(() => ac.abort(), 5000);
  try {
    const r = await fetch(`${SFU}/sfu/rooms/${room}/peers`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sdp: "v=0\r\ninvalid-offer\r\n", token }),
      signal: ac.signal,
    });
    return { status: r.status, body: (await r.text()).slice(0, 80) };
  } catch (e) {
    return { status: "ERR", body: String(e).slice(0, 80) };
  } finally {
    clearTimeout(t);
  }
}

const cases = [
  ["1. aucun jeton", "", [401]],
  ["2. jeton illisible", "abc.def.ghi", [401]],
  ["3. bon format, MAUVAIS secret", jwt("mauvais-secret", `1.${ROOM}`, "voice"), [401]],
  ["4. bon secret, MAUVAIS kind", jwt(SECRET, `1.${ROOM}`, "access"), [401]],
  ["5. bon secret/kind, MAUVAIS salon", jwt(SECRET, "1.888000", "voice"), [403]],
  ["6. jeton VALIDE (auth doit passer → 400 sur SDP bidon)", jwt(SECRET, `1.${ROOM}`, "voice"), [400]],
  ["7. jeton EXPIRÉ", jwt(SECRET, `1.${ROOM}`, "voice", -10), [401]],
];

let ok = 0;
for (const [label, token, expected] of cases) {
  const res = await post(ROOM, token);
  const pass = expected.includes(res.status);
  if (pass) ok++;
  console.log(`${pass ? "PASS" : "FAIL"}  ${label}  → ${res.status} ${res.body ? `(${res.body})` : ""}`);
}
console.log(`\n${ok}/${cases.length} cas conformes`);
process.exit(ok === cases.length ? 0 : 1);
