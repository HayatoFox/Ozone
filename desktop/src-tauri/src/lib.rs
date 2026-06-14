// Point d'entrée de l'application Tauri. Le front (React/Vite buildé) est chargé depuis les
// ressources empaquetées ; toute la logique réseau (API, Gateway WS, SFU) est côté front et cible
// l'URL d'instance saisie par l'utilisateur (cf. desktop/src/lib/instance.ts).

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Accès direct au micro/à la caméra SANS prompt (comme Discord) : l'app est notre propre code
    // de confiance, et le média n'est acquis que lorsque l'utilisateur rejoint un vocal. Sur
    // Windows, WebView2 lit ses arguments additionnels depuis cette variable d'environnement ;
    // `--use-fake-ui-for-media-stream` auto-accorde les demandes getUserMedia (les VRAIS
    // périphériques sont utilisés — ce flag ne fait qu'accepter automatiquement l'UI de permission).
    #[cfg(target_os = "windows")]
    {
        let existing = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
        let flag = "--use-fake-ui-for-media-stream";
        if !existing.contains(flag) {
            let merged = if existing.is_empty() {
                flag.to_string()
            } else {
                format!("{existing} {flag}")
            };
            std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", merged);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .run(tauri::generate_context!())
        .expect("erreur au démarrage de l'application Tauri");
}
