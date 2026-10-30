//! Отметки об отправленных пользователю уведомлениях о сроке и квоте.
//!
//! `kind` — вид уведомления (`expiry_warn_3`, `expired`, `quota_warn_90`), `scope` — срок
//! или квота, к которой оно относится: после продления или смены квоты уведомление
//! снова может быть отправлено.

use crate::db::{Db, STATUS_APPROVED, current_unix_timestamp};

impl Db {
    /// Атомарно отмечает уведомление отправленным. `false` — уже отправлялось
    /// или пользователь не approved.
    pub async fn claim_user_notification(
        &self,
        tg_user_id: i64,
        kind: &str,
        scope: &str,
    ) -> Result<bool, anyhow::Error> {
        let r = sqlx::query(
            "INSERT OR IGNORE INTO user_notifications (tg_user_id, kind, scope, sent_at)
             SELECT ?, ?, ?, ?
             WHERE EXISTS (
                 SELECT 1 FROM registration_requests WHERE tg_user_id = ? AND status = ?
             )",
        )
        .bind(tg_user_id)
        .bind(kind)
        .bind(scope)
        .bind(current_unix_timestamp()?)
        .bind(tg_user_id)
        .bind(STATUS_APPROVED)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Снимает отметки, чтобы уведомления можно было отправить снова: после временной
    /// ошибки доставки или когда расход трафика опустился ниже порога.
    pub async fn forget_user_notifications(
        &self,
        tg_user_id: i64,
        kinds: &[String],
        scope: Option<&str>,
    ) -> Result<u64, anyhow::Error> {
        let mut removed = 0;
        for kind in kinds {
            let r = sqlx::query(
                "DELETE FROM user_notifications
                 WHERE tg_user_id = ? AND kind = ? AND (? IS NULL OR scope = ?)",
            )
            .bind(tg_user_id)
            .bind(kind)
            .bind(scope)
            .bind(scope)
            .execute(&self.pool)
            .await?;
            removed += r.rows_affected();
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use crate::db::test_support::TestDb;

    #[tokio::test]
    async fn claim_user_notification_fires_once_per_kind_and_scope() -> Result<(), anyhow::Error> {
        let fixture = TestDb::new().await?;
        fixture
            .db
            .set_approved(1, Some("alice"), None, "tg_1", "secret", None)
            .await?;
        let db = &fixture.db;

        let first = "2026-10-01T00:00:00Z";
        assert!(
            db.claim_user_notification(1, "expiry_warn_3", first)
                .await?
        );
        assert!(
            !db.claim_user_notification(1, "expiry_warn_3", first)
                .await?
        );
        assert!(
            db.claim_user_notification(1, "expiry_warn_1", first)
                .await?
        );
        // Продление срока — новый scope, уведомление снова разрешено.
        assert!(
            db.claim_user_notification(1, "expiry_warn_3", "2026-11-01T00:00:00Z")
                .await?
        );
        // Пользователя нет в БД.
        assert!(
            !db.claim_user_notification(2, "expiry_warn_3", first)
                .await?
        );

        // Снятая отметка разрешает уведомление снова; чужой scope не трогается.
        let kinds = ["expiry_warn_3".to_string()];
        assert_eq!(
            db.forget_user_notifications(1, &kinds, Some("other"))
                .await?,
            0
        );
        assert_eq!(
            db.forget_user_notifications(1, &kinds, Some(first)).await?,
            1
        );
        assert!(
            db.claim_user_notification(1, "expiry_warn_3", first)
                .await?
        );
        // Без scope снимаются отметки любого срока или квоты.
        assert_eq!(db.forget_user_notifications(1, &kinds, None).await?, 2);

        db.deactivate_user(1).await?;
        assert!(!db.claim_user_notification(1, "expired", first).await?);
        Ok(())
    }
}
