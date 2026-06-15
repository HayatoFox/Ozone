import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/inter"; // substitut libre de gg sans (chargé pour les machines sans gg sans)
// Polices du « style de pseudonyme » (poids 400, latin uniquement → ~25 Ko chacune). Embarquées pour
// un rendu IDENTIQUE sur toutes les machines, indépendamment des polices système installées.
import "@fontsource/lora/latin-400.css"; // serif
import "@fontsource/jetbrains-mono/latin-400.css"; // mono
import "@fontsource/comfortaa/latin-400.css"; // arrondie
import "@fontsource/bungee/latin-400.css"; // display
import "@fontsource/caveat/latin-400.css"; // manuscrite
import "@fontsource/oswald/latin-400.css"; // condensée
import "@fontsource/roboto-slab/latin-400.css"; // slab
import { App } from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
