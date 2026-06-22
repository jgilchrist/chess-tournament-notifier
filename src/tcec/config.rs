use anyhow::Result;
use reqwest::Url;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub struct Config {
    pub config_url: Url,
    pub tcec_notify_webhook: String,
    pub alphabeta_notify_webhook: String,
    pub log_webhook: Option<String>,
}

#[derive(Debug)]
pub struct NotifyConfig {
    pub tcec_engines: HashMap<String, HashSet<String>>,
    pub alphabeta_engines: HashMap<String, HashSet<String>>,
}

#[derive(Deserialize)]
struct ConfigFile {
    pub tcec_users: HashMap<String, HashSet<String>>,
    pub alphabeta_users: HashMap<String, HashSet<String>>,
}

pub fn get_config() -> Result<Config> {
    let config_url = std::env::var("TCEC_CONFIG_URL")?;
    let tcec_notify_webhook = std::env::var("TCEC_NOTIFY_WEBHOOK")?;
    let alphabeta_notify_webhook = std::env::var("ALPHABETA_NOTIFY_WEBHOOK")?;
    let log_webhook = std::env::var("LOG_WEBHOOK").ok();

    Ok(Config {
        config_url: Url::parse(&config_url)?,
        tcec_notify_webhook,
        alphabeta_notify_webhook,
        log_webhook,
    })
}

pub fn get_notify_config(config: &Config) -> Result<NotifyConfig> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let response = client
        .get(config.config_url.clone())
        .send()?
        .error_for_status()?;

    let config_file_contents = response.text()?;

    let config_file = serde_json5::from_str::<ConfigFile>(&config_file_contents)?;

    let mut tcec_engines_to_users: HashMap<String, HashSet<String>> = HashMap::new();

    for (user, engines) in &config_file.tcec_users {
        for engine in engines {
            tcec_engines_to_users
                .entry(engine.clone())
                .or_default()
                .insert(user.clone());
        }
    }

    let mut alphabeta_engines_to_users: HashMap<String, HashSet<String>> = HashMap::new();

    for (user, engines) in &config_file.alphabeta_users {
        for engine in engines {
            alphabeta_engines_to_users
                .entry(engine.clone())
                .or_default()
                .insert(user.clone());
        }
    }

    Ok(NotifyConfig {
        tcec_engines: tcec_engines_to_users,
        alphabeta_engines: alphabeta_engines_to_users,
    })
}
