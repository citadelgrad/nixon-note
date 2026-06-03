use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            "-- Migration 1: notes table + FTS5 + sync triggers
CREATE TABLE notes (
    id INTEGER PRIMARY KEY,
    content TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'text',
    source_type TEXT NOT NULL DEFAULT 'cli',
    source_url TEXT,
    title TEXT,
    summary TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE VIRTUAL TABLE notes_fts USING fts5(
    title, content, summary,
    content='notes', content_rowid='id',
    tokenize='porter unicode61 remove_diacritics 2'
);

CREATE TRIGGER notes_ai AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, title, content, summary)
    VALUES (NEW.id, NEW.title, NEW.content, NEW.summary);
END;

CREATE TRIGGER notes_au AFTER UPDATE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, content, summary)
    VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.summary);
    INSERT INTO notes_fts(rowid, title, content, summary)
    VALUES (NEW.id, NEW.title, NEW.content, NEW.summary);
END;

CREATE TRIGGER notes_ad AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, content, summary)
    VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.summary);
END;",
        ),
        M::up(
            "-- Migration 2: M2 Intelligence tables (tags, embeddings, links)
CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE note_tags (
    note_id INTEGER NOT NULL REFERENCES notes(id),
    tag_id INTEGER NOT NULL REFERENCES tags(id),
    confidence REAL DEFAULT 1.0,
    source TEXT NOT NULL DEFAULT 'ai',
    PRIMARY KEY (note_id, tag_id)
);

CREATE VIRTUAL TABLE note_embeddings USING vec0(
    note_id INTEGER PRIMARY KEY,
    embedding float[768]
);

CREATE TABLE note_links (
    source_note_id INTEGER NOT NULL REFERENCES notes(id),
    target_note_id INTEGER NOT NULL REFERENCES notes(id),
    link_type TEXT NOT NULL DEFAULT 'related',
    reason TEXT,
    strength REAL DEFAULT 0.5,
    PRIMARY KEY (source_note_id, target_note_id)
);",
        ),
        M::up(
            "-- Migration 3: API usage tracking
CREATE TABLE api_usage (
    id INTEGER PRIMARY KEY,
    service TEXT NOT NULL,
    operation TEXT NOT NULL,
    note_id INTEGER,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
    model TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_api_usage_created ON api_usage(created_at);
CREATE INDEX idx_api_usage_service ON api_usage(service);",
        ),
        M::up(
            "-- Migration 4: Audio episodes, episode-note junction, and settings

CREATE TABLE audio_episodes (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    episode_type TEXT NOT NULL DEFAULT 'single',
    content_mode TEXT NOT NULL DEFAULT 'full',
    tts_provider TEXT NOT NULL DEFAULT 'openai',
    tts_voice TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    audio_path TEXT,
    file_size_bytes INTEGER,
    duration_seconds REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE audio_episode_notes (
    episode_id INTEGER NOT NULL REFERENCES audio_episodes(id) ON DELETE CASCADE,
    note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (episode_id, note_id)
);

CREATE INDEX idx_audio_episode_notes_note ON audio_episode_notes(note_id);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO settings (key, value) VALUES ('tts_provider', 'openai');
INSERT INTO settings (key, value) VALUES ('tts_voice_openai', 'alloy');
INSERT INTO settings (key, value) VALUES ('tts_voice_gemini', 'Kore');",
        ),
    ])
}

pub fn run(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_to_fresh_db() {
        // Register sqlite-vec for migrations test
        unsafe {
            use rusqlite::ffi::sqlite3_auto_extension;
            sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();

        // Verify notes table exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn migrations_are_idempotent() {
        // Register sqlite-vec for migrations test
        unsafe {
            use rusqlite::ffi::sqlite3_auto_extension;
            sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run should be no-op
    }
}
