//! WEB-профили пользователей в `[[web.vhosts]]` telemt и отложенный reload.
//!
//! Профили меняются через `PATCH /v1/config`: массив `web.vhosts` заменяется целиком, поэтому
//! остальные поля vhost переносятся из `GET /v1/config` как есть (`serde_json::Value`).
//! Изменённые профили начинают работать только после `POST /v1/system/reload`; пока идёт
//! предыдущий reload (в том числе drain), telemt отвечает `409 reload_in_progress`, и
//! [`ReloadScheduler`] повторяет запрос, объединяя несколько изменений в один reload.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::anyhow;
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::config::WebSecretMode;

use super::api_client::TelemtApiClient;
use super::types::{TelemtApiError, WebProfileTarget};

/// Пауза между попытками reload, пока telemt занят предыдущим.
const RELOAD_RETRY_INTERVAL: Duration = Duration::from_secs(15);
/// Предел ожидания: drain длится не дольше 3600 с.
const RELOAD_MAX_ATTEMPTS: u32 = 300;
/// Попытки PATCH при конкурентном изменении конфига (`409 revision_conflict`).
const PATCH_MAX_ATTEMPTS: u32 = 3;

fn secret_mode_str(mode: WebSecretMode) -> &'static str {
    match mode {
        WebSecretMode::Dd => "dd",
        WebSecretMode::Plain => "plain",
    }
}

fn normalize_base_path(value: Option<&str>) -> &str {
    value.unwrap_or_default().trim().trim_matches('/')
}

/// Адрес vhost: нормализованные `host` и `base_path`.
fn vhost_key(vhost: &Value) -> (Option<String>, String) {
    let host = vhost
        .get("host")
        .and_then(Value::as_str)
        .map(|host| host.trim().trim_end_matches('.').to_ascii_lowercase());
    let base_path = normalize_base_path(vhost.get("base_path").and_then(Value::as_str));
    (host, base_path.to_string())
}

fn vhost_matches(vhost: &Value, target: &WebProfileTarget) -> bool {
    let (host, base_path) = vhost_key(vhost);
    host.as_deref() == Some(target.host.as_str())
        && base_path == normalize_base_path(target.base_path.as_deref())
}

fn profile_user(profile: &Value) -> Option<&str> {
    profile.get("user").and_then(Value::as_str)
}

/// Добавляет профиль `user` в vhost `target`. `Ok(false)` — профиль уже есть.
pub(crate) fn upsert_profile(
    vhosts: &mut [Value],
    target: &WebProfileTarget,
    user: &str,
) -> Result<bool, anyhow::Error> {
    let vhost = vhosts
        .iter_mut()
        .find(|vhost| vhost_matches(vhost, target))
        .ok_or_else(|| {
            anyhow!(
                "В telemt нет WEB-vhost host={} base_path={:?}",
                target.host,
                normalize_base_path(target.base_path.as_deref())
            )
        })?;
    let object = vhost
        .as_object_mut()
        .ok_or_else(|| anyhow!("web.vhosts содержит не объект"))?;
    let profiles = object
        .entry("profiles")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| anyhow!("web.vhosts[].profiles не массив"))?;
    if profiles
        .iter()
        .any(|profile| profile_user(profile) == Some(user))
    {
        return Ok(false);
    }
    profiles.push(json!({
        "user": user,
        "secret_mode": secret_mode_str(target.secret_mode),
    }));
    Ok(true)
}

/// Профиль, снятый с vhost перед удалением пользователя. Хранит vhost и профиль как есть,
/// чтобы вернуть его, если telemt не удалил пользователя.
#[derive(Debug, Clone)]
pub(crate) struct RemovedProfile {
    vhost: (Option<String>, String),
    profile: Value,
}

/// Снимает профили `user` со всех vhost: telemt не даёт удалить пользователя,
/// на которого ссылается хоть один профиль. Возвращает снятые профили.
pub(crate) fn take_profiles(vhosts: &mut [Value], user: &str) -> Vec<RemovedProfile> {
    let mut removed = Vec::new();
    for vhost in vhosts.iter_mut() {
        let key = vhost_key(vhost);
        let Some(profiles) = vhost.get_mut("profiles").and_then(Value::as_array_mut) else {
            continue;
        };
        let (taken, kept) = std::mem::take(profiles)
            .into_iter()
            .partition(|profile| profile_user(profile) == Some(user));
        *profiles = kept;
        removed.extend(taken.into_iter().map(|profile| RemovedProfile {
            vhost: key.clone(),
            profile,
        }));
    }
    removed
}

/// Возвращает профили, снятые [`take_profiles`], в их vhost. Профиль, который уже есть,
/// и vhost, которого больше нет, пропускаются. `true` — что-то возвращено.
pub(crate) fn restore_profiles(vhosts: &mut [Value], removed: &[RemovedProfile]) -> bool {
    let mut restored = false;
    for item in removed {
        let Some(user) = profile_user(&item.profile) else {
            continue;
        };
        let Some(object) = vhosts
            .iter_mut()
            .find(|vhost| vhost_key(vhost) == item.vhost)
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let Some(profiles) = object
            .entry("profiles")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
        else {
            continue;
        };
        if profiles
            .iter()
            .any(|profile| profile_user(profile) == Some(user))
        {
            continue;
        }
        profiles.push(item.profile.clone());
        restored = true;
    }
    restored
}

fn is_api_error(error: &anyhow::Error, status: StatusCode, code: &str) -> bool {
    error
        .downcast_ref::<TelemtApiError>()
        .is_some_and(|api_error| {
            api_error.status == status && api_error.code.as_deref() == Some(code)
        })
}

/// Читает `web.vhosts`, применяет `change` и сохраняет результат `PATCH /v1/config`.
/// `Ok(false)` — изменений не потребовалось.
pub(crate) async fn patch_vhosts(
    client: &TelemtApiClient,
    mut change: impl FnMut(&mut [Value]) -> Result<bool, anyhow::Error>,
) -> Result<bool, anyhow::Error> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let config = client.get_success::<Value>("/v1/config").await?;
        let mut vhosts = match config.data.pointer("/web/vhosts") {
            Some(Value::Array(vhosts)) => vhosts.clone(),
            _ => Vec::new(),
        };
        if !change(&mut vhosts)? {
            return Ok(false);
        }
        let body = json!({ "web": { "vhosts": vhosts } });
        match client
            .mutate_with_revision::<Value, Value>(
                Method::PATCH,
                "/v1/config",
                Some(&body),
                &config.revision,
            )
            .await
        {
            Ok(_) => return Ok(true),
            Err(error)
                if attempt < PATCH_MAX_ATTEMPTS
                    && is_api_error(&error, StatusCode::CONFLICT, "revision_conflict") =>
            {
                tracing::debug!(
                    attempt = attempt,
                    "telemt config changed concurrently, retrying"
                );
            }
            Err(error) => return Err(error),
        }
    }
}

/// Запускает reload telemt в фоне; повторные запросы во время ожидания объединяются.
pub(crate) struct ReloadScheduler {
    client: Arc<TelemtApiClient>,
    drain_secs: u64,
    dirty: AtomicBool,
    running: AtomicBool,
}

impl ReloadScheduler {
    pub(crate) fn new(client: Arc<TelemtApiClient>, drain_secs: u64) -> Self {
        Self {
            client,
            drain_secs,
            dirty: AtomicBool::new(false),
            running: AtomicBool::new(false),
        }
    }

    /// Ставит reload в очередь. Reload читает конфиг с диска в момент запроса, поэтому
    /// изменения, пришедшие после принятого запроса, вызывают ещё один reload.
    pub(crate) fn request(self: &Arc<Self>) {
        self.dirty.store(true, Ordering::SeqCst);
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move { this.run().await });
    }

    async fn run(&self) {
        loop {
            while self.dirty.swap(false, Ordering::SeqCst) {
                self.reload_until_accepted().await;
            }
            self.running.store(false, Ordering::SeqCst);
            // Запрос мог прийти между проверкой `dirty` и сбросом `running`.
            if !self.dirty.load(Ordering::SeqCst) || self.running.swap(true, Ordering::SeqCst) {
                return;
            }
        }
    }

    async fn reload_until_accepted(&self) {
        let body = json!({
            "mode": "drain",
            "timeout_secs": self.drain_secs,
            "failure_policy": "rollback",
        });
        for attempt in 1..=RELOAD_MAX_ATTEMPTS {
            match self
                .client
                .post_success::<Value, Value>("/v1/system/reload", &body)
                .await
            {
                Ok(accepted) => {
                    tracing::info!(
                        reload_id = ?accepted.data.get("reload_id"),
                        target_generation = ?accepted.data.get("target_generation"),
                        drain_secs = self.drain_secs,
                        "telemt reload accepted after WEB profile change"
                    );
                    return;
                }
                Err(error) if is_api_error(&error, StatusCode::CONFLICT, "reload_in_progress") => {
                    tracing::debug!(attempt = attempt, "telemt reload in progress, waiting");
                    tokio::time::sleep(RELOAD_RETRY_INTERVAL).await;
                }
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "telemt reload failed; WEB profile changes stay inactive until next reload"
                    );
                    return;
                }
            }
        }
        tracing::warn!("telemt reload still in progress after waiting; giving up");
    }
}

#[cfg(test)]
mod tests {
    use super::{restore_profiles, take_profiles, upsert_profile};
    use crate::config::WebSecretMode;
    use crate::telemt_backend::types::WebProfileTarget;
    use serde_json::{Value, json};

    fn vhosts() -> Vec<Value> {
        vec![
            json!({
                "host": "joy.example.com",
                "base_path": "chill",
                "public_addr": "203.0.113.10:443",
                "decoy": {"mode": "http_upstream", "upstream": "http://127.0.0.1:18081"},
                "profiles": [{"user": "alice", "secret_mode": "dd"}]
            }),
            json!({
                "host": "other.example.com",
                "profiles": [{"user": "alice", "secret_mode": "plain"}]
            }),
        ]
    }

    fn target(base_path: Option<&str>) -> WebProfileTarget {
        WebProfileTarget {
            host: "joy.example.com".to_string(),
            base_path: base_path.map(str::to_string),
            secret_mode: WebSecretMode::Dd,
        }
    }

    #[test]
    fn upsert_adds_profile_and_keeps_other_fields() {
        let mut vhosts = vhosts();
        assert!(upsert_profile(&mut vhosts, &target(Some("/chill/")), "tg_1").unwrap());
        assert_eq!(
            vhosts[0]["profiles"][1],
            json!({"user": "tg_1", "secret_mode": "dd"})
        );
        assert_eq!(vhosts[0]["public_addr"], "203.0.113.10:443");
        assert_eq!(vhosts[0]["decoy"]["upstream"], "http://127.0.0.1:18081");
        assert_eq!(vhosts[1]["profiles"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn upsert_is_idempotent() {
        let mut vhosts = vhosts();
        assert!(!upsert_profile(&mut vhosts, &target(Some("chill")), "alice").unwrap());
        assert_eq!(vhosts[0]["profiles"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn upsert_creates_profiles_array_when_missing() {
        let mut vhosts = vec![json!({"host": "joy.example.com"})];
        assert!(upsert_profile(&mut vhosts, &target(None), "tg_1").unwrap());
        assert_eq!(vhosts[0]["profiles"][0]["user"], "tg_1");
    }

    #[test]
    fn upsert_fails_when_vhost_is_missing() {
        let mut vhosts = vhosts();
        assert!(upsert_profile(&mut vhosts, &target(Some("other")), "tg_1").is_err());
        assert!(upsert_profile(&mut vhosts, &target(None), "tg_1").is_err());
    }

    #[test]
    fn take_drops_user_from_every_vhost() {
        let mut vhosts = vhosts();
        let removed = take_profiles(&mut vhosts, "alice");
        assert_eq!(removed.len(), 2);
        assert!(vhosts[0]["profiles"].as_array().unwrap().is_empty());
        assert!(vhosts[1]["profiles"].as_array().unwrap().is_empty());
        assert!(take_profiles(&mut vhosts, "alice").is_empty());
    }

    #[test]
    fn restore_puts_taken_profiles_back_unchanged() {
        let mut vhosts = vhosts();
        let removed = take_profiles(&mut vhosts, "alice");
        assert!(restore_profiles(&mut vhosts, &removed));
        assert_eq!(vhosts, self::vhosts());
    }

    #[test]
    fn restore_skips_existing_profiles_and_missing_vhosts() {
        let mut vhosts = vhosts();
        let removed = take_profiles(&mut vhosts, "alice");
        let mut only_other = vec![vhosts.remove(1)];
        assert!(restore_profiles(&mut only_other, &removed));
        assert!(!restore_profiles(&mut only_other, &removed));
        assert_eq!(only_other[0]["profiles"].as_array().unwrap().len(), 1);
    }
}
