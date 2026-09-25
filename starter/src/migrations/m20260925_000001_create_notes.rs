use sea_orm_migration::prelude::*;

pub struct CreateNotes;
impl MigrationName for CreateNotes {
    fn name(&self) -> &str {
        "m20260925_000001_create_notes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for CreateNotes {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "CREATE TABLE notes (id uuid PRIMARY KEY, title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200 AND title = btrim(title)))"
        ).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("notes")).to_owned())
            .await
    }
}
