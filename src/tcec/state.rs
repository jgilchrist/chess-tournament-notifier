use super::tcec_pgn::Pgn;
use anyhow::Result;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

const STATE_FILE: &str = "tcec-state.bin";

pub struct SeenGames {
    state: HashSet<u64>,
    file: File,
}

impl SeenGames {
    pub fn load() -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(STATE_FILE)?;

        let mut contents = String::new();
        _ = file.read_to_string(&mut contents);

        let state = contents
            .lines()
            .map(|l| l.parse::<u64>().expect("Bad state file"))
            .collect();

        Ok(Self { state, file })
    }

    pub fn contains(&self, game: &Pgn) -> bool {
        self.state.contains(&game.as_hash())
    }

    pub fn add(&mut self, game: &Pgn) -> Result<()> {
        self.state.insert(game.as_hash());

        writeln!(&mut self.file, "{}", game.as_hash())?;

        Ok(())
    }
}
