use anyhow::Result;
use reqwest::Url;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub struct Config {
    pub config_url: Url,
    pub notify_webhook: String,
    pub log_webhook: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct NotifyConfig {
    pub engines: HashMap<String, HashSet<String>>,
}

impl NotifyConfig {
    pub fn engines_summary(&self) -> String {
        let mut engines: Vec<_> = self.engines.iter().collect();
        engines.sort_by(|(left, _), (right, _)| left.cmp(right));
        engines
            .into_iter()
            .map(|(engine, users)| {
                let mut users: Vec<_> = users.iter().cloned().collect();
                users.sort();
                format!("{}: {}", engine, users.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn diff_summary(&self, new: &NotifyConfig) -> String {
        let old: HashSet<_> = self.engines_summary().lines().map(String::from).collect();
        let updated: HashSet<_> = new.engines_summary().lines().map(String::from).collect();
        let mut lines: Vec<_> = old
            .difference(&updated)
            .map(|line| format!("-{}", line))
            .chain(updated.difference(&old).map(|line| format!("+{}", line)))
            .collect();
        lines.sort();
        if lines.is_empty() {
            "No changes".to_owned()
        } else {
            lines.join("\n")
        }
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    users: HashMap<String, UserConfig>,
}

#[derive(Deserialize)]
struct UserConfig {
    engines: HashSet<String>,
}

pub fn get_config() -> Result<Config> {
    Ok(Config {
        config_url: Url::parse(&std::env::var("CCC_CONFIG_URL")?)?,
        notify_webhook: std::env::var("CCC_NOTIFY_WEBHOOK")?,
        log_webhook: std::env::var("LOG_WEBHOOK").ok(),
    })
}

pub fn get_notify_config(config: &Config) -> Result<NotifyConfig> {
    let response = reqwest::blocking::get(config.config_url.clone())?.error_for_status()?;
    let file = serde_json5::from_str::<ConfigFile>(&response.text()?)?;
    let mut engines = HashMap::<String, HashSet<String>>::new();
    for (user, user_config) in file.users {
        for engine in user_config.engines {
            engines.entry(engine).or_default().insert(user.clone());
        }
    }
    Ok(NotifyConfig { engines })
}
