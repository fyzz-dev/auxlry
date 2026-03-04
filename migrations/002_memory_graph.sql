-- Memory metadata: types, access tracking, timestamps
CREATE TABLE IF NOT EXISTS memory_metadata (
    id TEXT PRIMARY KEY,
    memory_type TEXT NOT NULL DEFAULT 'observation',
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memory_metadata_type ON memory_metadata(memory_type);
CREATE INDEX IF NOT EXISTS idx_memory_metadata_accessed ON memory_metadata(last_accessed_at);

-- Memory graph edges: typed relationships between memories
CREATE TABLE IF NOT EXISTS memory_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    relation_type TEXT NOT NULL CHECK (relation_type IN ('related_to','supersedes','contradicts','caused_by','part_of')),
    weight REAL NOT NULL DEFAULT 1.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, target_id, relation_type)
);

CREATE INDEX IF NOT EXISTS idx_memory_edges_source ON memory_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_target ON memory_edges(target_id);
