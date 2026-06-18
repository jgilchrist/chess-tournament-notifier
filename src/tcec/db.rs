use anyhow::Result;
use crate::db::Db;
use super::tcec_pgn::Pgn;

pub struct TcecDb(Db);

impl TcecDb {
    pub fn open() -> Result<Self> {
        Ok(Self(Db::open()?))
    }

    pub fn contains(&self, game: &Pgn) -> Result<bool> {
        let hash = game.as_hash() as i64;
        let result = self.0.rt.block_on(async {
            sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM tcec_games WHERE hash = ?)",
            )
            .bind(hash)
            .fetch_one(&self.0.pool)
            .await
        })?;
        Ok(result != 0)
    }

    pub fn add(&self, game: &Pgn) -> Result<()> {
        let hash = game.as_hash() as i64;
        let white = game.white_player.to_string();
        let black = game.black_player.to_string();
        self.0.rt.block_on(async {
            sqlx::query(
                "INSERT OR IGNORE INTO tcec_games (hash, white_player, black_player, date, event) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(hash)
            .bind(&white)
            .bind(&black)
            .bind(&game.date)
            .bind(&game.event)
            .execute(&self.0.pool)
            .await
        })?;
        Ok(())
    }
}
