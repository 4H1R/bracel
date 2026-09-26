use sea_orm::TransactionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Accounts;
#[async_trait::async_trait]
impl MigrationTrait for Accounts {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let tx = manager.get_connection().begin().await?;
        tx.execute_unprepared("CREATE TABLE users (
            id uuid PRIMARY KEY, email text NOT NULL UNIQUE CHECK(email=lower(email)),
            display_name text NOT NULL CHECK(char_length(display_name) BETWEEN 1 AND 100),
            password_hash text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp());
            CREATE TABLE account_sessions (
            token_id uuid PRIMARY KEY REFERENCES bracel_tokens(id) ON DELETE CASCADE,
            user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE);
            CREATE INDEX account_sessions_user ON account_sessions(user_id);
            CREATE TABLE account_resets (
            id uuid PRIMARY KEY, user_id uuid NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
            token_hash text UNIQUE, expires_at timestamptz NOT NULL,
            attempts integer NOT NULL DEFAULT 0, delivered boolean NOT NULL DEFAULT false,
            lease_id uuid, lease_until timestamptz, available_at timestamptz NOT NULL DEFAULT clock_timestamp());
            CREATE TABLE account_quotas (
            key text PRIMARY KEY, attempts integer NOT NULL, expires_at timestamptz NOT NULL);")
            .await?;
        tx.commit().await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP TABLE account_quotas, account_resets, account_sessions, users",
            )
            .await?;
        Ok(())
    }
}
