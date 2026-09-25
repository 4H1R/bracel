use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "CREATE TABLE \"__TABLE__\" (id uuid PRIMARY KEY, owner text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), __COLUMNS__);
             CREATE INDEX \"__TABLE___owner_cursor\" ON \"__TABLE__\" (owner, created_at, id)"
        ).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DROP TABLE \"__TABLE__\"").await?;
        Ok(())
    }
}
