use super::config::Config;
use super::discord;

pub fn get_logger(config: &Config) -> Box<dyn Logger> {
    match config.log_webhook {
        None => Box::new(StdoutLogger),
        Some(ref hook) => Box::new(DiscordLogger::new(hook.clone())),
    }
}

pub trait Logger {
    fn start(&self);
    fn info(&self, msg: &str);
    fn warning(&self, msg: &str);
    fn error(&self, msg: &str);
}

impl Logger for Box<dyn Logger + '_> {
    fn start(&self) {
        (**self).start()
    }

    fn info(&self, msg: &str) {
        (**self).info(msg)
    }

    fn warning(&self, msg: &str) {
        (**self).warning(msg)
    }

    fn error(&self, msg: &str) {
        (**self).error(msg)
    }
}

pub struct StdoutLogger;

impl Logger for StdoutLogger {
    fn start(&self) {}

    fn info(&self, msg: &str) {
        println!("{}", msg);
    }

    fn warning(&self, msg: &str) {
        eprintln!("{}", msg);
    }

    fn error(&self, msg: &str) {
        eprintln!("{}", msg);
    }
}

#[derive(Clone)]
pub struct DiscordLogger {
    log_webhook: String,
}

impl DiscordLogger {
    pub fn new(log_webhook: String) -> DiscordLogger {
        Self { log_webhook }
    }
}

impl Logger for DiscordLogger {
    fn start(&self) {
        let _ = discord::send_message(&self.log_webhook, "```───────────────────────────────────────────────────────────────────────────────────────────────────────────```");
    }

    fn info(&self, msg: &str) {
        println!("{}", msg);

        let _ = discord::send_message(&self.log_webhook, msg);
    }

    fn warning(&self, msg: &str) {
        println!(":yellow_circle: {}", msg);

        let _ = discord::send_message(&self.log_webhook, msg);
    }

    fn error(&self, msg: &str) {
        eprintln!("{}", msg);

        let _ = discord::send_message(
            &self.log_webhook,
            &("<@!106120945231466496> :red_circle:".to_string() + msg),
        );
    }
}
