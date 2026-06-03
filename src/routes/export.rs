use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::db::queries;

use super::notes::{AppError, flatten_interact};

#[derive(Deserialize)]
pub struct ExportQuery {
    pub format: String,
}

#[derive(Serialize)]
struct ExportJson {
    export_date: String,
    note_count: usize,
    version: &'static str,
    notes: Vec<ExportNote>,
}

#[derive(Serialize)]
struct ExportNote {
    id: i64,
    content: String,
    title: Option<String>,
    summary: Option<String>,
    source_type: String,
    source_url: Option<String>,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

impl From<queries::Note> for ExportNote {
    fn from(n: queries::Note) -> Self {
        ExportNote {
            id: n.id,
            content: n.content,
            title: n.title,
            summary: n.summary,
            source_type: n.source_type,
            source_url: n.source_url,
            tags: n.tags,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}

/// Sanitize a title for use as a filename: lowercase, replace non-alphanumeric
/// with hyphens, collapse consecutive hyphens, trim leading/trailing hyphens,
/// and truncate to 50 characters.
fn sanitize_title(title: &str) -> String {
    let sanitized: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    // Collapse consecutive hyphens
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push(c);
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    // Trim leading/trailing hyphens and truncate
    let trimmed = result.trim_matches('-');
    if trimmed.len() > 50 {
        // Truncate at char boundary (all ASCII so this is safe)
        trimmed[..50].trim_end_matches('-').to_string()
    } else {
        trimmed.to_string()
    }
}

pub async fn export_notes(
    State(state): State<AppState>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, AppError> {
    let notes = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(|conn| queries::export_all_notes(conn))
            .await,
    )?;

    match params.format.as_str() {
        "json" => export_json(notes),
        "markdown" => export_markdown_zip(notes),
        _ => Err(AppError::BadRequest(
            "Invalid format. Use 'json' or 'markdown'.".into(),
        )),
    }
}

fn export_json(notes: Vec<queries::Note>) -> Result<Response, AppError> {
    let export = ExportJson {
        export_date: chrono_now(),
        note_count: notes.len(),
        version: "1.0",
        notes: notes.into_iter().map(ExportNote::from).collect(),
    };

    let json_bytes =
        serde_json::to_vec_pretty(&export).map_err(|e| AppError::Internal(e.into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"nixonnote-export.json\""),
    );

    Ok((headers, json_bytes).into_response())
}

fn export_markdown_zip(notes: Vec<queries::Note>) -> Result<Response, AppError> {
    use std::io::{Cursor, Write};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for note in &notes {
        let title_part = match &note.title {
            Some(t) if !t.is_empty() => sanitize_title(t),
            _ => "untitled".to_string(),
        };
        let filename = format!("{}-{}.md", note.id, title_part);

        zip.start_file(&filename, options)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("ZIP error: {e}")))?;

        // Build YAML frontmatter
        let title_yaml = match &note.title {
            Some(t) => format!("title: \"{}\"", t.replace('"', "\\\"")),
            None => "title: \"\"".to_string(),
        };
        let tags_yaml = format!(
            "[{}]",
            note.tags
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(", ")
        );
        // Extract just the date portion from created_at (e.g. "2026-02-08" from "2026-02-08 12:00:00")
        let created_date = note
            .created_at
            .split_whitespace()
            .next()
            .unwrap_or(&note.created_at);

        let frontmatter = format!(
            "---\nid: {}\n{}\nsource_type: \"{}\"\ntags: {}\ncreated_at: \"{}\"\n---\n\n",
            note.id, title_yaml, note.source_type, tags_yaml, created_date
        );

        write!(zip, "{}{}", frontmatter, note.content)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("ZIP write error: {e}")))?;
    }

    let cursor = zip
        .finish()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ZIP finish error: {e}")))?;
    let zip_bytes = cursor.into_inner();

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"nixonnote-export.zip\""),
    );

    Ok((headers, zip_bytes).into_response())
}

/// Returns the current UTC datetime as an ISO 8601 string.
/// Uses a simple approach without requiring the `chrono` crate.
fn chrono_now() -> String {
    // We rely on the system to provide the current time formatted as ISO 8601.
    // Since this is a Rust backend without chrono, we use a formatted SQLite-style timestamp.
    // For the export, we'll use a static approach via std::time.
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Convert Unix timestamp to ISO 8601 manually
    // Days calculation from Unix epoch (1970-01-01)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Compute year, month, day from days since epoch
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm adapted from Howard Hinnant's civil_from_days
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_title_basic() {
        assert_eq!(sanitize_title("Hello World"), "hello-world");
    }

    #[test]
    fn sanitize_title_special_chars() {
        assert_eq!(
            sanitize_title("My Note! @#$ About Rust"),
            "my-note-about-rust"
        );
    }

    #[test]
    fn sanitize_title_truncation() {
        let long = "a".repeat(100);
        let result = sanitize_title(&long);
        assert!(result.len() <= 50);
    }

    #[test]
    fn sanitize_title_empty() {
        assert_eq!(sanitize_title(""), "");
    }

    #[test]
    fn sanitize_title_leading_trailing_special() {
        assert_eq!(sanitize_title("---hello---"), "hello");
    }

    #[test]
    fn chrono_now_format() {
        let now = chrono_now();
        // Should match ISO 8601 pattern: YYYY-MM-DDTHH:MM:SSZ
        assert!(now.ends_with('Z'));
        assert_eq!(now.len(), 20);
        assert_eq!(&now[4..5], "-");
        assert_eq!(&now[7..8], "-");
        assert_eq!(&now[10..11], "T");
    }

    #[test]
    fn chrono_now_produces_valid_date() {
        let now = chrono_now();
        let year: u64 = now[..4].parse().unwrap();
        assert!(year >= 2024 && year <= 2030, "Year out of range: {}", year);
        let month: u64 = now[5..7].parse().unwrap();
        assert!(month >= 1 && month <= 12, "Month out of range: {}", month);
        let day: u64 = now[8..10].parse().unwrap();
        assert!(day >= 1 && day <= 31, "Day out of range: {}", day);
        let hours: u64 = now[11..13].parse().unwrap();
        assert!(hours < 24);
        let minutes: u64 = now[14..16].parse().unwrap();
        assert!(minutes < 60);
        let seconds: u64 = now[17..19].parse().unwrap();
        assert!(seconds < 60);
    }

    #[test]
    fn days_to_ymd_known_dates() {
        // Unix epoch: 1970-01-01 = day 0
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // 2000-01-01 = day 10957
        assert_eq!(days_to_ymd(10957), (2000, 1, 1));
        // 2024-02-29 (leap day) = day 19782
        assert_eq!(days_to_ymd(19782), (2024, 2, 29));
    }
}
