use sea_orm::TransactionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct ApiPackages;
#[async_trait::async_trait]
impl MigrationTrait for ApiPackages {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let tx = manager.get_connection().begin().await?;
        tx.execute_unprepared(include_str!("sql/000004_data.sql"))
            .await?;
        tx.execute_unprepared(include_str!("sql/000004_realtime.sql"))
            .await?;
        tx.execute_unprepared(include_str!("sql/000004_queues.sql"))
            .await?;
        tx.execute_unprepared(include_str!("sql/000004_delivery.sql"))
            .await?;
        tx.execute_unprepared(include_str!("sql/000004_files.sql"))
            .await?;
        tx.execute_unprepared("CREATE TABLE projects(id uuid PRIMARY KEY, owner text NOT NULL, document jsonb NOT NULL, version bigint NOT NULL DEFAULT 1, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), deleted_at timestamptz); CREATE INDEX projects_owner_cursor ON projects(owner,created_at,id); CREATE INDEX projects_search ON projects USING gin(to_tsvector('simple',document->>'name'));").await?;
        tx.commit().await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DROP TABLE projects, bracel_attachments, bracel_files, bracel_notifications, bracel_notification_preferences, bracel_mail_delivery, bracel_webhook_inbox, bracel_calendar, bracel_event_consumers, bracel_events, bracel_event_clock, bracel_idempotency, bracel_memberships, bracel_audit; DROP INDEX bracel_jobs_queue_due; ALTER TABLE bracel_jobs DROP COLUMN queue, DROP COLUMN priority, DROP COLUMN dispatch_delay_seconds;").await?;
        Ok(())
    }
}
