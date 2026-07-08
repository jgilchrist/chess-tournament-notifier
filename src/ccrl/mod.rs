use self::db::CcrlDb;
use self::notify::NotifyContent;
use crate::log::Logger;
use anyhow::Result;
use std::collections::HashSet;
use std::time::Duration;

mod ccrl_pgn;
mod ccrllive;
mod config;
mod db;
mod notify;

const POLL_DELAY: Duration = Duration::from_secs(30);

pub fn run() -> Result<()> {
    let config = config::get_config().expect("Unable to load config");
    let log = crate::log::get_logger(config.log_webhook.clone(), "ccrl");

    log.start();

    let mut first_run = true;

    let db = CcrlDb::open().expect("Unable to open database");
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

        let current_games_result = ccrllive::get_current_games(&log);

        let Ok(current_games) = current_games_result else {
            let e = current_games_result.unwrap_err();

            log.warning(&format!("Unable to fetch in-progress games: {:?}", e));

            std::thread::sleep(POLL_DELAY);
            continue;
        };

        if first_run {
            for (room, game) in &current_games {
                log.info(&format!(
                    "`{}` In progress: `{}` vs `{}` ({} plies)",
                    room.code(),
                    game.white_player,
                    game.black_player,
                    game.moves.len()
                ))
            }

            first_run = false;
        }

        let new_games = current_games
            .iter()
            .filter(|(_, game)| !db.contains(game).unwrap_or(false))
            .collect::<Vec<_>>();

        for (room, game) in &new_games {
            log.info(&format!(
                "`{}` - `{}` vs `{}`",
                room.code(),
                game.white_player,
                game.black_player,
            ));

            let mut mentions = HashSet::new();

            for (engine, user_configs) in &notify_config.engines {
                if game.has_player(engine) {
                    let matching_users: Vec<String> = user_configs
                        .iter()
                        .filter(|user_config| user_config.rules.notify_for_tournament(&game.site))
                        .map(|user_config| user_config.user_id.clone())
                        .collect();

                    if !matching_users.is_empty() {
                        mentions.extend(matching_users.iter().cloned());
                        log.info(&format!(
                            "`{}` Will notify {} users for engine `{}`",
                            room.code(),
                            matching_users.len(),
                            &engine,
                        ));
                    }
                }
            }

            if !mentions.is_empty() {
                let notify_result = notify::notify(
                    &config,
                    NotifyContent {
                        white_player: game.white_player.clone(),
                        black_player: game.black_player.clone(),
                        tournament: game.site.clone(),
                        room: room.clone(),
                        mentions,
                    },
                );

                if let Err(e) = notify_result {
                    log.error(&format!("Unable to send notify: {:?}", e));
                }
            }

            if let Err(e) = db.add(&game) {
                log.error(&format!("Unable to write seen game to db: {:?}", e));
            }
        }

        std::thread::sleep(POLL_DELAY);
    }
}
