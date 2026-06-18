CREATE TABLE ccrl_games (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hash INTEGER NOT NULL UNIQUE,
    white_player TEXT NOT NULL,
    black_player TEXT NOT NULL,
    date TEXT NOT NULL,
    site TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE tcec_games (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hash INTEGER NOT NULL UNIQUE,
    white_player TEXT NOT NULL,
    black_player TEXT NOT NULL,
    date TEXT NOT NULL,
    event TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
