mod application;
mod auth;
mod dto;
mod http;
mod mail;
mod settings;

pub use auth::CurrentUser;
pub use dto::User;
pub(crate) use http::{decorate, registry_with_policies};
pub use http::{registry, router};
pub use mail::{MailWorker, cleanup, mail_once};
pub use settings::Settings;

pub const ISSUER: &str = "bracel-starter:accounts";
pub const SCOPE: &str = "account:self";
pub const SESSION_SECONDS: i32 = 86400;

pub fn verifier(db: sea_orm::DatabaseConnection) -> bracel::identity::BearerAuth {
    bracel::identity::BearerAuth::pending(ISSUER, "starter-api")
        .expect("static local identity configuration")
        .with_tokens(db)
}
