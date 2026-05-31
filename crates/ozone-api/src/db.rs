//! Connexion SQLite, migrations et **bootstrap de l'instance** (cf. docs/features/00-instances.md §8).

use crate::config::Config;
use crate::crypto;
use crate::state::{AppState, HubEvent, InstanceRuntime};
use ozone_proto::dto::RegistrationPolicy;
use ozone_proto::{Snowflake, SnowflakeGenerator};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub async fn connect_and_migrate(db_path: &str) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Connecte la base, applique les migrations et initialise l'instance si nécessaire.
pub async fn bootstrap_state(cfg: &Config) -> anyhow::Result<AppState> {
    let pool = connect_and_migrate(&cfg.db_path).await?;
    let ids = Arc::new(SnowflakeGenerator::new(1));

    let existing = sqlx::query(
        "SELECT instance_id, name, description, registration_policy, access_gate_hash, jwt_secret, version \
         FROM instance_config WHERE id = 1",
    )
    .fetch_optional(&pool)
    .await?;

    let (runtime, secret) = if let Some(r) = existing {
        let secret: String = r.get("jwt_secret");
        let gate_hash: Option<String> = r.get("access_gate_hash");
        let rt = InstanceRuntime {
            instance_id: Snowflake::from_i64(r.get::<i64, _>("instance_id")),
            name: r.get("name"),
            description: r.get("description"),
            version: r.get("version"),
            registration_policy: RegistrationPolicy::parse(
                &r.get::<String, _>("registration_policy"),
            ),
            gate_enabled: gate_hash.is_some(),
            gate_hash,
        };
        (rt, secret)
    } else {
        let instance_id = ids.next();
        let secret = crypto::random_token();
        let gate_hash = match &cfg.instance_password {
            Some(p) => Some(crypto::hash_password(p).map_err(|e| anyhow::anyhow!(e))?),
            None => None,
        };
        sqlx::query(
            "INSERT INTO instance_config \
             (id, instance_id, name, description, accent_color, registration_policy, access_gate_hash, jwt_secret, public_key, version, created_at) \
             VALUES (1, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?)",
        )
        .bind(instance_id.as_i64())
        .bind(cfg.instance_name.as_str())
        .bind(cfg.instance_description.as_deref())
        .bind(cfg.registration_policy.as_str())
        .bind(gate_hash.as_deref())
        .bind(secret.as_str())
        .bind("ozone-dev-public-key")
        .bind(cfg.version.as_str())
        .bind(now_ms())
        .execute(&pool)
        .await?;
        tracing::info!(
            "Instance « {} » initialisée (id={}, inscription={}, gate={}).",
            cfg.instance_name,
            instance_id,
            cfg.registration_policy,
            gate_hash.is_some()
        );
        let rt = InstanceRuntime {
            instance_id,
            name: cfg.instance_name.clone(),
            description: cfg.instance_description.clone(),
            version: cfg.version.clone(),
            registration_policy: RegistrationPolicy::parse(&cfg.registration_policy),
            gate_enabled: gate_hash.is_some(),
            gate_hash,
        };
        (rt, secret)
    };

    let (hub, _rx) = broadcast::channel::<HubEvent>(1024);

    // Secret des jetons vocaux : partagé avec le SFU via OZONE_VOICE_SECRET ; sinon le secret JWT.
    let voice_secret = std::env::var("OZONE_VOICE_SECRET")
        .ok()
        .map(|s| s.into_bytes())
        .unwrap_or_else(|| secret.clone().into_bytes());

    // Répertoire des pièces jointes (créé si absent).
    let upload_dir = std::env::var("OZONE_UPLOAD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("ozone-uploads"));
    let _ = std::fs::create_dir_all(&upload_dir);

    Ok(AppState {
        pool,
        ids,
        jwt_secret: Arc::new(secret.into_bytes()),
        instance: Arc::new(runtime),
        hub,
        presence: Arc::new(crate::presence::Registry::new()),
        voice_secret: Arc::new(voice_secret),
        upload_dir: Arc::new(upload_dir),
    })
}
