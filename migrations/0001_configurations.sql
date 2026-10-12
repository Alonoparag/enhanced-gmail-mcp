CREATE TABLE configurations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    gmail_query TEXT,
    inbox_only INTEGER NOT NULL,
    include_spam_trash INTEGER NOT NULL,
    max_results INTEGER NOT NULL CHECK(max_results BETWEEN 1 AND 100),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
