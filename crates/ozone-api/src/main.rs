//! Binaire serveur Ozone (mode tout-en-un).

#[tokio::main]
async fn main() {
    if let Err(e) = ozone_api::run().await {
        eprintln!("erreur fatale : {e:#}");
        std::process::exit(1);
    }
}
