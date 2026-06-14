// Point d'entrée de l'application Tauri. Le front (React/Vite buildé) est chargé depuis les
// ressources empaquetées ; toute la logique réseau (API, Gateway WS, SFU) est côté front et cible
// l'URL d'instance saisie par l'utilisateur (cf. desktop/src/lib/instance.ts).

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("erreur au démarrage de l'application Tauri");
}
