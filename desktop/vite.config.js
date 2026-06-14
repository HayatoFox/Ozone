var _a, _b;
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// Cible du serveur Ozone (ozone-api). Surchargeable via OZONE_SERVER.
var SERVER = (_a = process.env.OZONE_SERVER) !== null && _a !== void 0 ? _a : "http://127.0.0.1:8080";
var WS_SERVER = SERVER.replace(/^http/, "ws");
// Nœud média SFU (ozone-sfu). Surchargeable via OZONE_SFU.
var SFU = (_b = process.env.OZONE_SFU) !== null && _b !== void 0 ? _b : "http://127.0.0.1:8081";
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
                rewrite: function (p) { return p.replace(/^\/api/, ""); },
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
