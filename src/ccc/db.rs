use super::ccc_pgn::Game;
use crate::db::Db;
use anyhow::Result;

pub struct CccDb(Db);

impl CccDb {
    pub fn open() -> Result<Self> {
        Ok(Self(Db::open()?))
    }

    pub fn contains(&self, game: &Game) -> Result<bool> {
        let exists = self.0.rt.block_on(async {
            sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM ccc_games WHERE hash = ?)")
                .bind(game.hash() as i64)
                .fetch_one(&self.0.pool)
                .await
        })?;
        Ok(exists != 0)
    }

    pub fn add(&self, game: &Game) -> Result<()> {
        self.0.rt.block_on(async {
            sqlx::query("INSERT OR IGNORE INTO ccc_games (hash, white_player, black_player, date, event) VALUES (?, ?, ?, ?, ?)")
                .bind(game.hash() as i64).bind(&game.white).bind(&game.black)
                .bind(&game.date).bind(&game.event).execute(&self.0.pool).await
        })?;
        Ok(())
    }
}
