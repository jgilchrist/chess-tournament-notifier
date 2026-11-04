use self::db::CccDb;
use crate::log::Logger;
use anyhow::Result;
use std::collections::HashSet;
use std::time::Duration;

mod ccc_pgn;
mod config;
mod db;
mod notify;
mod websocket;

const POLL_DELAY: Duration = Duration::from_secs(30);

pub fn run() -> Result<()> {
    let config = config::get_config().expect("Unable to load CCC config");
    let log = crate::log::get_logger(config.log_webhook.clone(), "ccc");
    log.start();

    let db = CccDb::open().expect("Unable to open CCC database");
    let mut notify_config = config::get_notify_config(&config).expect("Unable to load CCC config");
    log.info(&format!(
        "Loaded config:\n{}",
        notify_config.engines_summary()
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut first_run = true;

    loop {
        match config::get_notify_config(&config) {
            Ok(updated) if notify_config != updated => {
                log.info(&format!(
                    "<@!106120945231466496> Config update loaded:\n```diff\n{}\n```",
                    notify_config.diff_summary(&updated)
                ));
                notify_config = updated;
            }
            Ok(_) => {}
            Err(e) => log.warning(&format!("Unable to fetch new config: {:?}", e)),
        }

        match runtime.block_on(websocket::get_current_games()) {
            Ok(games) => {
                for game in games {
                    if first_run {
                        log.info(&format!(
                            "In progress: `{}` vs `{}`",
                            game.white, game.black
                        ));
                    }
                    if db.contains(&game).unwrap_or(false) {
                        continue;
                    }

                    log.info(&format!("`{}` vs `{}`", game.white, game.black));
                    let mut mentions = HashSet::new();
                    for (engine, users) in &notify_config.engines {
                        if game.has_player(engine) {
                            mentions.extend(users.iter().cloned());
                            log.info(&format!(
                                "Will notify {} users for engine `{}` in CCC",
                                users.len(),
                                engine
                            ));
                        }
                    }

                    if !mentions.is_empty() {
                        if let Err(e) = notify::notify(&config, &game, mentions) {
                            log.error(&format!("Unable to send notify: {:?}", e));
                        }
                    }
                    if let Err(e) = db.add(&game) {
                        log.error(&format!("Unable to write seen game to db: {:?}", e));
                    }
                }
                first_run = false;
            }
            Err(e) => log.warning(&format!("Unable to fetch CCC live games: {:?}", e)),
        }

        std::thread::sleep(POLL_DELAY);
    }
}
