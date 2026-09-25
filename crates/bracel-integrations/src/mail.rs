use crate::Error;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone)]
pub struct Mailer {
    transport: Transport,
}
#[derive(Clone)]
enum Transport {
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
    Capture(Arc<Mutex<Vec<Message>>>, usize),
}
impl Mailer {
    pub fn relay(host: &str, username: String, password: String) -> Result<Self, Error> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map_err(|_| Error::Configuration)?
            .credentials(Credentials::new(username, password))
            .timeout(Some(Duration::from_secs(5)))
            .build();
        Ok(Self {
            transport: Transport::Smtp(transport),
        })
    }
    /// Plaintext is limited to numeric loopback for a local capture service.
    pub fn local(port: u16) -> Self {
        Self {
            transport: Transport::Smtp(
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("127.0.0.1")
                    .port(port)
                    .timeout(Some(Duration::from_secs(5)))
                    .build(),
            ),
        }
    }
    pub fn capture(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 || capacity > 1000 {
            return Err(Error::Configuration);
        }
        Ok(Self {
            transport: Transport::Capture(Arc::new(Mutex::new(Vec::new())), capacity),
        })
    }
    pub fn captured(&self) -> Result<Vec<Message>, Error> {
        match &self.transport {
            Transport::Capture(messages, _) => {
                Ok(messages.lock().map_err(|_| Error::Unavailable)?.clone())
            }
            _ => Err(Error::Configuration),
        }
    }
    pub async fn send(&self, message: Message) -> Result<(), Error> {
        if message.formatted().len() > 1024 * 1024 {
            return Err(Error::TooLarge);
        }
        match &self.transport {
            Transport::Capture(messages, capacity) => {
                let mut messages = messages.lock().map_err(|_| Error::Unavailable)?;
                if messages.len() >= *capacity {
                    return Err(Error::TooLarge);
                }
                messages.push(message);
                Ok(())
            }
            Transport::Smtp(smtp) => {
                tokio::time::timeout(Duration::from_secs(6), smtp.send(message))
                    .await
                    .map_err(|_| Error::Unavailable)?
                    .map_err(|error| {
                        if error.is_permanent() {
                            Error::Rejected
                        } else {
                            Error::Unavailable
                        }
                    })?;
                Ok(())
            }
        }
    }
}
/// Plain text plus an escaped HTML alternative. Addresses are parsed, never concatenated into headers.
pub fn message(from: &str, to: &str, subject: &str, text: &str) -> Result<Message, Error> {
    if text.len() > 512 * 1024 || subject.len() > 200 || subject.contains(['\r', '\n']) {
        return Err(Error::InvalidInput);
    }
    let from = from.parse::<Mailbox>().map_err(|_| Error::InvalidInput)?;
    let to = to.parse::<Mailbox>().map_err(|_| Error::InvalidInput)?;
    let html = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('\n', "<br>");
    Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::plain(text.to_string()))
                .singlepart(SinglePart::html(format!("<p>{html}</p>"))),
        )
        .map_err(|_| Error::InvalidInput)
}
