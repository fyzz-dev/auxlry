CREATE TABLE IF NOT EXISTS pending_link_codes (
    code TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
