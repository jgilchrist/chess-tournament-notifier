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

impl NotifyConfig {
    pub fn engines_summary(&self) -> String {
        fn format_group(engines: &HashMap<String, HashSet<String>>) -> String {
            let mut engines: Vec<_> = engines.iter().collect();
            engines.sort_by_key(|(name, _)| name.clone());

            engines
                .into_iter()
                .map(|(engine, authors)| {
                    let mut authors: Vec<_> = authors.iter().cloned().collect();
                    authors.sort();
                    format!("{}: {}", engine, authors.join(", "))
                })
                .collect::<Vec<_>>()
                .join("\n")
        }

        format!(
            "TCEC:\n{}\nAlphaBeta:\n{}",
            format_group(&self.tcec_engines),
            format_group(&self.alphabeta_engines)
        )
    }

    pub fn diff_summary(&self, new: &NotifyConfig) -> String {
        let old_lines: HashSet<String> = self.engines_summary().lines().map(String::from).collect();
        let new_lines: HashSet<String> = new.engines_summary().lines().map(String::from).collect();

        let mut removed: Vec<_> = old_lines.difference(&new_lines).cloned().collect();
        let mut added: Vec<_> = new_lines.difference(&old_lines).cloned().collect();
        removed.sort();
        added.sort();

        let lines: Vec<String> = removed
            .into_iter()
            .map(|line| format!("-{}", line))
            .chain(added.into_iter().map(|line| format!("+{}", line)))
            .collect();

        if lines.is_empty() {
            "No changes".to_string()
        } else {
            lines.join("\n")
        }
    }
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
