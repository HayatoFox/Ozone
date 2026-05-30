//! Client natif Ozone — placeholder.
//!
//! L'interface GPU (GPUI, repli Iced) sera branchée ici. Le tout premier écran sera la
//! **connexion à une instance** (cf. `docs/features/00-instances.md`).

fn main() {
    println!("┌──────────────────────────────────────────────┐");
    println!("│  Ozone — client natif (placeholder)          │");
    println!("└──────────────────────────────────────────────┘");
    println!();
    println!("Prochaine étape UI : écran de connexion à une instance.");
    println!("  → docs/features/00-instances.md");
    println!("UI GPU à intégrer (GPUI, repli Iced) : docs/02-stack-technique.md");
    println!();

    let demo = ozone_core::InstanceRef::new("ozone.exemple.fr");
    println!("Démo cœur partagé — API base résolue : {}", demo.api_base());
}
