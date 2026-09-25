use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin};
type Handler<C> = Box<
    dyn Fn(C, Vec<String>) -> Pin<Box<dyn Future<Output = Result<Value, &'static str>> + Send>>
        + Send
        + Sync,
>;
#[derive(Clone, Serialize)]
pub struct CommandInfo {
    pub name: &'static str,
    pub summary: &'static str,
    pub arguments: &'static [&'static str],
}
pub struct Commands<C> {
    commands: BTreeMap<&'static str, (CommandInfo, Handler<C>)>,
}
impl<C> Default for Commands<C> {
    fn default() -> Self {
        Self {
            commands: BTreeMap::new(),
        }
    }
}
impl<C> Commands<C> {
    pub fn register<F, Fut>(&mut self, info: CommandInfo, handler: F) -> Result<(), &'static str>
    where
        F: Fn(C, Vec<String>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, &'static str>> + Send + 'static,
    {
        if info.name.is_empty() || self.commands.contains_key(info.name) {
            return Err("Invalid or duplicate command");
        }
        self.commands.insert(
            info.name,
            (info, Box::new(move |c, a| Box::pin(handler(c, a)))),
        );
        Ok(())
    }
    pub fn manifest(&self) -> Vec<&CommandInfo> {
        self.commands.values().map(|(i, _)| i).collect()
    }
    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
    pub async fn run(
        &self,
        name: &str,
        args: Vec<String>,
        context: C,
    ) -> Result<Value, &'static str> {
        let (info, handler) = self.commands.get(name).ok_or("Unknown command")?;
        if args.len() != info.arguments.len() {
            return Err("Incorrect command arguments; see commands for usage");
        }
        handler(context, args).await
    }
}
