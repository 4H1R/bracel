use sea_orm::TransactionTrait;
use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Batteries;
#[async_trait::async_trait]
impl MigrationTrait for Batteries {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let transaction = manager.get_connection().begin().await?;
        transaction.execute_unprepared(bracel::jobs::SCHEMA).await?;
        transaction
            .execute_unprepared(bracel::tokens::SCHEMA)
            .await?;
        transaction.commit().await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP TABLE bracel_tokens, bracel_job_replays, bracel_jobs, bracel_schedules",
            )
            .await?;
        Ok(())
    }
}
