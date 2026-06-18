use anyhow::Result;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;

const DB_PATH: &str = "chess-notifier.db";

pub struct Db {
    pub(crate) rt: tokio::runtime::Runtime,
    pub(crate) pool: SqlitePool,
}

impl Db {
    pub fn open() -> Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        let pool = rt.block_on(async {
            let options = SqliteConnectOptions::new()
                .filename(DB_PATH)
                .create_if_missing(true);
            SqlitePool::connect_with(options).await
        })?;

        Ok(Self { rt, pool })
    }
}

pub fn run_migrations() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let pool = rt.block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(DB_PATH)
            .create_if_missing(true);
        SqlitePool::connect_with(options).await
    })?;

    rt.block_on(async { sqlx::migrate!().run(&pool).await })?;

    Ok(())
}
