mod m20260925_000001_create_notes;
mod m20260925_000002_note_created_at;
mod m20260926_000003_batteries;
mod m20260926_000004_api_packages;
mod m20260926_000005_accounts;
mod m20260927_000006_job_scheduling;
mod m20260927_000007_email_verification;
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
            Box::new(m20260926_000004_api_packages::ApiPackages),
            Box::new(m20260926_000005_accounts::Accounts),
            Box::new(m20260927_000006_job_scheduling::JobScheduling),
            Box::new(m20260927_000007_email_verification::EmailVerification),
            // bracel:generated-migration-list
        ]
    }
}
