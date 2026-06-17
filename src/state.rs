use anyhow::Result;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

pub struct SeenGames {
    state: HashSet<u64>,
    file: File,
}

impl SeenGames {
    pub fn load(path: &str) -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)?;

        let mut contents = String::new();
        _ = file.read_to_string(&mut contents);

        let state = contents
            .lines()
            .map(|l| l.parse::<u64>().expect("Bad state file"))
            .collect();

        Ok(Self { state, file })
    }

    pub fn contains(&self, hash: u64) -> bool {
        self.state.contains(&hash)
    }

    pub fn add(&mut self, hash: u64) -> Result<()> {
        self.state.insert(hash);

        writeln!(&mut self.file, "{}", hash)?;

        Ok(())
    }
}
