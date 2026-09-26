#[cfg(feature = "mail")]
use super::application::{hash, secret, sql};
use sea_orm::{ConnectionTrait, DatabaseConnection};
#[cfg(feature = "mail")]
use uuid::Uuid;

/// Initialized once per worker process; credentials and transport stay private.
pub struct MailWorker {
    #[cfg(feature = "mail")]
    mailer: bracel_integrations::mail::Mailer,
    #[cfg(feature = "mail")]
    from: String,
}

impl MailWorker {
    pub fn from_env() -> Result<Self, &'static str> {
        Self::from_lookup(&|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: &impl Fn(&str) -> Option<String>) -> Result<Self, &'static str> {
        let settings = crate::provider_settings::MailSettings::from_lookup(get)
            .map_err(|_| "Invalid account mail configuration")?;
        Self::from_settings(&settings)
    }
    pub fn from_settings(
        settings: &crate::provider_settings::MailSettings,
    ) -> Result<Self, &'static str> {
        #[cfg(not(feature = "mail"))]
        {
            let _ = settings;
            Err("Compile with the mail feature")
        }
        #[cfg(feature = "mail")]
        {
            Ok(Self {
                mailer: settings.build()?,
                from: settings.from.clone(),
            })
        }
    }

    /// Send one intent, prioritizing resets. Only hashed credentials reach PostgreSQL.
    pub async fn tick(&self, db: &DatabaseConnection) -> Result<bool, &'static str> {
        #[cfg(not(feature = "mail"))]
        {
            let _ = db;
            Err("Compile with the mail feature")
        }
        #[cfg(feature = "mail")]
        {
            use bracel_integrations::mail::message;
            // Identifiers and conditions are fixed application constants, never request input.
            for (table, label, subject, instructions, eligible) in [
                (
                    "account_resets",
                    "Reset",
                    "Reset your password",
                    "Submit this token and your new password to POST /api/auth/reset-password using your application.",
                    "TRUE",
                ),
                (
                    "account_verifications",
                    "Verification",
                    "Verify your email",
                    "While signed in to your account, submit this token to POST /api/auth/verify-email using your application.",
                    "u.email=r.email AND u.email_verified_at IS NULL",
                ),
            ] {
                let lease = Uuid::now_v7();
                let token = secret().map_err(|_| "Account credential generation unavailable")?;
                let row = db.query_one_raw(sql(&format!(
                    "UPDATE {table} r SET lease_id=$1,lease_until=clock_timestamp()+interval '30 seconds',attempts=r.attempts+1,token_hash=$2
                    FROM (SELECT r.id FROM {table} r JOIN users u ON u.id=r.user_id
                    WHERE NOT r.delivered AND r.attempts<5 AND r.expires_at>clock_timestamp() AND r.available_at<=clock_timestamp()
                    AND (r.lease_until IS NULL OR r.lease_until<=clock_timestamp()) AND {eligible}
                    ORDER BY r.available_at FOR UPDATE OF r SKIP LOCKED LIMIT 1) candidate
                    WHERE r.id=candidate.id RETURNING r.id,r.user_id"),
                    vec![lease.into(),hash(&token).into()],
                )).await.map_err(|_| "Account mail queue unavailable")?;
                let Some(row) = row else {
                    continue;
                };
                let id: Uuid = row.try_get("", "id").map_err(|_| "Invalid mail intent")?;
                let user_id: Uuid = row
                    .try_get("", "user_id")
                    .map_err(|_| "Invalid mail intent")?;
                let (query, recipient_id) = if table == "account_verifications" {
                    ("SELECT email FROM account_verifications WHERE id=$1", id)
                } else {
                    ("SELECT email FROM users WHERE id=$1", user_id)
                };
                let row = db
                    .query_one_raw(sql(query, vec![recipient_id.into()]))
                    .await
                    .map_err(|_| "Account unavailable")?;
                // A resend or confirmation may have removed this claimed intent.
                let Some(row) = row else {
                    return Ok(true);
                };
                let email: String = row
                    .try_get("", "email")
                    .map_err(|_| "Account unavailable")?;
                let text = format!(
                    "{subject}\n\n{label} token: {token}\n\n{instructions} It expires 15 minutes after the request. If you did not request this, ignore this message."
                );
                let mail = message(&self.from, &email, subject, &text)
                    .map_err(|_| "Invalid account email")?;
                let sent = self.mailer.send(mail).await;
                let permanent = matches!(sent, Err(bracel_integrations::Error::Rejected));
                db.execute_raw(sql(&format!("UPDATE {table} SET delivered=$3,attempts=CASE WHEN $4 THEN 5 ELSE attempts END,lease_id=NULL,lease_until=NULL,available_at=clock_timestamp()+interval '30 seconds' WHERE id=$1 AND lease_id=$2"),vec![id.into(),lease.into(),sent.is_ok().into(),permanent.into()]))
                    .await.map_err(|_| "Account delivery acknowledgement failed")?;
                return Ok(true);
            }
            Ok(false)
        }
    }
}

/// Convenience for a single delivery; long-running callers should retain MailWorker.
pub async fn mail_once(db: &DatabaseConnection) -> Result<bool, &'static str> {
    MailWorker::from_env()?.tick(db).await
}

pub async fn cleanup(db: &DatabaseConnection) -> Result<(), &'static str> {
    for statement in [
        "DELETE FROM account_resets WHERE id IN (SELECT id FROM account_resets WHERE expires_at<=clock_timestamp() LIMIT 1000)",
        "DELETE FROM account_verifications WHERE id IN (SELECT id FROM account_verifications WHERE expires_at<=clock_timestamp() LIMIT 1000)",
        "DELETE FROM account_quotas WHERE key IN (SELECT key FROM account_quotas WHERE expires_at<=clock_timestamp() LIMIT 1000)",
        "DELETE FROM bracel_tokens WHERE id IN (SELECT t.id FROM bracel_tokens t JOIN account_sessions s ON s.token_id=t.id WHERE t.expires_at<=clock_timestamp() OR t.revoked_at IS NOT NULL LIMIT 1000)",
    ] {
        db.execute_unprepared(statement)
            .await
            .map_err(|_| "Account cleanup failed")?;
    }
    Ok(())
}
