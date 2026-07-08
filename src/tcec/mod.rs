use self::config::NotifyConfig;
use self::db::TcecDb;
use self::notify::NotifyContent;
use crate::log::Logger;
use anyhow::Result;
use std::cmp::PartialEq;
use std::collections::HashSet;
use std::time::Duration;

mod config;
mod db;
mod notify;
mod tcec;
mod tcec_pgn;

const POLL_DELAY: Duration = Duration::from_secs(30);

impl PartialEq for NotifyConfig {
    fn eq(&self, other: &Self) -> bool {
        self.tcec_engines == other.tcec_engines
    }
}

pub fn run() -> Result<()> {
    let config = config::get_config().expect("Unable to load config");
    let log = crate::log::get_logger(config.log_webhook.clone(), "tcec");

    log.start();

    let mut first_run = true;

    let db = TcecDb::open().expect("Unable to open database");
    let mut notify_config = config::get_notify_config(&config).expect("Unable to load config");

    log.info(&format!("Loaded config:\n{}", notify_config.engines_summary()));

    loop {
        let new_notify_config = config::get_notify_config(&config);
        if let Err(e) = new_notify_config {
            log.warning(&format!("Unable to fetch new config: {:?}", e));
        } else {
            let new_notify_config = new_notify_config?;
            if notify_config != new_notify_config {
                log.info(&format!(
                    "<@!106120945231466496> Config update loaded:\n```diff\n{}\n```",
                    notify_config.diff_summary(&new_notify_config)
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

        if db.contains(&game).unwrap_or(false) {
            std::thread::sleep(POLL_DELAY);
            continue;
        }

        log.info(&format!(
            "`{}` vs `{}`",
            game.white_player, game.black_player,
        ));

        let mut tcec_mentions = HashSet::new();
        let mut alphabeta_mentions = HashSet::new();

        for (engine, notifies) in &notify_config.tcec_engines {
            if game.has_player(engine) {
                tcec_mentions.extend(notifies.iter().cloned());
                log.info(&format!(
                    "Will notify {} users for engine `{}` in TCEC",
                    notifies.len(),
                    &engine,
                ));
            }
        }

        for (engine, notifies) in &notify_config.alphabeta_engines {
            if game.has_player(engine) {
                alphabeta_mentions.extend(notifies.iter().cloned());
                log.info(&format!(
                    "Will notify {} users for engine `{}` in AlphaBeta",
                    notifies.len(),
                    &engine,
                ));
            }
        }

        let tcec_notify_result = notify::notify_tcec(
            &config,
            NotifyContent {
                tournament: game.event.clone(),
                white_player: game.white_player.clone(),
                black_player: game.black_player.clone(),
                mentions: tcec_mentions,
            },
        );

        if let Err(e) = tcec_notify_result {
            log.error(&format!("Unable to send notify: {:?}", e));
        }

        let alphabeta_notify_result = notify::notify_alphabeta(
            &config,
            NotifyContent {
                tournament: game.event.clone(),
                white_player: game.white_player.clone(),
                black_player: game.black_player.clone(),
                mentions: alphabeta_mentions,
            },
        );

        if let Err(e) = alphabeta_notify_result {
            log.error(&format!("Unable to send notify: {:?}", e));
        }

        if let Err(e) = db.add(&game) {
            log.error(&format!("Unable to write seen game to db: {:?}", e));
        }

        std::thread::sleep(POLL_DELAY);
    }
}
