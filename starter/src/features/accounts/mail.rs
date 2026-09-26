#[cfg(feature = "mail")]
use super::application::{hash, secret, sql};
use sea_orm::{ConnectionTrait, DatabaseConnection};
#[cfg(feature = "mail")]
use uuid::Uuid;

/// A worker creates credentials only in memory; pending delivery stores no reset secret.
pub async fn mail_once(db: &DatabaseConnection) -> Result<bool, &'static str> {
    #[cfg(not(feature = "mail"))]
    {
        let _ = db;
        Err("Compile with the mail feature")
    }
    #[cfg(feature = "mail")]
    {
        use bracel_integrations::mail::{Mailer, message};
        let get = |key: &str| std::env::var(key).ok();
        let settings =
            super::Settings::from_lookup(&get).map_err(|_| "Invalid account mail configuration")?;
        if !settings.mail_configured {
            return Err("Configure SMTP before running the account mail worker");
        }
        let mailer = if let Some(port) = get("MAIL_LOCAL_PORT") {
            Mailer::local(port.parse().map_err(|_| "Invalid local SMTP port")?)
        } else {
            Mailer::relay(
                &get("MAIL_SMTP_HOST").ok_or("SMTP host required")?,
                get("MAIL_SMTP_USERNAME").ok_or("SMTP username required")?,
                get("MAIL_SMTP_PASSWORD").ok_or("SMTP password required")?,
            )
            .map_err(|_| "Invalid SMTP configuration")?
        };
        let from = get("MAIL_FROM").unwrap_or_else(|| "Bracel <noreply@example.test>".into());
        let lease = Uuid::now_v7();
        let token = secret().map_err(|_| "Reset credential generation unavailable")?;
        let row=db.query_one_raw(sql("UPDATE account_resets r SET lease_id=$1,lease_until=clock_timestamp()+interval '30 seconds',attempts=r.attempts+1,token_hash=$2
            FROM (SELECT id FROM account_resets WHERE NOT delivered AND attempts<5 AND expires_at>clock_timestamp() AND available_at<=clock_timestamp() AND (lease_until IS NULL OR lease_until<=clock_timestamp()) ORDER BY available_at FOR UPDATE SKIP LOCKED LIMIT 1) candidate
            WHERE r.id=candidate.id RETURNING r.id,r.user_id",vec![lease.into(),hash(&token).into()])).await.map_err(|_| "Reset queue unavailable")?;
        let Some(row) = row else {
            return Ok(false);
        };
        let id: Uuid = row.try_get("", "id").map_err(|_| "Invalid reset intent")?;
        let user_id: Uuid = row
            .try_get("", "user_id")
            .map_err(|_| "Invalid reset intent")?;
        let user = db
            .query_one_raw(sql(
                "SELECT email FROM users WHERE id=$1",
                vec![user_id.into()],
            ))
            .await
            .map_err(|_| "Account unavailable")?
            .ok_or("Account unavailable")?;
        let email: String = user
            .try_get("", "email")
            .map_err(|_| "Account unavailable")?;
        let text = format!(
            "A password reset was requested for your account.\n\nReset token: {token}\n\nSubmit this token and your new password to POST /api/auth/reset-password using your application. It expires 15 minutes after the request. If you did not request this, ignore this message."
        );
        let mail = message(&from, &email, "Reset your password", &text)
            .map_err(|_| "Invalid reset email")?;
        let sent = mailer.send(mail).await;
        let permanent = matches!(sent, Err(bracel_integrations::Error::Rejected));
        db.execute_raw(sql("UPDATE account_resets SET delivered=$3,attempts=CASE WHEN $4 THEN 5 ELSE attempts END,lease_id=NULL,lease_until=NULL,available_at=clock_timestamp()+interval '30 seconds' WHERE id=$1 AND lease_id=$2",vec![id.into(),lease.into(),sent.is_ok().into(),permanent.into()])).await.map_err(|_| "Reset delivery acknowledgement failed")?;
        Ok(true)
    }
}

pub async fn cleanup(db: &DatabaseConnection) -> Result<(), &'static str> {
    for statement in [
        "DELETE FROM account_resets WHERE id IN (SELECT id FROM account_resets WHERE expires_at<=clock_timestamp() LIMIT 1000)",
        "DELETE FROM account_quotas WHERE key IN (SELECT key FROM account_quotas WHERE expires_at<=clock_timestamp() LIMIT 1000)",
        "DELETE FROM bracel_tokens WHERE id IN (SELECT t.id FROM bracel_tokens t JOIN account_sessions s ON s.token_id=t.id WHERE t.expires_at<=clock_timestamp() OR t.revoked_at IS NOT NULL LIMIT 1000)",
    ] {
        db.execute_unprepared(statement)
            .await
            .map_err(|_| "Account cleanup failed")?;
    }
    Ok(())
}
