import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Cible du serveur Ozone (ozone-api). Surchargeable via OZONE_SERVER.
const SERVER = process.env.OZONE_SERVER ?? "http://127.0.0.1:8080";
const WS_SERVER = SERVER.replace(/^http/, "ws");
// Nœud média SFU (ozone-sfu). Surchargeable via OZONE_SFU.
const SFU = process.env.OZONE_SFU ?? "http://127.0.0.1:8081";

// Proxy de développement : zéro CORS, zéro changement serveur.
//   /api/*     -> SERVER/*        (REST, le préfixe /api est retiré)
//   /gateway   -> WS_SERVER/gateway (WebSocket temps réel)
//   /sfu/*     -> SFU/sfu/*       (signalisation média offre/réponse SDP)
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/api": {
        target: SERVER,
        changeOrigin: true,
        rewrite: (p) => p.replace(/^\/api/, ""),
      },
      "/sfu": {
        target: SFU,
        changeOrigin: true,
      },
      "/gateway": {
        target: WS_SERVER,
        ws: true,
        changeOrigin: true,
      },
    },
  },
});
