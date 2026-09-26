mod application;
mod dto;
mod http;
mod mail;

pub use dto::Settings;
pub use http::{registry, router};
pub use mail::{cleanup, mail_once};

pub const ISSUER: &str = "bracel-starter:accounts";
pub const SCOPE: &str = "account:self";
pub const SESSION_SECONDS: i32 = 86400;

pub fn verifier(db: sea_orm::DatabaseConnection) -> bracel::identity::BearerAuth {
    bracel::identity::BearerAuth::pending(ISSUER, "starter-api")
        .expect("static local identity configuration")
        .with_tokens(db)
}
