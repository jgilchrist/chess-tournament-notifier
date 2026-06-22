use crate::discord;
use super::tcec::{EngineName, CTV_TCEC_URL, TCEC_URL};
use anyhow::Result;
use std::collections::HashSet;
use crate::tcec::config::Config;

pub struct NotifyContent {
    pub white_player: EngineName,
    pub black_player: EngineName,
    pub tournament: String,
    pub mentions: HashSet<String>,
}

pub fn notify_tcec(config: &Config, content: NotifyContent) -> Result<()> {
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
        &config.tcec_notify_webhook,
        "tcec-notifier",
        &format!(
            "[`{}`]({}) `{}` vs. `{}`{}",
            content.tournament, TCEC_URL, content.white_player, content.black_player, mentions_str
        ),
    )
}

pub fn notify_alphabeta(config: &Config, content: NotifyContent) -> Result<()> {
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
        &config.alphabeta_notify_webhook,
        "tcec-notifier",
        &format!(
            "[`{}`]({}) `{}` vs. `{}`{}",
            content.tournament, CTV_TCEC_URL, content.white_player, content.black_player, mentions_str
        ),
    )
}
