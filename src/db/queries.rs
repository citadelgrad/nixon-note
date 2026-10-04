use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct Note {
    pub id: i64,
    pub content: String,
    pub content_type: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn row_to_note(row: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get("id")?,
        content: row.get("content")?,
        content_type: row.get("content_type")?,
        source_type: row.get("source_type")?,
        source_url: row.get("source_url")?,
        title: row.get("title")?,
        summary: row.get("summary")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn insert_note(
    conn: &Connection,
    content: &str,
    content_type: &str,
    source_type: &str,
    source_url: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO notes (content, content_type, source_type, source_url) VALUES (?1, ?2, ?3, ?4)",
        params![content, content_type, source_type, source_url],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn find_note_by_source_url(conn: &Connection, url: &str) -> Result<Option<Note>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at FROM notes WHERE source_url = ?1 LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![url], row_to_note)?;
    Ok(rows.next().transpose()?)
}

pub fn get_note(conn: &Connection, id: i64) -> Result<Note> {
    conn.query_row(
        "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at FROM notes WHERE id = ?1",
        params![id],
        row_to_note,
    ).context("Note not found")
}

pub fn update_note(conn: &Connection, id: i64, content: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE notes
         SET content = ?1, title = NULL, summary = NULL, updated_at = datetime('now')
         WHERE id = ?2",
        params![content, id],
    )?;
    tx.execute(
        "DELETE FROM note_embeddings WHERE note_id = ?1",
        params![id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn delete_note(conn: &Connection, id: i64) -> Result<()> {
    // Delete in order to respect foreign key constraints
    // 1. Delete embeddings
    conn.execute(
        "DELETE FROM note_embeddings WHERE note_id = ?1",
        params![id],
    )?;

    // 2. Delete links (both as source and target)
    conn.execute(
        "DELETE FROM note_links WHERE source_note_id = ?1 OR target_note_id = ?1",
        params![id],
    )?;

    // 3. Delete the note itself (FTS trigger will handle notes_fts cleanup)
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;

    Ok(())
}

pub fn get_notes_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<Note>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    // Build IN clause with placeholders
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at
         FROM notes WHERE id IN ({}) ORDER BY id DESC",
        placeholders
    );

    let mut stmt = conn.prepare(&query)?;
    let params: Vec<&dyn rusqlite::ToSql> =
        ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    let notes = stmt.query_map(&params[..], row_to_note)?;
    let mut notes = notes.collect::<Result<Vec<_>, _>>()?;
    notes.sort_by_key(|note| {
        ids.iter()
            .position(|id| *id == note.id)
            .unwrap_or(usize::MAX)
    });
    Ok(notes)
}

pub fn list_notes(conn: &Connection, limit: i64, offset: i64) -> Result<Vec<Note>> {
    list_notes_filtered(conn, limit, offset, false)
}

pub fn list_notes_filtered(
    conn: &Connection,
    limit: i64,
    offset: i64,
    hide_imported: bool,
) -> Result<Vec<Note>> {
    let sql = if hide_imported {
        "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at
         FROM notes
         WHERE source_type NOT IN ('bookmark', 'homebrew')
         ORDER BY id DESC LIMIT ?1 OFFSET ?2"
    } else {
        "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at
         FROM notes ORDER BY id DESC LIMIT ?1 OFFSET ?2"
    };
    let mut stmt = conn.prepare(sql)?;
    let notes = stmt.query_map(params![limit, offset], row_to_note)?;
    notes.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn search_fts(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Note>> {
    search_fts_filtered(conn, query, limit, false)
}

pub fn search_fts_filtered(
    conn: &Connection,
    query: &str,
    limit: i64,
    hide_imported: bool,
) -> Result<Vec<Note>> {
    // Sanitize query for FTS5: wrap each token in double quotes to escape
    // special characters like hyphens, asterisks, etc. that FTS5 interprets
    // as operators (e.g. "claude-m" would be parsed as "claude NOT m").
    let sanitized = query
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ");

    if sanitized.is_empty() {
        return Ok(vec![]);
    }

    let sql = if hide_imported {
        "SELECT n.id, n.content, n.content_type, n.source_type, n.source_url, n.title, n.summary, n.created_at, n.updated_at
         FROM notes n JOIN notes_fts ON notes_fts.rowid = n.id
         WHERE notes_fts MATCH ?1 AND n.source_type NOT IN ('bookmark', 'homebrew')
         ORDER BY rank LIMIT ?2"
    } else {
        "SELECT n.id, n.content, n.content_type, n.source_type, n.source_url, n.title, n.summary, n.created_at, n.updated_at
         FROM notes n JOIN notes_fts ON notes_fts.rowid = n.id
         WHERE notes_fts MATCH ?1 ORDER BY rank LIMIT ?2"
    };
    let mut stmt = conn.prepare(sql)?;
    let notes = stmt.query_map(params![sanitized, limit], row_to_note)?;
    notes.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn count_notes(conn: &Connection) -> Result<i64> {
    count_notes_filtered(conn, false)
}

pub fn count_notes_filtered(conn: &Connection, hide_imported: bool) -> Result<i64> {
    if hide_imported {
        return Ok(conn.query_row(
            "SELECT COUNT(*) FROM notes WHERE source_type NOT IN ('bookmark', 'homebrew')",
            [],
            |r| r.get(0),
        )?);
    }
    Ok(conn.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))?)
}

/// Export all notes (no limit/offset) for data export.
pub fn export_all_notes(conn: &Connection) -> Result<Vec<Note>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at
         FROM notes ORDER BY id ASC",
    )?;
    let notes = stmt.query_map([], row_to_note)?;
    notes.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get random notes for rediscovery — surfaces forgotten content.
pub fn random_notes(conn: &Connection, limit: i64, hide_imported: bool) -> Result<Vec<Note>> {
    let (query, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = if hide_imported {
        (
            "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at
             FROM notes
             WHERE source_type NOT IN ('bookmark', 'homebrew')
             ORDER BY RANDOM() LIMIT ?1"
                .to_string(),
            vec![Box::new(limit) as Box<dyn rusqlite::ToSql>],
        )
    } else {
        (
            "SELECT id, content, content_type, source_type, source_url, title, summary, created_at, updated_at
             FROM notes ORDER BY RANDOM() LIMIT ?1"
                .to_string(),
            vec![Box::new(limit) as Box<dyn rusqlite::ToSql>],
        )
    };

    let mut stmt = conn.prepare(&query)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let notes = stmt.query_map(&params_refs[..], row_to_note)?;
    notes.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// --- M2: Embeddings ---

pub fn insert_embedding(conn: &Connection, note_id: i64, embedding: &[f32]) -> Result<()> {
    if embedding.len() != 768 {
        anyhow::bail!("Embedding must be 768 dimensions, got {}", embedding.len());
    }
    let embedding_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

    // vec0 virtual table doesn't support UPSERT, so delete then insert
    conn.execute(
        "DELETE FROM note_embeddings WHERE note_id = ?1",
        params![note_id],
    )?;
    conn.execute(
        "INSERT INTO note_embeddings (note_id, embedding) VALUES (?1, ?2)",
        params![note_id, embedding_bytes],
    )?;
    Ok(())
}

pub fn insert_embedding_if_content_matches(
    conn: &Connection,
    note_id: i64,
    expected_content: &str,
    embedding: &[f32],
) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let current_content: String = tx.query_row(
        "SELECT content FROM notes WHERE id = ?1",
        params![note_id],
        |row| row.get(0),
    )?;
    if current_content != expected_content {
        return Ok(false);
    }
    insert_embedding(&tx, note_id, embedding)?;
    tx.commit()?;
    Ok(true)
}

pub fn search_similar(
    conn: &Connection,
    query_embedding: &[f32],
    limit: i64,
) -> Result<Vec<(i64, f64)>> {
    if query_embedding.len() != 768 {
        anyhow::bail!(
            "Query embedding must be 768 dimensions, got {}",
            query_embedding.len()
        );
    }
    let embedding_bytes: Vec<u8> = query_embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let mut stmt = conn.prepare(
        "SELECT note_id, distance
         FROM note_embeddings
         WHERE embedding MATCH ?1 AND k = ?2
         ORDER BY distance",
    )?;
    let results = stmt.query_map(params![embedding_bytes, limit], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    results.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn has_embedding(conn: &Connection, note_id: i64) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM note_embeddings WHERE note_id = ?1",
        params![note_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

pub fn notes_needing_embedding(conn: &Connection, limit: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM notes WHERE id NOT IN (SELECT note_id FROM note_embeddings) LIMIT ?1",
    )?;
    let ids = stmt.query_map(params![limit], |row| row.get(0))?;
    ids.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// --- M2: Links ---

#[derive(Debug, Serialize, Clone)]
#[allow(dead_code)]
pub struct NoteLink {
    pub source_note_id: i64,
    pub target_note_id: i64,
    pub link_type: String,
    pub reason: Option<String>,
    pub strength: f64,
}

#[allow(dead_code)]
pub fn add_note_link(
    conn: &Connection,
    source_note_id: i64,
    target_note_id: i64,
    link_type: &str,
    reason: Option<&str>,
    strength: f64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO note_links (source_note_id, target_note_id, link_type, reason, strength)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(source_note_id, target_note_id) DO UPDATE SET link_type = ?3, reason = ?4, strength = ?5",
        params![source_note_id, target_note_id, link_type, reason, strength],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn get_note_links(conn: &Connection, note_id: i64) -> Result<Vec<NoteLink>> {
    let mut stmt = conn.prepare(
        "SELECT source_note_id, target_note_id, link_type, reason, strength
         FROM note_links
         WHERE source_note_id = ?1 OR target_note_id = ?1",
    )?;
    let links = stmt.query_map(params![note_id], |row| {
        Ok(NoteLink {
            source_note_id: row.get(0)?,
            target_note_id: row.get(1)?,
            link_type: row.get(2)?,
            reason: row.get(3)?,
            strength: row.get(4)?,
        })
    })?;
    links.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// --- Audio Episodes ---

#[derive(Debug, Serialize, Clone)]
pub struct AudioEpisode {
    pub id: i64,
    pub title: String,
    pub episode_type: String,
    pub content_mode: String,
    pub tts_provider: String,
    pub tts_voice: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub audio_path: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub duration_seconds: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    pub note_ids: Vec<i64>,
}

fn row_to_audio_episode(row: &rusqlite::Row) -> rusqlite::Result<AudioEpisode> {
    Ok(AudioEpisode {
        id: row.get("id")?,
        title: row.get("title")?,
        episode_type: row.get("episode_type")?,
        content_mode: row.get("content_mode")?,
        tts_provider: row.get("tts_provider")?,
        tts_voice: row.get("tts_voice")?,
        status: row.get("status")?,
        error_message: row.get("error_message")?,
        audio_path: row.get("audio_path")?,
        file_size_bytes: row.get("file_size_bytes")?,
        duration_seconds: row.get("duration_seconds")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        note_ids: vec![], // populated separately
    })
}

pub fn create_audio_episode(
    conn: &Connection,
    title: &str,
    episode_type: &str,
    content_mode: &str,
    tts_provider: &str,
    tts_voice: &str,
    note_ids: &[i64],
) -> Result<i64> {
    conn.execute(
        "INSERT INTO audio_episodes (title, episode_type, content_mode, tts_provider, tts_voice)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![title, episode_type, content_mode, tts_provider, tts_voice],
    )?;
    let episode_id = conn.last_insert_rowid();

    for (i, note_id) in note_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO audio_episode_notes (episode_id, note_id, position) VALUES (?1, ?2, ?3)",
            params![episode_id, note_id, i as i64],
        )?;
    }

    Ok(episode_id)
}

pub fn backfill_audio_episode_titles(conn: &Connection) -> Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT id, episode_type
         FROM audio_episodes
         WHERE TRIM(title) = '' OR title = 'Untitled Note'
         ORDER BY id ASC",
    )?;
    let episodes = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut updated = 0;
    for (episode_id, episode_type) in episodes {
        let note_ids = get_episode_note_ids(conn, episode_id)?;
        let notes = get_notes_by_ids(conn, &note_ids)?;
        let title = derive_audio_episode_title(&episode_type, &notes);
        if title != "Untitled Note" {
            conn.execute(
                "UPDATE audio_episodes SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![title, episode_id],
            )?;
            updated += 1;
        }
    }

    Ok(updated)
}

pub fn derive_audio_episode_title(episode_type: &str, notes: &[Note]) -> String {
    match episode_type {
        "digest" => {
            let today = display_today();
            format!("{today} Daily Digest")
        }
        "batch" => {
            let first_title = notes
                .first()
                .and_then(note_title_or_content_title)
                .unwrap_or_else(|| "Notes".to_string());
            format!("{} + {} more", first_title, notes.len().saturating_sub(1))
        }
        _ => notes
            .first()
            .and_then(note_title_or_content_title)
            .unwrap_or_else(|| "Untitled Note".to_string()),
    }
}

pub fn clean_audio_episode_title(title: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn note_title_or_content_title(note: &Note) -> Option<String> {
    note.title
        .as_deref()
        .and_then(clean_audio_episode_title)
        .or_else(|| title_from_content(&note.content))
}

fn title_from_content(content: &str) -> Option<String> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "[Voice memo - transcribing...]")?;

    let line = line
        .trim_start_matches('#')
        .trim_start_matches(['-', '*', '•'])
        .trim();

    let sentence_end = line
        .char_indices()
        .find_map(|(idx, ch)| matches!(ch, '.' | '!' | '?').then_some(idx));
    let candidate = match sentence_end {
        Some(idx) => &line[..idx],
        None => line,
    }
    .trim_matches([':', '-', '—', ' ']);

    let title = truncate_title_words(candidate, 10);
    clean_audio_episode_title(&title)
}

fn truncate_title_words(text: &str, max_words: usize) -> String {
    text.split_whitespace()
        .take(max_words)
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_today() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = now / 86400;
    let year = 1970 + days / 365;
    let remaining_days = days % 365;
    let month = remaining_days / 30 + 1;
    let day = remaining_days % 30 + 1;
    format!("{year}-{month:02}-{day:02}")
}

fn get_episode_note_ids(conn: &Connection, episode_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT note_id FROM audio_episode_notes WHERE episode_id = ?1 ORDER BY position ASC",
    )?;
    let note_ids = stmt.query_map(params![episode_id], |row| row.get(0))?;
    note_ids
        .collect::<std::result::Result<Vec<i64>, _>>()
        .map_err(Into::into)
}

pub fn get_audio_episode(conn: &Connection, id: i64) -> Result<AudioEpisode> {
    let mut episode = conn
        .query_row(
            "SELECT id, title, episode_type, content_mode, tts_provider, tts_voice, status,
                    error_message, audio_path, file_size_bytes, duration_seconds, created_at, updated_at
             FROM audio_episodes WHERE id = ?1",
            params![id],
            row_to_audio_episode,
        )
        .context("Audio episode not found")?;

    // Populate note_ids
    episode.note_ids = get_episode_note_ids(conn, id)?;

    Ok(episode)
}

pub fn list_audio_episodes(conn: &Connection) -> Result<Vec<AudioEpisode>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, episode_type, content_mode, tts_provider, tts_voice, status,
                error_message, audio_path, file_size_bytes, duration_seconds, created_at, updated_at
         FROM audio_episodes ORDER BY id DESC",
    )?;
    let episodes: Vec<AudioEpisode> = stmt
        .query_map([], row_to_audio_episode)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Populate note_ids for each episode
    let mut note_stmt = conn.prepare(
        "SELECT episode_id, note_id FROM audio_episode_notes ORDER BY episode_id, position",
    )?;
    let note_rows: Vec<(i64, i64)> = note_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut notes_by_episode: std::collections::HashMap<i64, Vec<i64>> =
        std::collections::HashMap::new();
    for (ep_id, note_id) in note_rows {
        notes_by_episode.entry(ep_id).or_default().push(note_id);
    }

    Ok(episodes
        .into_iter()
        .map(|mut ep| {
            ep.note_ids = notes_by_episode.remove(&ep.id).unwrap_or_default();
            ep
        })
        .collect())
}

pub fn get_episodes_for_note(conn: &Connection, note_id: i64) -> Result<Vec<AudioEpisode>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.title, e.episode_type, e.content_mode, e.tts_provider, e.tts_voice, e.status,
                e.error_message, e.audio_path, e.file_size_bytes, e.duration_seconds, e.created_at, e.updated_at
         FROM audio_episodes e
         JOIN audio_episode_notes aen ON aen.episode_id = e.id
         WHERE aen.note_id = ?1
         ORDER BY e.id DESC",
    )?;
    let episodes: Vec<AudioEpisode> = stmt
        .query_map(params![note_id], row_to_audio_episode)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(episodes)
}

pub fn update_episode_status(
    conn: &Connection,
    id: i64,
    status: &str,
    audio_path: Option<&str>,
    file_size: Option<i64>,
    duration: Option<f64>,
    error: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE audio_episodes SET status = ?1, audio_path = COALESCE(?2, audio_path),
         file_size_bytes = COALESCE(?3, file_size_bytes), duration_seconds = COALESCE(?4, duration_seconds),
         error_message = ?5, updated_at = datetime('now') WHERE id = ?6",
        params![status, audio_path, file_size, duration, error, id],
    )?;
    Ok(())
}

pub fn delete_audio_episode(conn: &Connection, id: i64) -> Result<Option<String>> {
    // Get audio_path before deleting
    let audio_path: Option<String> = conn
        .query_row(
            "SELECT audio_path FROM audio_episodes WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .ok();

    // Junction table rows are deleted by ON DELETE CASCADE
    conn.execute("DELETE FROM audio_episodes WHERE id = ?1", params![id])?;
    Ok(audio_path)
}

// --- Settings ---

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    );
    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = datetime('now')",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_all_settings(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}

// --- Usage Tracking ---

#[derive(Debug, Serialize, Clone)]
pub struct UsageSummary {
    pub service: String,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_usd: f64,
    pub request_count: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct DailyUsage {
    pub date: String,
    pub service: String,
    pub total_cost_usd: f64,
    pub request_count: i64,
}

#[allow(clippy::too_many_arguments)]
pub fn record_usage(
    conn: &Connection,
    service: &str,
    operation: &str,
    note_id: Option<i64>,
    input_tokens: i64,
    output_tokens: i64,
    estimated_cost_usd: f64,
    model: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO api_usage (service, operation, note_id, input_tokens, output_tokens, estimated_cost_usd, model)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![service, operation, note_id, input_tokens, output_tokens, estimated_cost_usd, model],
    )?;
    Ok(())
}

pub fn get_usage_summary(conn: &Connection) -> Result<Vec<UsageSummary>> {
    let mut stmt = conn.prepare(
        "SELECT service, SUM(input_tokens), SUM(output_tokens), SUM(estimated_cost_usd), COUNT(*)
         FROM api_usage
         GROUP BY service
         ORDER BY SUM(estimated_cost_usd) DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageSummary {
            service: row.get(0)?,
            total_input_tokens: row.get(1)?,
            total_output_tokens: row.get(2)?,
            total_cost_usd: row.get(3)?,
            request_count: row.get(4)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_daily_usage(conn: &Connection, days: i64) -> Result<Vec<DailyUsage>> {
    let mut stmt = conn.prepare(
        "SELECT date(created_at) as day, service, SUM(estimated_cost_usd), COUNT(*)
         FROM api_usage
         WHERE created_at >= datetime('now', ?1)
         GROUP BY day, service
         ORDER BY day DESC",
    )?;
    let offset = format!("-{} days", days);
    let rows = stmt.query_map(params![offset], |row| {
        Ok(DailyUsage {
            date: row.get(0)?,
            service: row.get(1)?,
            total_cost_usd: row.get(2)?,
            request_count: row.get(3)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn setup() -> Connection {
        // Register sqlite-vec for tests
        unsafe {
            use rusqlite::ffi::sqlite3_auto_extension;
            sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let mut conn = Connection::open_in_memory().unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn insert_and_get() {
        let conn = setup();
        let id = insert_note(&conn, "test thought", "text", "cli", None).unwrap();
        let note = get_note(&conn, id).unwrap();
        assert_eq!(note.content, "test thought");
        assert_eq!(note.source_type, "cli");
    }

    #[test]
    fn insert_and_search() {
        let conn = setup();
        insert_note(
            &conn,
            "exploring sqlite-vec for vector search",
            "text",
            "cli",
            None,
        )
        .unwrap();
        insert_note(&conn, "making dinner tonight", "text", "web", None).unwrap();

        let results = search_fts(&conn, "vector", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("vector"));
    }

    #[test]
    fn filtered_search_applies_source_filter_before_limit() {
        let conn = setup();
        insert_note(&conn, "shared term", "text", "web", None).unwrap();
        insert_note(&conn, "shared term", "text", "homebrew", None).unwrap();

        let results = search_fts_filtered(&conn, "shared", 1, true).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_type, "web");
    }

    #[test]
    fn list_notes_ordered() {
        let conn = setup();
        insert_note(&conn, "first note", "text", "cli", None).unwrap();
        insert_note(&conn, "second note", "text", "cli", None).unwrap();

        let notes = list_notes(&conn, 10, 0).unwrap();
        assert_eq!(notes.len(), 2);
        // Most recent first
        assert_eq!(notes[0].content, "second note");
    }

    #[test]
    fn fts_update_trigger() {
        let conn = setup();
        let id = insert_note(&conn, "original content", "text", "cli", None).unwrap();

        // Search finds original
        let results = search_fts(&conn, "original", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Update the note
        conn.execute(
            "UPDATE notes SET content = 'updated content', updated_at = datetime('now') WHERE id = ?1",
            params![id],
        ).unwrap();

        // Search finds updated, not original
        let results = search_fts(&conn, "updated", 10).unwrap();
        assert_eq!(results.len(), 1);
        let results = search_fts(&conn, "original", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn fts_delete_trigger() {
        let conn = setup();
        let id = insert_note(&conn, "temporary note", "text", "cli", None).unwrap();

        let results = search_fts(&conn, "temporary", 10).unwrap();
        assert_eq!(results.len(), 1);

        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
            .unwrap();

        let results = search_fts(&conn, "temporary", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn get_missing_note_returns_error() {
        let conn = setup();
        assert!(get_note(&conn, 999).is_err());
    }

    #[test]
    fn search_empty_db() {
        let conn = setup();
        let results = search_fts(&conn, "anything", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn insert_and_check_embedding() {
        let conn = setup();
        let note_id = insert_note(&conn, "test", "text", "cli", None).unwrap();
        let embedding = vec![0.1f32; 768];

        assert!(!has_embedding(&conn, note_id).unwrap());
        insert_embedding(&conn, note_id, &embedding).unwrap();
        assert!(has_embedding(&conn, note_id).unwrap());
    }

    #[test]
    fn embedding_wrong_dimension_errors() {
        let conn = setup();
        let note_id = insert_note(&conn, "test", "text", "cli", None).unwrap();
        let wrong_embedding = vec![0.1f32; 512]; // wrong dimension (< 768)

        assert!(insert_embedding(&conn, note_id, &wrong_embedding).is_err());
    }

    #[test]
    fn embedding_too_large_dimension_errors() {
        let conn = setup();
        let note_id = insert_note(&conn, "test", "text", "cli", None).unwrap();
        let wrong_embedding = vec![0.1f32; 1024]; // wrong dimension (> 768)

        assert!(insert_embedding(&conn, note_id, &wrong_embedding).is_err());
    }

    #[test]
    fn search_similar_rejects_wrong_dimensions() {
        let conn = setup();

        // Test with embedding < 768 dimensions
        let small_embedding = vec![0.1f32; 512];
        assert!(search_similar(&conn, &small_embedding, 10).is_err());

        // Test with embedding > 768 dimensions
        let large_embedding = vec![0.1f32; 1024];
        assert!(search_similar(&conn, &large_embedding, 10).is_err());
    }

    #[test]
    fn search_similar_basic() {
        let conn = setup();
        let n1 = insert_note(&conn, "test1", "text", "cli", None).unwrap();
        let n2 = insert_note(&conn, "test2", "text", "cli", None).unwrap();

        let emb1 = vec![1.0f32; 768];
        let emb2 = vec![0.5f32; 768];

        insert_embedding(&conn, n1, &emb1).unwrap();
        insert_embedding(&conn, n2, &emb2).unwrap();

        let query = vec![1.0f32; 768];
        let results = search_similar(&conn, &query, 10).unwrap();
        assert_eq!(results.len(), 2);
        // First result should be n1 (closer to query)
        assert_eq!(results[0].0, n1);
    }

    #[test]
    fn notes_needing_embedding_finds_unprocessed() {
        let conn = setup();
        let n1 = insert_note(&conn, "has embedding", "text", "cli", None).unwrap();
        let n2 = insert_note(&conn, "no embedding", "text", "cli", None).unwrap();

        let emb = vec![0.1f32; 768];
        insert_embedding(&conn, n1, &emb).unwrap();

        let needing = notes_needing_embedding(&conn, 10).unwrap();
        assert_eq!(needing.len(), 1);
        assert_eq!(needing[0], n2);
    }

    #[test]
    fn count_notes_returns_correct_count() {
        let conn = setup();
        assert_eq!(count_notes(&conn).unwrap(), 0);
        insert_note(&conn, "note1", "text", "cli", None).unwrap();
        assert_eq!(count_notes(&conn).unwrap(), 1);
        insert_note(&conn, "note2", "text", "cli", None).unwrap();
        assert_eq!(count_notes(&conn).unwrap(), 2);
    }

    #[test]
    fn find_note_by_source_url_works() {
        let conn = setup();
        let id = insert_note(&conn, "content", "text", "web", Some("https://example.com")).unwrap();
        let found = find_note_by_source_url(&conn, "https://example.com").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id);
        let missing = find_note_by_source_url(&conn, "https://nope.com").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn get_notes_by_ids_preserves_requested_order() {
        let conn = setup();
        let first = insert_note(&conn, "first requested", "text", "cli", None).unwrap();
        let second = insert_note(&conn, "second requested", "text", "cli", None).unwrap();

        let notes = get_notes_by_ids(&conn, &[first, second]).unwrap();

        assert_eq!(
            notes.iter().map(|note| note.id).collect::<Vec<_>>(),
            vec![first, second]
        );
    }

    #[test]
    fn backfill_audio_episode_titles_uses_note_title() {
        let conn = setup();
        let note_id = insert_note(&conn, "raw body", "text", "cli", None).unwrap();
        conn.execute(
            "UPDATE notes SET title = 'Backfilled Note Title' WHERE id = ?1",
            params![note_id],
        )
        .unwrap();
        let episode_id = create_audio_episode(
            &conn,
            "Untitled Note",
            "single",
            "full",
            "elevenlabs",
            "voice",
            &[note_id],
        )
        .unwrap();

        assert_eq!(backfill_audio_episode_titles(&conn).unwrap(), 1);

        assert_eq!(
            get_audio_episode(&conn, episode_id).unwrap().title,
            "Backfilled Note Title"
        );
    }

    #[test]
    fn backfill_audio_episode_titles_uses_content_fallback() {
        let conn = setup();
        let note_id = insert_note(
            &conn,
            "# ElevenLabs rollout notes\nThis body should not become the title.",
            "text",
            "cli",
            None,
        )
        .unwrap();
        let episode_id = create_audio_episode(
            &conn,
            "Untitled Note",
            "single",
            "full",
            "elevenlabs",
            "voice",
            &[note_id],
        )
        .unwrap();

        assert_eq!(backfill_audio_episode_titles(&conn).unwrap(), 1);

        assert_eq!(
            get_audio_episode(&conn, episode_id).unwrap().title,
            "ElevenLabs rollout notes"
        );
    }

    #[test]
    fn backfill_audio_episode_titles_preserves_custom_titles() {
        let conn = setup();
        let note_id = insert_note(&conn, "new body", "text", "cli", None).unwrap();
        let episode_id = create_audio_episode(
            &conn,
            "Already Good",
            "single",
            "full",
            "elevenlabs",
            "voice",
            &[note_id],
        )
        .unwrap();

        assert_eq!(backfill_audio_episode_titles(&conn).unwrap(), 0);

        assert_eq!(
            get_audio_episode(&conn, episode_id).unwrap().title,
            "Already Good"
        );
    }

    #[test]
    fn search_fts_empty_query_returns_empty() {
        let conn = setup();
        insert_note(&conn, "some content", "text", "cli", None).unwrap();
        let results = search_fts(&conn, "", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn add_and_get_note_links() {
        let conn = setup();
        let n1 = insert_note(&conn, "source", "text", "cli", None).unwrap();
        let n2 = insert_note(&conn, "target", "text", "cli", None).unwrap();

        add_note_link(&conn, n1, n2, "related", Some("both about rust"), 0.8).unwrap();

        let links = get_note_links(&conn, n1).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].source_note_id, n1);
        assert_eq!(links[0].target_note_id, n2);
        assert_eq!(links[0].link_type, "related");
        assert_eq!(links[0].reason, Some("both about rust".to_string()));
        assert_eq!(links[0].strength, 0.8);
    }

    #[test]
    fn update_note_invalidates_derived_metadata_and_embedding() {
        let conn = setup();
        let note_id = insert_note(&conn, "old content", "text", "cli", None).unwrap();
        conn.execute(
            "UPDATE notes SET title = 'Old title', summary = 'Old summary' WHERE id = ?1",
            params![note_id],
        )
        .unwrap();
        insert_embedding(&conn, note_id, &vec![0.1; 768]).unwrap();

        update_note(&conn, note_id, "new content").unwrap();

        let note = get_note(&conn, note_id).unwrap();
        assert_eq!(note.content, "new content");
        assert_eq!(note.title, None);
        assert_eq!(note.summary, None);
        assert!(!has_embedding(&conn, note_id).unwrap());
    }

    #[test]
    fn stale_embedding_is_not_stored_after_content_changes() {
        let conn = setup();
        let note_id = insert_note(&conn, "old content", "text", "cli", None).unwrap();

        update_note(&conn, note_id, "new content").unwrap();
        let stored =
            insert_embedding_if_content_matches(&conn, note_id, "old content", &vec![0.1; 768])
                .unwrap();

        assert!(!stored);
        assert!(!has_embedding(&conn, note_id).unwrap());
    }
}
