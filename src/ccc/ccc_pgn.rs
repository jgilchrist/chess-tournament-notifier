use anyhow::{bail, Result};
use regex::Regex;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct Game {
    pub event: String,
    pub round: String,
    pub date: String,
    pub white: String,
    pub black: String,
}

impl Game {
    pub fn has_player(&self, engine: &str) -> bool {
        let engine = normalize(engine);
        normalize(&self.white) == engine || normalize(&self.black) == engine
    }

    pub fn hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.event.hash(&mut hasher);
        self.round.hash(&mut hasher);
        self.date.hash(&mut hasher);
        normalize(&self.white).hash(&mut hasher);
        normalize(&self.black).hash(&mut hasher);
        hasher.finish()
    }
}

fn normalize(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    Regex::new(r" v?\d+(\.\d+)?(\.\d+)?$")
        .unwrap()
        .replace(&name, "")
        .trim()
        .to_owned()
}

pub fn parse(pgn: &str) -> Result<Game> {
    let header =
        Regex::new(r#"(?m)^\[(Event|Date|Round|White|Black)\s+"((?:\\.|[^"])*)"\]\r?$"#).unwrap();
    let mut event = None;
    let mut round = None;
    let mut date = None;
    let mut white = None;
    let mut black = None;
    for capture in header.captures_iter(pgn) {
        let value = capture[2].replace("\\\"", "\"").replace("\\\\", "\\");
        match &capture[1] {
            "Event" => event = Some(value),
            "Round" => round = Some(value),
            "Date" => date = Some(value),
            "White" => white = Some(value),
            "Black" => black = Some(value),
            _ => {}
        }
    }
    let (Some(event), Some(date), Some(white), Some(black)) = (event, date, white, black) else {
        bail!("CCC PGN is missing Event, Date, White, or Black header")
    };
    Ok(Game {
        event,
        round: round.unwrap_or_default(),
        date,
        white,
        black,
    })
}
