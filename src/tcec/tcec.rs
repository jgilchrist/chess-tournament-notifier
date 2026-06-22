use crate::log::Logger;
use super::tcec_pgn;
use super::tcec_pgn::Pgn;
use anyhow::{bail, Result};
use regex::Regex;
use std::fmt::Formatter;
use std::hash::Hasher;

const TCEC_PGN_URL: &str = "https://tcec-chess.com/live.pgn";
pub const TCEC_URL: &str = "https://tcec-chess.com/";
pub const CTV_TCEC_URL: &str = "https://ctv.yoshie2000.de/?provider=tcec";

#[derive(Debug, Clone)]
pub struct EngineName(String);

impl EngineName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    fn normalize(name: &str) -> String {
        let mut name = name.to_ascii_lowercase();

        // v1.2.3
        let version_regex = Regex::new(r" v?(\d+)(\.\d+)?(\.\d+)?$").unwrap();
        name = version_regex.replace_all(&name, "").trim().to_string();

        // 2025a
        let date_version_regex = Regex::new(r" \d{4}[a-zA-Z]").unwrap();
        name = date_version_regex.replace_all(&name, "").trim().to_string();

        name
    }

    pub fn matches(&self, name: &str) -> bool {
        Self::normalize(&self.0).contains(&Self::normalize(name))
    }
}

impl PartialEq for EngineName {
    fn eq(&self, other: &Self) -> bool {
        Self::normalize(&self.0) == Self::normalize(&other.0)
    }
}

impl std::fmt::Display for EngineName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::hash::Hash for EngineName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Self::normalize(&self.0).hash(state);
    }
}

fn get_current_pgn() -> Result<Pgn> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let response = client.get(TCEC_PGN_URL).send()?.error_for_status()?;

    if response.status() != reqwest::StatusCode::OK {
        bail!("Unexpected server response: {}", response.status());
    }

    let pgn_content = response.text()?;

    let pgn_info = tcec_pgn::get_pgn_info(&pgn_content)?;

    Ok(pgn_info)
}

pub fn get_current_game(log: &dyn Logger) -> Result<Option<Pgn>> {
    let pgn_fetch_result = get_current_pgn();

    let Ok(pgn) = pgn_fetch_result else {
        let e = pgn_fetch_result.unwrap_err();

        log.warning(&format!("Unable to fetch PGN {:?}", e));

        return Err(e);
    };

    if !pgn.out_of_book() {
        return Ok(None);
    }

    Ok(Some(pgn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_ignores_version() {
        assert!(EngineName::new("Lunar 2").matches("Lunar"));
        assert!(EngineName::new("Lunar 2.0").matches("Lunar"));
        assert!(EngineName::new("Lunar 2.0.1").matches("Lunar"));
    }

    #[test]
    fn test_matches_ignores_date_version() {
        assert!(EngineName::new("Colossus 2025b").matches("Colossus"));
    }
}
