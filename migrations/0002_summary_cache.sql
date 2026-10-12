CREATE TABLE summary_cache (
    identity TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('message', 'thread')),
    summary TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
