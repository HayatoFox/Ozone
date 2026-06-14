import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/inter"; // substitut libre de gg sans (chargé pour les machines sans gg sans)
import { App } from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
