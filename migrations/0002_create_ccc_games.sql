CREATE TABLE ccc_games (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hash INTEGER NOT NULL UNIQUE,
    white_player TEXT NOT NULL,
    black_player TEXT NOT NULL,
    date TEXT NOT NULL,
    event TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
