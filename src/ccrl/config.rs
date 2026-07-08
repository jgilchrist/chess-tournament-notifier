use anyhow::Result;
use regex::Regex;
use reqwest::Url;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub struct Config {
    pub config_url: Url,
    pub notify_webhook: String,
    pub log_webhook: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotifyRule {
    #[serde(with = "serde_regex")]
    pub pattern: Regex,
    pub action: NotifyAction,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyAction {
    Notify,
    Ignore,
}

#[derive(Debug, Clone)]
pub struct TournamentRules {
    pub rules: Vec<NotifyRule>,
}

impl TournamentRules {
    pub fn notify_for_tournament(&self, tournament_name: &str) -> bool {
        // Evaluate rules in order - first matching rule wins
        for rule in &self.rules {
            if rule.pattern.is_match(tournament_name) {
                return rule.action == NotifyAction::Notify;
            }
        }

        // If no rules matched, allow by default
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserNotifyConfig {
    pub user_id: String,
    pub rules: TournamentRules,
}

#[derive(Debug, PartialEq)]
pub struct NotifyConfig {
    pub engines: HashMap<String, Vec<UserNotifyConfig>>,
}

impl NotifyConfig {
    pub fn engines_summary(&self) -> String {
        let mut engines: Vec<_> = self.engines.iter().collect();
        engines.sort_by_key(|(name, _)| name.clone());

        engines
            .into_iter()
            .map(|(engine, users)| {
                let mut authors: Vec<_> = users.iter().map(|u| u.user_id.clone()).collect();
                authors.sort();
                format!("{}: {}", engine, authors.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n")
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

impl PartialEq for TournamentRules {
    fn eq(&self, other: &Self) -> bool {
        self.rules.len() == other.rules.len()
            && self
                .rules
                .iter()
                .zip(other.rules.iter())
                .all(|(a, b)| a.pattern.as_str() == b.pattern.as_str() && a.action == b.action)
    }
}

#[derive(Deserialize)]
struct UserConfig {
    pub engines: Vec<String>,
    #[serde(default)]
    pub rules: Vec<NotifyRule>,
}

#[derive(Deserialize)]
struct ConfigFile {
    pub users: HashMap<String, UserConfig>,
}

pub fn get_config() -> Result<Config> {
    let config_url = std::env::var("CCRL_CONFIG_URL")?;
    let notify_webhook = std::env::var("CCRL_NOTIFY_WEBHOOK")?;
    let log_webhook = std::env::var("LOG_WEBHOOK").ok();

    Ok(Config {
        config_url: Url::parse(&config_url)?,
        notify_webhook,
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

    let mut engines_to_users: HashMap<String, Vec<UserNotifyConfig>> = HashMap::new();

    for (user, user_config) in &config_file.users {
        let tournament_rules = TournamentRules {
            rules: user_config.rules.clone(),
        };

        let user_notify_config = UserNotifyConfig {
            user_id: user.clone(),
            rules: tournament_rules,
        };

        // Add engines with user config
        for engine in &user_config.engines {
            engines_to_users
                .entry(engine.clone())
                .or_default()
                .push(user_notify_config.clone());
        }
    }

    Ok(NotifyConfig {
        engines: engines_to_users,
    })
}
