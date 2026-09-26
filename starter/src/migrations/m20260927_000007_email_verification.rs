use sea_orm::TransactionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct EmailVerification;

#[async_trait::async_trait]
impl MigrationTrait for EmailVerification {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let tx = manager.get_connection().begin().await?;
        tx.execute_unprepared("ALTER TABLE users ADD COLUMN email_verified_at timestamptz;
            CREATE TABLE account_verifications (
            id uuid PRIMARY KEY, user_id uuid NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
            email text NOT NULL, token_hash text UNIQUE, expires_at timestamptz NOT NULL,
            attempts integer NOT NULL DEFAULT 0, delivered boolean NOT NULL DEFAULT false,
            lease_id uuid, lease_until timestamptz, available_at timestamptz NOT NULL DEFAULT clock_timestamp());")
            .await?;
        tx.commit().await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let tx = manager.get_connection().begin().await?;
        tx.execute_unprepared(
            "DROP TABLE account_verifications; ALTER TABLE users DROP COLUMN email_verified_at;",
        )
        .await?;
        tx.commit().await
    }
}
