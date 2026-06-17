use super::ccrl_pgn;
use super::ccrl_pgn::Pgn;
use super::log::Logger;
use anyhow::Result;
use regex::Regex;
use std::fmt::Formatter;
use std::hash::Hasher;

const CCRL_LIVE_ROOMS_URL: &str = "https://ccrl.live/broadcasts";

#[derive(Debug, Clone)]
pub struct CcrlLiveRoom {
    code: String,
}

impl CcrlLiveRoom {
    pub fn new(code: &str) -> Self {
        Self { code: code.into() }
    }

    pub fn code(&self) -> String {
        self.code.clone()
    }

    fn ccrl_url(suffix: &str) -> String {
        format!("https://ccrl.live/{suffix}")
    }

    pub fn url(&self) -> String {
        Self::ccrl_url(&self.code)
    }

    pub fn pgn_url(&self) -> String {
        Self::ccrl_url(&format!("{}/pgn", self.code))
    }
}

#[derive(Debug, Clone, Hash)]
pub struct EngineName(String);

impl EngineName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    fn normalized(&self) -> String {
        let mut name = self.0.to_string();
        name = name.to_ascii_lowercase();

        // Remove '64-bit' suffix which is appended to many engine names in CCRL
        name = name
            .strip_suffix("64-bit")
            .map(|s| s.trim().to_string())
            .unwrap_or(name);

        // v1.2.3
        let version_regex = Regex::new(r" v?(\d+)(\.\d+)?(\.\d+)?$").unwrap();
        name = version_regex.replace_all(&name, "").trim().to_string();

        // 2025a
        let date_version_regex = Regex::new(r" \d{4}[a-zA-Z]").unwrap();
        name = date_version_regex.replace_all(&name, "").trim().to_string();

        name
    }
}

impl PartialEq for EngineName {
    fn eq(&self, other: &Self) -> bool {
        self.normalized() == other.normalized()
    }
}

impl std::fmt::Display for EngineName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct CcrlLivePlayer {
    name: EngineName,
}

impl CcrlLivePlayer {
    pub fn new(name: &str) -> Self {
        Self {
            name: EngineName::new(&name),
        }
    }

    pub fn matches(&self, name: &str) -> bool {
        self.name == EngineName::new(&name)
    }
}

impl std::fmt::Display for CcrlLivePlayer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl std::hash::Hash for CcrlLivePlayer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

fn get_active_broadcasts() -> Result<Vec<CcrlLiveRoom>> {
    let response = reqwest::blocking::get(CCRL_LIVE_ROOMS_URL)?.error_for_status()?;

    let rooms = response
        .json::<Vec<u64>>()?
        .iter()
        .map(|r| CcrlLiveRoom::new(&r.to_string()))
        .collect();

    Ok(rooms)
}

fn get_current_pgn(room: &CcrlLiveRoom) -> Result<Option<Pgn>> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let response = client.get(room.pgn_url()).send()?.error_for_status()?;

    if response.status() != reqwest::StatusCode::OK {
        return Ok(None);
    }

    let pgn_content = response.text()?;

    let pgn_info = ccrl_pgn::get_pgn_info(&pgn_content)?;

    Ok(Some(pgn_info))
}

pub fn get_current_games(log: &dyn Logger) -> Result<Vec<(CcrlLiveRoom, Pgn)>> {
    let mut pgns: Vec<(CcrlLiveRoom, Pgn)> = vec![];

    let broadcasts = get_active_broadcasts()?;

    for room in &broadcasts {
        let pgn_fetch_result = get_current_pgn(room);

        let Ok(pgn) = pgn_fetch_result else {
            let e = pgn_fetch_result.unwrap_err();

            log.warning(&format!(
                "Unable to fetch PGN for room {}: {:?}",
                room.code(),
                e
            ));

            continue;
        };

        // We may have no PGN for the room if there's no active broadcast
        let Some(pgn) = pgn else {
            continue;
        };

        // Don't consider games which are still in book to have started since we need all the book
        // moves so we can hash the game correctly
        if !pgn.out_of_book() {
            continue;
        }

        pgns.push((room.clone(), pgn));
    }

    Ok(pgns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_does_not_match_substring() {
        let player = CcrlLivePlayer::new("Lunar");

        assert!(player.matches("Lunar"));
        assert!(!player.matches("Luna"));
    }

    #[test]
    fn test_matches_ignores_version() {
        assert!(CcrlLivePlayer::new("Lunar 2").matches("Lunar"));
        assert!(CcrlLivePlayer::new("Lunar 2.0").matches("Lunar"));
        assert!(CcrlLivePlayer::new("Lunar 2.0.1").matches("Lunar"));
    }

    #[test]
    fn test_matches_ignores_date_version() {
        assert!(CcrlLivePlayer::new("Colossus 2025b").matches("Colossus"));
    }
}
