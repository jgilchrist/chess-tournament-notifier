use super::{ccc_pgn::Game, config::Config};
use crate::discord;
use anyhow::Result;
use std::collections::HashSet;

const CCC_URL: &str = "https://ctv.yoshie2000.de/?provider=ccc";

pub fn notify(config: &Config, game: &Game, mentions: HashSet<String>) -> Result<()> {
    let mentions = if mentions.is_empty() {
        String::new()
    } else {
        format!(
            "   cc. {}",
            mentions
                .iter()
                .map(|id| format!("<@!{}>", id))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    discord::send_message(
        &config.notify_webhook,
        "ccc-notifier",
        &format!(
            "[`{}`]({}) `{}` vs. `{}`{}",
            game.event, CCC_URL, game.white, game.black, mentions
        ),
    )
}
