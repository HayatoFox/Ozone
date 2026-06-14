import animate from "tailwindcss-animate";

/** @type {import('tailwindcss').Config} */
export default {
  // Le thème bascule via la classe racine `.theme-dark` (sombre par défaut).
  darkMode: ["class", ".theme-dark"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  // Les classes de thème sont posées dynamiquement (`theme-${id}`) → on empêche leur purge,
  // sinon les règles `.theme-*` de @layer base (variables CSS) seraient supprimées.
  safelist: ["theme-dark", "theme-light", "theme-midnight", "theme-custom"],
  theme: {
    extend: {
      // Tokens « façon Discord » — référencent les variables CSS (cf. src/index.css).
      colors: {
        // Surfaces
        deepest: "var(--bg-deepest)", // rail serveurs / fond profond
        sidebar: "var(--bg-sidebar)", // sidebar salons / membres
        chat: "var(--bg-chat)", // zone de chat
        field: "var(--bg-field)", // champs / composeur
        userpanel: "var(--bg-userpanel)", // panneau utilisateur
        floating: "var(--bg-floating)", // popouts / menus / tooltips
        modal: "var(--bg-modal)", // dialogue (toujours opaque)
        "modal-nav": "var(--bg-modal-nav)",

        // Voiles translucides (survol / actif / sélectionné)
        hover: "var(--mod-hover)",
        active: "var(--mod-active)",
        selected: "var(--mod-selected)",
        mentioned: "var(--mentioned)",

        // Texte
        header: "var(--text-header)", // titres / pseudos
        subtext: "var(--text-subtext)", // labels secondaires
        normal: "var(--text-normal)", // corps
        muted: "var(--text-muted)", // atténué
        channel: "var(--text-channel)", // # salon au repos
        link: "var(--text-link)",

        // Interactif (icônes / labels)
        "interactive-normal": "var(--interactive-normal)",
        "interactive-hover": "var(--interactive-hover)",
        "interactive-active": "var(--interactive-active)",
        "interactive-muted": "var(--interactive-muted)",

        // Accent (iris Ozone)
        accent: "var(--accent)",
        "accent-hover": "var(--accent-hover)",
        // Héritage : les anciens usages « blurple » suivent désormais l'accent du thème.
        blurple: "var(--accent)",
        "blurple-hover": "var(--accent-hover)",

        // Statuts de présence
        online: "var(--status-online)",
        idle: "var(--status-idle)",
        dnd: "var(--status-dnd)",
        offline: "var(--status-offline)",

        // Bordure thématique (groove sombre / claire selon le thème).
        line: "var(--border)",
        // Contour de la carte flottante (un cran plus marqué que `line`).
        cardline: "var(--card-ring)",
        // Survol de message (subtil, indépendant du fond).
        "message-hover": "var(--message-hover)",
      },
      fontFamily: {
        // « gg sans » d'abord (vraie police Discord si installée — Discord la pose) ;
        // sinon Inter (chargée via @fontsource), puis repli système.
        sans: [
          "gg sans",
          "Inter Variable",
          "Inter",
          "Noto Sans",
          "-apple-system",
          "BlinkMacSystemFont",
          "Helvetica Neue",
          "Helvetica",
          "Arial",
          "sans-serif",
        ],
        mono: [
          "gg mono",
          "Consolas",
          "Liberation Mono",
          "Menlo",
          "Monaco",
          "Courier New",
          "monospace",
        ],
      },
      transitionTimingFunction: {
        discord: "cubic-bezier(0.4, 0, 0.2, 1)",
        smooth: "cubic-bezier(0.32, 0.72, 0, 1)",
      },
      // Élévations douces et superposées (rendu « premium » façon Apple, rien de brut).
      // Surcharge l'échelle Tailwind ⇒ tout `shadow-{sm,md,lg,xl,2xl}` de l'app se raffine d'un coup.
      boxShadow: {
        xs: "0 1px 2px rgb(0 0 0 / 0.28)",
        sm: "0 2px 8px rgb(0 0 0 / 0.30)",
        DEFAULT: "0 2px 8px rgb(0 0 0 / 0.30)",
        md: "0 6px 18px rgb(0 0 0 / 0.36)",
        lg: "0 12px 30px rgb(0 0 0 / 0.42)",
        xl: "0 18px 44px rgb(0 0 0 / 0.48)",
        "2xl": "0 30px 66px rgb(0 0 0 / 0.55)",
        // Popouts / menus flottants : halo doux + légère définition de bord.
        pop: "0 10px 28px rgb(0 0 0 / 0.42), 0 1px 0 rgb(255 255 255 / 0.04) inset",
      },
      keyframes: {
        shimmer: {
          "0%": { backgroundPosition: "-400px 0" },
          "100%": { backgroundPosition: "400px 0" },
        },
        "msg-in": {
          from: { opacity: "0", transform: "translateY(6px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "overlay-in": {
          from: { opacity: "0" },
          to: { opacity: "1" },
        },
        "pop-in": {
          from: { opacity: "0", transform: "scale(0.96)" },
          to: { opacity: "1", transform: "scale(1)" },
        },
        loadbar: {
          "0%": { transform: "translateX(-110%)" },
          "100%": { transform: "translateX(260%)" },
        },
        // Plein écran vocal : la scène s'étire rapidement vers les coins (pas brut).
        "fs-zoom": {
          from: { transform: "scale(0.86)", opacity: "0.4" },
          to: { transform: "scale(1)", opacity: "1" },
        },
        // Mise en avant d'une tuile (focus) : grandit doucement depuis le centre.
        "spot-in": {
          from: { transform: "scale(0.92)", opacity: "0.6" },
          to: { transform: "scale(1)", opacity: "1" },
        },
      },
      animation: {
        shimmer: "shimmer 1.4s linear infinite",
        "msg-in": "msg-in 140ms ease-out",
        "overlay-in": "overlay-in 150ms ease-out",
        "pop-in": "pop-in 140ms cubic-bezier(0.16, 1, 0.3, 1)",
        loadbar: "loadbar 1.1s ease-in-out infinite",
        "fs-zoom": "fs-zoom 320ms cubic-bezier(0.16, 1, 0.3, 1)",
        "spot-in": "spot-in 200ms cubic-bezier(0.16, 1, 0.3, 1)",
      },
    },
  },
  plugins: [animate],
};
