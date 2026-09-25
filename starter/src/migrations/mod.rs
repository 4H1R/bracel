mod m20260925_000001_create_notes;
mod m20260925_000002_note_created_at;
mod m20260926_000003_batteries;
// bracel:generated-migrations

use sea_orm_migration::prelude::*;

pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260925_000001_create_notes::CreateNotes),
            Box::new(m20260925_000002_note_created_at::NoteCreatedAt),
            Box::new(m20260926_000003_batteries::Batteries),
            // bracel:generated-migration-list
        ]
    }
}
