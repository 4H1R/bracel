use sea_orm::{DbBackend, Statement, TransactionTrait};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct JobScheduling;
#[async_trait::async_trait]
impl MigrationTrait for JobScheduling {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let tx = manager.get_connection().begin().await?;
        tx.execute_unprepared("ALTER TABLE bracel_jobs ADD COLUMN IF NOT EXISTS queue text NOT NULL DEFAULT 'default', ADD COLUMN IF NOT EXISTS priority integer NOT NULL DEFAULT 0, ADD COLUMN IF NOT EXISTS dispatch_delay_seconds bigint NOT NULL DEFAULT 0; CREATE INDEX IF NOT EXISTS bracel_jobs_queue_due ON bracel_jobs(queue,status,available_at); CREATE TABLE IF NOT EXISTS bracel_calendar(name text PRIMARY KEY,expression text NOT NULL,timezone text NOT NULL,kind text NOT NULL,version integer NOT NULL,payload jsonb NOT NULL,max_attempts integer NOT NULL,next_due_at timestamptz NOT NULL,last_job uuid,enabled boolean NOT NULL DEFAULT true);").await?;
        tx.commit().await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The optional 000004 migration may already own these objects.
        let tx = manager.get_connection().begin().await?;
        let existing = tx.query_one_raw(Statement::from_string(DbBackend::Postgres,
            "SELECT version FROM seaql_migrations WHERE version='m20260926_000004_api_packages'".to_owned())).await?;
        if existing.is_none() {
            tx.execute_unprepared("DROP TABLE bracel_calendar; DROP INDEX bracel_jobs_queue_due; ALTER TABLE bracel_jobs DROP COLUMN queue, DROP COLUMN priority, DROP COLUMN dispatch_delay_seconds;").await?;
        }
        tx.commit().await
    }
}
