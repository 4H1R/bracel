use sea_orm::TransactionTrait;
use sea_orm_migration::prelude::*;

pub struct NoteCreatedAt;
impl MigrationName for NoteCreatedAt {
    fn name(&self) -> &str {
        "m20260925_000002_note_created_at"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for NoteCreatedAt {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let transaction = manager.get_connection().begin().await?;
        // Existing rows share the migration timestamp: their original creation
        // times were not stored. New inserts receive the database clock time.
        transaction.execute_unprepared("ALTER TABLE notes ADD COLUMN created_at timestamptz NOT NULL DEFAULT statement_timestamp()").await?;
        transaction
            .execute_unprepared(
                "ALTER TABLE notes ALTER COLUMN created_at SET DEFAULT clock_timestamp()",
            )
            .await?;
        transaction
            .execute_unprepared(
                "CREATE INDEX notes_created_at_id_idx ON notes (created_at DESC, id DESC)",
            )
            .await?;
        transaction.commit().await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE notes DROP COLUMN created_at")
            .await?;
        Ok(())
    }
}
