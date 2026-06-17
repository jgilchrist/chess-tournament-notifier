use super::config::Config;
use super::discord;
use super::tcec::{EngineName, TCEC_URL};
use anyhow::Result;
use std::collections::HashSet;

pub struct NotifyContent {
    pub white_player: EngineName,
    pub black_player: EngineName,
    pub tournament: String,
    pub mentions: HashSet<String>,
}

pub fn notify(config: &Config, content: NotifyContent) -> Result<()> {
    let mentions_str = if !content.mentions.is_empty() {
        "   cc. ".to_string()
            + content
                .mentions
                .iter()
                .map(|m| format!("<@!{}>", m))
                .collect::<Vec<_>>()
                .join(" ")
                .as_str()
    } else {
        String::new()
    };

    discord::send_message(
        &config.notify_webhook,
        &format!(
            "[`{}`]({}) `{}` vs. `{}`{}",
            content.tournament, TCEC_URL, content.white_player, content.black_player, mentions_str
        ),
    )
}
