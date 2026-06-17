use self::config::NotifyConfig;
use self::notify::NotifyContent;
use crate::log::Logger;
use crate::state::SeenGames;
use anyhow::Result;
use std::cmp::PartialEq;
use std::collections::HashSet;
use std::time::Duration;

mod config;
mod notify;
mod tcec;
mod tcec_pgn;

const POLL_DELAY: Duration = Duration::from_secs(30);

impl PartialEq for NotifyConfig {
    fn eq(&self, other: &Self) -> bool {
        self.engines == other.engines
    }
}

pub fn run() -> Result<()> {
    let config = config::get_config().expect("Unable to load config");
    let log = crate::log::get_logger(&config, "tcec-notifier");

    log.start();

    let mut first_run = true;

    let mut seen_games = SeenGames::load("tcec-state.bin").expect("Unable to load state");
    let mut notify_config = config::get_notify_config(&config).expect("Unable to load config");

    log.info(&format!("Loaded config: {:?}", notify_config));

    loop {
        let new_notify_config = config::get_notify_config(&config);
        if let Err(e) = new_notify_config {
            log.warning(&format!("Unable to fetch new config: {:?}", e));
        } else {
            let new_notify_config = new_notify_config?;
            if notify_config != new_notify_config {
                log.info(&format!(
                    "<@!106120945231466496> Config update loaded: {:?}",
                    new_notify_config
                ));
                notify_config = new_notify_config;
            }
        }

        let current_game_result = tcec::get_current_game(&log);

        let Ok(current_game) = current_game_result else {
            let e = current_game_result.unwrap_err();

            log.warning(&format!("Unable to fetch in-progress game: {:?}", e));

            std::thread::sleep(POLL_DELAY);
            continue;
        };

        let Some(game) = current_game else {
            std::thread::sleep(POLL_DELAY);
            continue;
        };

        if first_run {
            log.info(&format!(
                "In progress: `{}` vs `{}` ({} plies)",
                game.white_player,
                game.black_player,
                game.moves.len()
            ));

            first_run = false;
        }

        if seen_games.contains(game.as_hash()) {
            std::thread::sleep(POLL_DELAY);
            continue;
        }

        log.info(&format!(
            "`{}` vs `{}`",
            game.white_player, game.black_player,
        ));

        let mut mentions = HashSet::new();

        for (engine, notifies) in &notify_config.engines {
            if game.has_player(engine) {
                mentions.extend(notifies.iter().cloned());
                log.info(&format!(
                    "Will notify {} users for engine `{}`",
                    notifies.len(),
                    &engine,
                ));
            }
        }

        let notify_result = notify::notify(
            &config,
            NotifyContent {
                tournament: game.event.clone(),
                white_player: game.white_player.clone(),
                black_player: game.black_player.clone(),
                mentions,
            },
        );

        if let Err(e) = notify_result {
            log.error(&format!("Unable to send notify: {:?}", e));
        }

        let write_state_result = seen_games.add(game.as_hash());

        if let Err(e) = write_state_result {
            log.error(&format!("Unable to write seen game to file: {:?}", e));
        }

        std::thread::sleep(POLL_DELAY);
    }
}
