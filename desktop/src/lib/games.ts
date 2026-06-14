// Catalogue préfait de jeux connus pour « Jeux joués » du profil serveur.
// Les visuels sont des placeholders pour l'instant (tuile colorée + initiales) ; on branchera
// de vraies jaquettes plus tard. Les `key` sont des slugs (alphanum/-/_ — validés côté serveur).

export interface GameDef {
  key: string;
  name: string;
}

export const GAMES: GameDef[] = [
  { key: "counter-strike-2", name: "Counter-Strike 2" },
  { key: "valorant", name: "Valorant" },
  { key: "league-of-legends", name: "League of Legends" },
  { key: "dota-2", name: "Dota 2" },
  { key: "overwatch-2", name: "Overwatch 2" },
  { key: "apex-legends", name: "Apex Legends" },
  { key: "fortnite", name: "Fortnite" },
  { key: "rocket-league", name: "Rocket League" },
  { key: "minecraft", name: "Minecraft" },
  { key: "roblox", name: "Roblox" },
  { key: "gta-v", name: "Grand Theft Auto V" },
  { key: "rust", name: "Rust" },
  { key: "escape-from-tarkov", name: "Escape from Tarkov" },
  { key: "elden-ring", name: "Elden Ring" },
  { key: "baldurs-gate-3", name: "Baldur's Gate 3" },
  { key: "cyberpunk-2077", name: "Cyberpunk 2077" },
  { key: "helldivers-2", name: "Helldivers 2" },
  { key: "palworld", name: "Palworld" },
  { key: "marvel-rivals", name: "Marvel Rivals" },
  { key: "the-finals", name: "The Finals" },
  { key: "deadlock", name: "Deadlock" },
  { key: "vrchat", name: "VRChat" },
  { key: "phasmophobia", name: "Phasmophobia" },
  { key: "lethal-company", name: "Lethal Company" },
  { key: "sea-of-thieves", name: "Sea of Thieves" },
  { key: "terraria", name: "Terraria" },
  { key: "stardew-valley", name: "Stardew Valley" },
  { key: "satisfactory", name: "Satisfactory" },
  { key: "subnautica", name: "Subnautica" },
  { key: "peak", name: "PEAK" },
  { key: "conan-exiles", name: "Conan Exiles" },
  { key: "genshin-impact", name: "Genshin Impact" },
  { key: "final-fantasy-xiv", name: "Final Fantasy XIV" },
  { key: "dofus", name: "Dofus" },
  { key: "forza-horizon", name: "Forza Horizon" },
  { key: "celeste", name: "Celeste" },
  { key: "hades", name: "Hades" },
  { key: "hollow-knight", name: "Hollow Knight" },
];

const BY_KEY = new Map(GAMES.map((g) => [g.key, g]));
export function gameName(key: string): string {
  return BY_KEY.get(key)?.name ?? key;
}
