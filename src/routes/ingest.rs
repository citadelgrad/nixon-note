use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::AppState;
use crate::db::queries;
use crate::routes::notes::flatten_interact;

// ============================================
// Bookmarks Ingestion
// ============================================

#[derive(Deserialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct BookmarksImportRequest {
    pub bookmarks: Vec<Bookmark>,
}

#[derive(Serialize)]
pub struct BookmarksImportResponse {
    pub imported: usize,
    pub failed: usize,
    pub note_ids: Vec<i64>,
}

/// POST /api/ingest/bookmarks
/// Import bookmarks as notes
pub async fn import_bookmarks(
    State(state): State<AppState>,
    Json(body): Json<BookmarksImportRequest>,
) -> Result<Json<BookmarksImportResponse>, AppError> {
    let mut note_ids = Vec::new();
    let mut failed = 0;

    for bookmark in body.bookmarks {
        // Create content from bookmark
        let content = if let Some(notes) = bookmark.notes {
            format!("{}\n\n{}", bookmark.title, notes)
        } else {
            bookmark.title.clone()
        };

        // Insert note
        let note_id_result = flatten_interact(
            state
                .pool
                .get()
                .await
                .map_err(anyhow::Error::from)?
                .interact(move |conn| {
                    queries::insert_note(conn, &content, "text", "bookmark", Some(&bookmark.url))
                })
                .await,
        );

        match note_id_result {
            Ok(note_id) => {
                note_ids.push(note_id);

                // Queue for background processing (embeddings, auto-tagging)
                let _ = state.background.enqueue(note_id).await;
            }
            Err(_) => failed += 1,
        }
    }

    Ok(Json(BookmarksImportResponse {
        imported: note_ids.len(),
        failed,
        note_ids,
    }))
}

// ============================================
// YouTube Ingestion
// ============================================

#[derive(Deserialize)]
pub struct YouTubeIngestRequest {
    pub url: String,
}

#[derive(Serialize)]
pub struct YouTubeIngestResponse {
    pub note_id: i64,
    pub title: String,
    pub summary: String,
}

#[derive(Serialize)]
struct GeminiRequest {
    model: String,
    messages: Vec<GeminiMessage>,
}

#[derive(Serialize)]
struct GeminiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    choices: Vec<GeminiChoice>,
}

#[derive(Deserialize)]
struct GeminiChoice {
    message: GeminiResponseMessage,
}

#[derive(Deserialize)]
struct GeminiResponseMessage {
    content: String,
}

const GEMINI_API_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";
const GEMINI_MODEL: &str = "gemini-2.5-flash";

/// Extract video ID from YouTube URL
pub(crate) fn extract_video_id(url: &str) -> Result<String, AppError> {
    // Handle various YouTube URL formats:
    // - https://www.youtube.com/watch?v=VIDEO_ID
    // - https://youtu.be/VIDEO_ID
    // - https://www.youtube.com/embed/VIDEO_ID
    // - https://m.youtube.com/watch?v=VIDEO_ID

    if let Some(id) = url.strip_prefix("https://youtu.be/") {
        return Ok(id.split('?').next().unwrap_or(id).to_string());
    }

    if let Some(query_start) = url.find('?') {
        let query = &url[query_start + 1..];
        for param in query.split('&') {
            if let Some(v) = param.strip_prefix("v=") {
                return Ok(v.to_string());
            }
        }
    }

    if let Some(embed_pos) = url.find("/embed/") {
        let id_start = embed_pos + 7;
        let id = &url[id_start..];
        return Ok(id.split('?').next().unwrap_or(id).to_string());
    }

    Err(AppError::BadRequest(
        "Could not extract video ID from URL".to_string(),
    ))
}

/// Fetch transcript using yt-dlp as fallback (tier 2).
/// Uses --write-auto-subs to get auto-generated captions (~95% of videos).
async fn fetch_transcript_ytdlp(video_id: &str) -> Option<String> {
    use tokio::process::Command;

    // Check if yt-dlp is available
    if Command::new("yt-dlp")
        .arg("--version")
        .output()
        .await
        .is_err()
    {
        info!("yt-dlp not found, skipping tier 2 transcript fetch");
        return None;
    }

    let temp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return None,
    };

    let output_template = temp_dir.path().join("%(id)s.%(ext)s");
    let url = format!("https://www.youtube.com/watch?v={}", video_id);

    info!("Fetching transcript via yt-dlp for video {}", video_id);

    let output = Command::new("yt-dlp")
        .args([
            "--write-auto-subs",
            "--skip-download",
            "--sub-format",
            "vtt",
            "--sub-langs",
            "en.*",
            "-o",
        ])
        .arg(output_template.to_str().unwrap_or("output"))
        .arg(&url)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        info!("yt-dlp failed: {}", stderr);
        // Retry without language filter
        let output = Command::new("yt-dlp")
            .args([
                "--write-auto-subs",
                "--skip-download",
                "--sub-format",
                "vtt",
                "-o",
            ])
            .arg(
                temp_dir
                    .path()
                    .join("%(id)s.%(ext)s")
                    .to_str()
                    .unwrap_or("output"),
            )
            .arg(&url)
            .output()
            .await
            .ok()?;

        if !output.status.success() {
            return None;
        }
    }

    // Find VTT file in temp dir
    let vtt_path = find_vtt_file(temp_dir.path())?;

    // Read and parse VTT
    let vtt_content = std::fs::read_to_string(&vtt_path).ok()?;
    let text = parse_vtt(&vtt_content);

    if text.is_empty() { None } else { Some(text) }
}

/// Find a .vtt file in the given directory
fn find_vtt_file(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("vtt") {
                Some(path)
            } else {
                None
            }
        })
}

/// Parse WebVTT content into clean text.
/// Removes WEBVTT header, timestamps, formatting tags, and deduplicates lines.
pub(crate) fn parse_vtt(vtt: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut prev_line = String::new();

    for line in vtt.lines() {
        let trimmed = line.trim();

        // Skip WEBVTT header, NOTE blocks, cue identifiers, and empty lines
        if trimmed.is_empty()
            || trimmed.starts_with("WEBVTT")
            || trimmed.starts_with("Kind:")
            || trimmed.starts_with("Language:")
            || trimmed.starts_with("NOTE")
            || trimmed.contains("-->")
        {
            continue;
        }

        // Skip lines that are just timestamps (e.g., "00:00:01.000")
        if trimmed.len() <= 20
            && trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || c == ':' || c == '.' || c == ',')
        {
            continue;
        }

        // Strip VTT formatting tags like <c.colorE5E5E5>, </c>, <b>, etc.
        let cleaned = strip_vtt_tags(trimmed);

        // Deduplicate consecutive identical lines (common in auto-generated subs)
        if !cleaned.is_empty() && cleaned != prev_line {
            lines.push(cleaned.clone());
            prev_line = cleaned;
        }
    }

    lines.join(" ")
}

/// Remove VTT formatting tags from a line
fn strip_vtt_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;

    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result.trim().to_string()
}

/// POST /api/ingest/youtube
/// Fetch YouTube transcript, summarize with Gemini, save as note.
/// Gracefully degrades: creates a note even if transcript or summary fails.
/// Tier 1: yt-transcript-rs (fast, native) → Tier 2: yt-dlp (reliable, subprocess)
pub async fn ingest_youtube(
    State(state): State<AppState>,
    Json(body): Json<YouTubeIngestRequest>,
) -> Result<Json<YouTubeIngestResponse>, AppError> {
    use yt_transcript_rs::YouTubeTranscriptApi;

    // 1. Extract video ID from URL (required)
    let video_id = extract_video_id(&body.url)?;

    // 2. Try to fetch video details + transcript (both optional)
    let api = YouTubeTranscriptApi::new(None, None, None).ok();

    let video_title = match &api {
        Some(a) => a
            .fetch_video_details(&video_id)
            .await
            .map(|d| d.title)
            .unwrap_or_else(|_| format!("YouTube Video {}", video_id)),
        None => format!("YouTube Video {}", video_id),
    };

    // Tier 1: try yt-transcript-rs (fast, native)
    let mut transcript_text = match &api {
        Some(a) => a
            .fetch_transcript(&video_id, &["en"], false)
            .await
            .ok()
            .map(|t| t.text()),
        None => None,
    };

    // Tier 2: fall back to yt-dlp if tier 1 failed
    if transcript_text.is_none() {
        info!(
            "Tier 1 transcript fetch failed for {}, trying yt-dlp",
            video_id
        );
        transcript_text = fetch_transcript_ytdlp(&video_id).await;
        if transcript_text.is_some() {
            info!(
                "Tier 2 (yt-dlp) transcript fetch succeeded for {}",
                video_id
            );
        }
    }

    // 3. Try to summarize with Gemini (optional — needs transcript + API key)
    let has_gemini = !std::env::var("GEMINI_API_KEY")
        .unwrap_or_default()
        .is_empty();
    tracing::info!("Gemini configured: {}", has_gemini);

    let mut gemini_failed = false;
    let summary = match (&transcript_text, has_gemini) {
        (Some(transcript), true) => {
            match summarize_with_gemini(&state.client, &video_title, transcript).await {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("Gemini summarization failed: {:?}", e);
                    gemini_failed = true;
                    None
                }
            }
        }
        _ => None,
    };

    // 4. Build note content with whatever we got
    let content = match (&summary, &transcript_text) {
        (Some(s), _) => format!(
            "# {}\n\n## Summary\n\n{}\n\n## Video\n\n{}",
            video_title, s, body.url
        ),
        (None, Some(_)) if gemini_failed => format!(
            "# {}\n\n*Transcript available but summarization failed. Check server logs for details.*\n\n## Video\n\n{}",
            video_title, body.url
        ),
        (None, Some(_)) => format!(
            "# {}\n\n*Transcript available but summarization skipped (Gemini not configured).*\n\n## Video\n\n{}",
            video_title, body.url
        ),
        (None, None) => format!(
            "# {}\n\n*No transcript available for this video.*\n\n## Video\n\n{}",
            video_title, body.url
        ),
    };

    let summary_text = summary.unwrap_or_default();

    // 5. Save note
    let note_id = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                queries::insert_note(conn, &content, "text", "youtube", Some(&body.url))
            })
            .await,
    )?;

    // Queue for background processing
    let _ = state.background.enqueue(note_id).await;

    Ok(Json(YouTubeIngestResponse {
        note_id,
        title: video_title,
        summary: summary_text,
    }))
}

async fn summarize_with_gemini(
    client: &reqwest::Client,
    video_title: &str,
    transcript: &str,
) -> Result<String, AppError> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("GEMINI_API_KEY not set")))?;

    let prompt = format!(
        "Please provide a concise summary of this YouTube video transcript.\n\nVideo Title: {}\n\nTranscript:\n{}",
        video_title, transcript
    );

    let request = GeminiRequest {
        model: GEMINI_MODEL.to_string(),
        messages: vec![GeminiMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let url = std::env::var("GEMINI_SUMMARIZE_URL").unwrap_or_else(|_| GEMINI_API_URL.to_string());

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to call Gemini API: {e}")))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "Gemini summarization failed ({}): {}",
            status, body
        )));
    }

    let gemini_res: GeminiResponse = res
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse Gemini response: {e}")))?;

    let summary = gemini_res
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("No content in Gemini response")))?;

    Ok(summary)
}

// ============================================
// Twitter/X.com Tweet Ingestion
// ============================================

/// Check if a URL is a Twitter/X.com tweet URL
fn is_twitter_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    (lower.contains("x.com/") || lower.contains("twitter.com/")) && lower.contains("/status/")
}

// --- FxTwitter API response types ---

#[derive(Deserialize, Debug)]
struct FxTweetResponse {
    tweet: Option<FxTweetData>,
}

#[derive(Deserialize, Debug)]
struct FxTweetData {
    text: Option<String>,
    author: Option<FxTweetAuthor>,
    created_at: Option<String>,
    likes: Option<i64>,
    replies: Option<i64>,
    retweets: Option<i64>,
    article: Option<FxTweetArticle>,
    media: Option<FxTweetMedia>,
}

#[derive(Deserialize, Debug)]
struct FxTweetAuthor {
    name: Option<String>,
    screen_name: Option<String>,
}

#[derive(Deserialize, Debug)]
struct FxTweetMedia {
    photos: Option<Vec<FxTweetPhoto>>,
}

#[derive(Deserialize, Debug)]
struct FxTweetPhoto {
    url: Option<String>,
}

#[derive(Deserialize, Debug)]
struct FxTweetArticle {
    title: Option<String>,
    content: Option<FxArticleContent>,
}

#[derive(Deserialize, Debug)]
struct FxArticleContent {
    blocks: Option<Vec<FxArticleBlock>>,
}

#[derive(Deserialize, Debug)]
struct FxArticleBlock {
    text: Option<String>,
    #[serde(rename = "type")]
    block_type: Option<String>,
}

const FXTWITTER_API_URL: &str = "https://api.fxtwitter.com";

/// Fetch tweet data from FxTwitter API (supports full article content)
async fn fetch_tweet(client: &reqwest::Client, url: &str) -> Result<FxTweetData, AppError> {
    // Extract user/status/id path from the original URL
    // e.g. https://x.com/Vtrivedy10/status/2023805578561060992
    let path = url
        .replace("https://x.com/", "")
        .replace("https://twitter.com/", "")
        .replace("http://x.com/", "")
        .replace("http://twitter.com/", "");

    let api_url = format!("{}/{}", FXTWITTER_API_URL, path);

    let res = client
        .get(&api_url)
        .header("User-Agent", "NixonNote/1.0")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch tweet: {e}")))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(AppError::BadRequest(format!(
            "FxTwitter API returned {} for {}",
            status, url
        )));
    }

    let fx_response: FxTweetResponse = res
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse tweet JSON: {e}")))?;

    fx_response
        .tweet
        .ok_or_else(|| AppError::BadRequest("Tweet not found".to_string()))
}

/// Convert FxTwitter article blocks (Draft.js format) to markdown
fn article_blocks_to_markdown(blocks: &[FxArticleBlock]) -> String {
    let mut lines: Vec<String> = Vec::new();

    for block in blocks {
        let text = block.text.as_deref().unwrap_or("");
        let block_type = block.block_type.as_deref().unwrap_or("unstyled");

        match block_type {
            "header-one" => lines.push(format!("## {}\n", text)),
            "header-two" => lines.push(format!("### {}\n", text)),
            "header-three" => lines.push(format!("#### {}\n", text)),
            "ordered-list-item" => lines.push(format!("1. {}", text)),
            "unordered-list-item" => lines.push(format!("- {}", text)),
            "blockquote" => lines.push(format!("> {}\n", text)),
            "atomic" => {} // media embeds — skip
            _ => {
                // "unstyled" = normal paragraph
                if !text.trim().is_empty() {
                    lines.push(format!("{}\n", text));
                }
            }
        }
    }

    lines.join("\n")
}

/// Format a tweet creation date to a short human-readable form.
/// Handles both "Tue Feb 17 17:03:45 +0000 2026" and ISO "2026-02-17T17:03:45.000Z".
fn format_tweet_date(created_at: &str) -> String {
    // ISO 8601: "2026-02-17T17:03:45.000Z"
    if created_at.contains('T') && created_at.contains('-') {
        let date_part = &created_at[..10]; // "2026-02-17"
        let parts: Vec<&str> = date_part.split('-').collect();
        if parts.len() == 3 {
            let month = match parts[1] {
                "01" => "Jan",
                "02" => "Feb",
                "03" => "Mar",
                "04" => "Apr",
                "05" => "May",
                "06" => "Jun",
                "07" => "Jul",
                "08" => "Aug",
                "09" => "Sep",
                "10" => "Oct",
                "11" => "Nov",
                "12" => "Dec",
                _ => parts[1],
            };
            return format!("{} {} {}", month, parts[2], parts[0]);
        }
    }

    // Old Twitter format: "Tue Feb 17 17:03:45 +0000 2026"
    let parts: Vec<&str> = created_at.split_whitespace().collect();
    if parts.len() >= 6 {
        format!("{} {} {}", parts[1], parts[2], parts[5])
    } else {
        created_at.to_string()
    }
}

/// Ingest a tweet via the FxTwitter API and save as a note
async fn ingest_tweet(
    state: &AppState,
    body: &UrlIngestRequest,
) -> Result<Json<UrlIngestResponse>, AppError> {
    let tweet = fetch_tweet(&state.client, &body.url).await?;

    let author = tweet.author.as_ref();
    let handle = author
        .and_then(|a| a.screen_name.as_deref())
        .unwrap_or("unknown");
    let display_name = author.and_then(|a| a.name.as_deref()).unwrap_or(handle);
    let tweet_text = tweet.text.as_deref().unwrap_or("");
    let date_str = tweet
        .created_at
        .as_deref()
        .map(format_tweet_date)
        .unwrap_or_default();

    // Build metadata line
    let mut meta_parts = vec![format!("[@{}](https://x.com/{})", handle, handle)];
    if !date_str.is_empty() {
        meta_parts.push(date_str);
    }
    if let Some(likes) = tweet.likes {
        meta_parts.push(format!("{} likes", likes));
    }
    if let Some(replies) = tweet.replies {
        meta_parts.push(format!("{} replies", replies));
    }
    if let Some(rts) = tweet.retweets {
        meta_parts.push(format!("{} retweets", rts));
    }
    meta_parts.push(format!("[View on X]({})", body.url));

    let meta_line = meta_parts.join(" · ");

    // Build full article content if present
    let article_body = tweet
        .article
        .as_ref()
        .and_then(|a| a.content.as_ref())
        .and_then(|c| c.blocks.as_ref())
        .map(|blocks| article_blocks_to_markdown(blocks))
        .filter(|s| !s.is_empty());

    let article_title = tweet.article.as_ref().and_then(|a| a.title.as_deref());

    // Build photos section
    let photos_section = tweet
        .media
        .as_ref()
        .and_then(|m| m.photos.as_ref())
        .map(|photos| {
            photos
                .iter()
                .filter_map(|p| p.url.as_deref())
                .map(|url| format!("![]({})", url))
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .filter(|s| !s.is_empty())
        .map(|s| format!("\n\n{}", s))
        .unwrap_or_default();

    // Build note content depending on whether this is an article or plain tweet
    let content = if let Some(article_md) = &article_body {
        let title = article_title.unwrap_or("Article");
        format!(
            "# {}\n\n*By {} ([@{}](https://x.com/{}))*\n\n{}\n\n---\n\n{}{}\n",
            title, display_name, handle, handle, article_md, meta_line, photos_section
        )
    } else {
        // Plain tweet: quote the text
        let quote = if tweet_text.is_empty() {
            String::new()
        } else {
            format!("> {}\n\n", tweet_text)
        };
        format!(
            "# {} (@{})\n\n{}— {}{}\n",
            display_name, handle, quote, meta_line, photos_section
        )
    };

    let title_text = if let Some(t) = article_title {
        t
    } else {
        tweet_text
    };
    let title = format!("@{}: {}", handle, truncate_text(title_text, 80));
    let word_count = content.split_whitespace().count();

    // Insert note
    let content_clone = content.clone();
    let url_for_insert = body.url.clone();
    let note_id = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                queries::insert_note(conn, &content_clone, "text", "tweet", Some(&url_for_insert))
            })
            .await,
    )?;

    // Apply tags
    let mut tags = body.tags.clone().unwrap_or_default();
    if !tags.iter().any(|t| t == "tweet") {
        tags.push("tweet".to_string());
    }
    crate::routes::notes::add_tags_to_note(&state.pool, note_id, tags).await?;

    // Queue for background processing
    let _ = state.background.enqueue(note_id).await;

    Ok(Json(UrlIngestResponse {
        note_id,
        title,
        url: body.url.clone(),
        word_count,
    }))
}

/// Truncate text to a maximum length, adding "..." if truncated
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len).collect();
        format!("{}...", truncated.trim_end())
    }
}

/// Remove low-value citation/link clutter from clipped article Markdown.
///
/// Keep this conservative: preserve normal inline links, but strip known trailing
/// reference sections and obvious trailing link farms that otherwise become token waste.
fn clean_clipped_markdown(markdown: &str) -> String {
    let without_reference_sections = remove_trailing_reference_section(markdown);
    remove_trailing_link_farm(&without_reference_sections)
        .trim()
        .to_string()
}

fn remove_trailing_reference_section(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    let Some(start) = lines.iter().position(|line| is_reference_heading(line)) else {
        return markdown.to_string();
    };

    lines[..start].join("\n").trim_end().to_string()
}

fn is_reference_heading(line: &str) -> bool {
    let heading = line
        .trim()
        .trim_start_matches('#')
        .trim()
        .trim_matches([':', '.', '-'])
        .to_lowercase();

    matches!(
        heading.as_str(),
        "works cited"
            | "work cited"
            | "references"
            | "sources"
            | "bibliography"
            | "citations"
            | "further reading"
            | "external links"
    )
}

fn remove_trailing_link_farm(markdown: &str) -> String {
    let mut blocks: Vec<&str> = markdown.split("\n\n").collect();

    while let Some(block) = blocks.last() {
        if !is_link_farm_block(block) {
            break;
        }
        blocks.pop();
    }

    blocks.join("\n\n").trim_end().to_string()
}

fn is_link_farm_block(block: &str) -> bool {
    let meaningful_lines: Vec<&str> = block
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if meaningful_lines.len() < 3 {
        return false;
    }

    let link_lines = meaningful_lines
        .iter()
        .filter(|line| is_list_like_link_line(line))
        .count();

    link_lines >= 3 && link_lines * 2 >= meaningful_lines.len()
}

fn is_list_like_link_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let listish = trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit());

    listish
        && (trimmed.contains("](") || trimmed.contains("http://") || trimmed.contains("https://"))
}

// ============================================
// URL Article Clipping
// ============================================

#[derive(Deserialize)]
pub struct UrlIngestRequest {
    pub url: String,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct UrlIngestResponse {
    pub note_id: i64,
    pub title: String,
    pub url: String,
    pub word_count: usize,
}

/// POST /api/ingest/url
/// Fetch a URL, extract article content, convert to Markdown, save as note
pub async fn ingest_url(
    State(state): State<AppState>,
    Json(body): Json<UrlIngestRequest>,
) -> Result<Json<UrlIngestResponse>, AppError> {
    use dom_smoothie::Readability;

    // 1. Validate URL
    if !body.url.starts_with("http://") && !body.url.starts_with("https://") {
        return Err(AppError::BadRequest(
            "URL must start with http:// or https://".to_string(),
        ));
    }

    // 2. Check for duplicate
    let url_clone = body.url.clone();
    let existing = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| queries::find_note_by_source_url(conn, &url_clone))
            .await,
    )?;
    if let Some(note) = existing {
        return Err(AppError::BadRequest(format!(
            "Article already clipped as note {}",
            note.id
        )));
    }

    // 2b. Branch to Twitter handler if applicable
    if is_twitter_url(&body.url) {
        return ingest_tweet(&state, &body).await;
    }

    // 3. Fetch HTML
    let response = state
        .client
        .get(&body.url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch URL: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "Failed to fetch article: {} {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown")
        )));
    }

    let html = response
        .text()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read response body: {e}")))?;

    // 4-7. Parse article and convert to markdown (synchronous, non-Send types)
    let (title, content, word_count) = {
        let mut readability = Readability::new(html, Some(&body.url), None)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse HTML: {e}")))?;

        if !readability.is_probably_readable() {
            return Err(AppError::BadRequest(
                "Page does not appear to contain a readable article".to_string(),
            ));
        }

        let article = readability
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to extract article: {e}")))?;

        let title = article.title.clone();
        let article_html = article.content.to_string();

        let markdown = htmd::convert(&article_html).map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to convert to Markdown: {e}"))
        })?;
        let markdown = clean_clipped_markdown(&markdown);

        let domain = body
            .url
            .split("//")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .unwrap_or(&body.url);

        let mut meta_line = format!("> Clipped from [{}]({})", domain, body.url);
        if let Some(ref byline) = article.byline {
            meta_line.push_str(&format!("\n> By {}", byline));
        }
        if let Some(ref published) = article.published_time {
            meta_line.push_str(&format!(" | Published {}", published));
        }

        let content = format!("# {}\n\n{}\n\n---\n\n{}", title, meta_line, markdown.trim());
        let word_count = content.split_whitespace().count();

        (title, content, word_count)
    };

    // 8. Insert note
    let content_clone = content.clone();
    let url_for_insert = body.url.clone();
    let note_id = flatten_interact(
        state
            .pool
            .get()
            .await
            .map_err(anyhow::Error::from)?
            .interact(move |conn| {
                queries::insert_note(
                    conn,
                    &content_clone,
                    "text",
                    "web-clip",
                    Some(&url_for_insert),
                )
            })
            .await,
    )?;

    // 9. Apply tags
    let tags = body.tags.unwrap_or_else(|| vec!["web-clip".to_string()]);
    crate::routes::notes::add_tags_to_note(&state.pool, note_id, tags).await?;

    // 10. Enqueue for background processing
    let _ = state.background.enqueue(note_id).await;

    Ok(Json(UrlIngestResponse {
        note_id,
        title,
        url: body.url,
        word_count,
    }))
}

// ============================================
// Error Handling
// ============================================

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AppError::Internal(err) => {
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
            }
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_video_id_standard_url() {
        let id = extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(id, "dQw4w9WgXcQ");
    }

    #[test]
    fn test_extract_video_id_short_url() {
        let id = extract_video_id("https://youtu.be/dQw4w9WgXcQ").unwrap();
        assert_eq!(id, "dQw4w9WgXcQ");
    }

    #[test]
    fn test_extract_video_id_short_url_with_params() {
        let id = extract_video_id("https://youtu.be/dQw4w9WgXcQ?t=42").unwrap();
        assert_eq!(id, "dQw4w9WgXcQ");
    }

    #[test]
    fn test_extract_video_id_embed_url() {
        let id = extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ").unwrap();
        assert_eq!(id, "dQw4w9WgXcQ");
    }

    #[test]
    fn test_extract_video_id_mobile_url() {
        let id = extract_video_id("https://m.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(id, "dQw4w9WgXcQ");
    }

    #[test]
    fn test_extract_video_id_with_extra_params() {
        let id = extract_video_id(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf",
        )
        .unwrap();
        assert_eq!(id, "dQw4w9WgXcQ");
    }

    #[test]
    fn test_extract_video_id_invalid_url() {
        let result = extract_video_id("https://example.com/article");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_video_id_no_v_param() {
        let result = extract_video_id("https://www.youtube.com/watch?list=PLrAXtmErZg");
        assert!(result.is_err());
    }

    #[test]
    fn test_clean_clipped_markdown_removes_works_cited_section() {
        let markdown = r#"Main argument paragraph.

## Works cited

1. [plastic-labs/honcho: Memory library](https://github.com/plastic-labs/honcho), accessed May 14, 2026
2. [Honcho Overview](https://docs.honcho.dev/), accessed May 14, 2026
"#;

        let cleaned = clean_clipped_markdown(markdown);

        assert!(cleaned.contains("Main argument paragraph."));
        assert!(!cleaned.contains("Works cited"));
        assert!(!cleaned.contains("plastic-labs/honcho"));
    }

    #[test]
    fn test_clean_clipped_markdown_removes_trailing_link_farm() {
        let markdown = r#"Useful clipped article content.

1. [GitHub repo](https://github.com/example/repo)
2. [API docs](https://example.com/api)
3. [Related paper](https://arxiv.org/abs/1234.5678)
4. [Project page](https://example.com/project)
"#;

        let cleaned = clean_clipped_markdown(markdown);

        assert_eq!(cleaned, "Useful clipped article content.");
    }

    #[test]
    fn test_clean_clipped_markdown_preserves_small_inline_links() {
        let markdown = "Read the [official docs](https://example.com) before deploying.\n\nThis is still useful.";

        let cleaned = clean_clipped_markdown(markdown);

        assert_eq!(cleaned, markdown);
    }

    #[test]
    fn test_parse_vtt_basic() {
        let vtt = r#"WEBVTT
Kind: captions
Language: en

00:00:00.000 --> 00:00:02.500
Hello world

00:00:02.500 --> 00:00:05.000
This is a test
"#;
        let result = parse_vtt(vtt);
        assert_eq!(result, "Hello world This is a test");
    }

    #[test]
    fn test_parse_vtt_deduplicates() {
        let vtt = r#"WEBVTT

00:00:00.000 --> 00:00:02.000
Hello world

00:00:01.000 --> 00:00:03.000
Hello world

00:00:02.000 --> 00:00:04.000
Something new
"#;
        let result = parse_vtt(vtt);
        assert_eq!(result, "Hello world Something new");
    }

    #[test]
    fn test_parse_vtt_strips_tags() {
        let vtt = r#"WEBVTT

00:00:00.000 --> 00:00:02.000
<c.colorE5E5E5>Hello</c> <c.colorCCCCCC>world</c>

00:00:02.000 --> 00:00:04.000
<b>Bold text</b>
"#;
        let result = parse_vtt(vtt);
        assert_eq!(result, "Hello world Bold text");
    }

    #[test]
    fn test_parse_vtt_empty() {
        assert_eq!(parse_vtt("WEBVTT\n\n"), "");
    }

    #[test]
    fn test_parse_vtt_real_auto_generated() {
        // Simulates real auto-generated YouTube subtitles with overlapping cues
        let vtt = r#"WEBVTT
Kind: captions
Language: en

00:00:00.030 --> 00:00:02.460 align:start position:0%
welcome<00:00:00.510> to<00:00:00.870> the<00:00:01.199> channel

00:00:02.460 --> 00:00:04.910 align:start position:0%
welcome to the channel
today<00:00:02.790> we<00:00:03.060> are<00:00:03.300> going<00:00:03.570> to

00:00:04.910 --> 00:00:07.000 align:start position:0%
today we are going to
talk<00:00:05.220> about<00:00:05.520> Rust
"#;
        let result = parse_vtt(vtt);
        assert_eq!(
            result,
            "welcome to the channel today we are going to talk about Rust"
        );
    }

    // ============================================
    // Gemini Summarization Tests
    // ============================================

    #[tokio::test]
    #[serial_test::serial]
    async fn test_summarize_with_gemini_uses_bearer_auth() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

        let mock_server = MockServer::start().await;

        // Verify the request uses Authorization: Bearer header
        Mock::given(matchers::method("POST"))
            .and(matchers::header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "content": "Test summary" }
                }]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        unsafe {
            std::env::set_var("GEMINI_API_KEY", "test-key-123");
            std::env::set_var("GEMINI_SUMMARIZE_URL", &mock_server.uri());
        }

        let client = reqwest::Client::new();
        let result = summarize_with_gemini(&client, "Test Video", "Some transcript text").await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
        assert_eq!(result.unwrap(), "Test summary");

        unsafe {
            std::env::remove_var("GEMINI_SUMMARIZE_URL");
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_summarize_with_gemini_no_query_param_key() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

        let mock_server = MockServer::start().await;

        // Mount a mock that captures all requests
        Mock::given(matchers::method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "content": "Summary" }
                }]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        unsafe {
            std::env::set_var("GEMINI_API_KEY", "test-key-123");
            std::env::set_var("GEMINI_SUMMARIZE_URL", &mock_server.uri());
        }

        let client = reqwest::Client::new();
        let _ = summarize_with_gemini(&client, "Title", "Transcript").await;

        // Verify the mock received exactly 1 request and check it didn't have ?key= in the URL
        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let req = &requests[0];
        assert!(
            !req.url.query().unwrap_or("").contains("key="),
            "API key should NOT be in query params, got URL: {}",
            req.url
        );
        assert!(
            req.headers.get("Authorization").is_some(),
            "Authorization header must be present"
        );
        let auth_val = req.headers.get("Authorization").unwrap().to_str().unwrap();
        assert!(
            auth_val.starts_with("Bearer "),
            "Auth header should be Bearer token, got: {}",
            auth_val
        );

        unsafe {
            std::env::remove_var("GEMINI_SUMMARIZE_URL");
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_summarize_with_gemini_api_error_returns_err() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

        let mock_server = MockServer::start().await;

        Mock::given(matchers::method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {
                    "code": 400,
                    "message": "Invalid request",
                    "status": "INVALID_ARGUMENT"
                }
            })))
            .mount(&mock_server)
            .await;

        unsafe {
            std::env::set_var("GEMINI_API_KEY", "test-key");
            std::env::set_var("GEMINI_SUMMARIZE_URL", &mock_server.uri());
        }

        let client = reqwest::Client::new();
        let result = summarize_with_gemini(&client, "Title", "Transcript").await;

        assert!(result.is_err(), "Expected Err for 400 response");
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("400"),
            "Error should contain status code, got: {}",
            err_msg
        );

        unsafe {
            std::env::remove_var("GEMINI_SUMMARIZE_URL");
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_summarize_with_gemini_missing_key_returns_err() {
        unsafe {
            std::env::remove_var("GEMINI_API_KEY");
            std::env::remove_var("GEMINI_SUMMARIZE_URL");
        }

        let client = reqwest::Client::new();
        let result = summarize_with_gemini(&client, "Title", "Transcript").await;

        assert!(
            result.is_err(),
            "Expected Err when GEMINI_API_KEY is missing"
        );
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("GEMINI_API_KEY"),
            "Error should mention GEMINI_API_KEY, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_strip_vtt_tags() {
        assert_eq!(strip_vtt_tags("<c.colorE5E5E5>Hello</c>"), "Hello");
        assert_eq!(strip_vtt_tags("<b>Bold</b> text"), "Bold text");
        assert_eq!(strip_vtt_tags("No tags here"), "No tags here");
        assert_eq!(
            strip_vtt_tags("welcome<00:00:00.510> to<00:00:00.870> the"),
            "welcome to the"
        );
    }

    // ============================================
    // Twitter/X.com Ingestion Tests
    // ============================================

    #[test]
    fn test_is_twitter_url_x_com() {
        assert!(is_twitter_url("https://x.com/user/status/1234567890"));
        assert!(is_twitter_url(
            "https://x.com/Vtrivedy10/status/2023805578561060992"
        ));
    }

    #[test]
    fn test_is_twitter_url_twitter_com() {
        assert!(is_twitter_url("https://twitter.com/user/status/1234567890"));
    }

    #[test]
    fn test_is_twitter_url_case_insensitive() {
        assert!(is_twitter_url("https://X.COM/user/status/123"));
        assert!(is_twitter_url("https://Twitter.com/user/status/123"));
    }

    #[test]
    fn test_is_twitter_url_rejects_non_tweet() {
        assert!(!is_twitter_url("https://x.com/user"));
        assert!(!is_twitter_url("https://example.com/status/123"));
        assert!(!is_twitter_url("https://youtube.com/watch?v=abc"));
    }

    #[test]
    fn test_format_tweet_date_old_format() {
        assert_eq!(
            format_tweet_date("Wed Oct 09 14:30:00 +0000 2024"),
            "Oct 09 2024"
        );
    }

    #[test]
    fn test_format_tweet_date_iso_format() {
        assert_eq!(format_tweet_date("2026-02-17T17:03:45.000Z"), "Feb 17 2026");
    }

    #[test]
    fn test_format_tweet_date_fxtwitter_format() {
        assert_eq!(
            format_tweet_date("Tue Feb 17 17:03:45 +0000 2026"),
            "Feb 17 2026"
        );
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 80), "short");
        let long = "a".repeat(100);
        let result = truncate_text(&long, 80);
        assert!(result.len() <= 84); // 80 chars + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_article_blocks_to_markdown_basic() {
        let blocks = vec![
            FxArticleBlock {
                text: Some("Introduction text".to_string()),
                block_type: Some("unstyled".to_string()),
            },
            FxArticleBlock {
                text: Some("Section Title".to_string()),
                block_type: Some("header-one".to_string()),
            },
            FxArticleBlock {
                text: Some("Body paragraph".to_string()),
                block_type: Some("unstyled".to_string()),
            },
        ];
        let result = article_blocks_to_markdown(&blocks);
        assert!(result.contains("Introduction text"));
        assert!(result.contains("## Section Title"));
        assert!(result.contains("Body paragraph"));
    }

    #[test]
    fn test_article_blocks_to_markdown_lists() {
        let blocks = vec![
            FxArticleBlock {
                text: Some("First item".to_string()),
                block_type: Some("ordered-list-item".to_string()),
            },
            FxArticleBlock {
                text: Some("Second item".to_string()),
                block_type: Some("ordered-list-item".to_string()),
            },
            FxArticleBlock {
                text: Some("Bullet".to_string()),
                block_type: Some("unordered-list-item".to_string()),
            },
        ];
        let result = article_blocks_to_markdown(&blocks);
        assert!(result.contains("1. First item"));
        assert!(result.contains("1. Second item"));
        assert!(result.contains("- Bullet"));
    }

    #[test]
    fn test_article_blocks_to_markdown_skips_atomic() {
        let blocks = vec![
            FxArticleBlock {
                text: Some("Before".to_string()),
                block_type: Some("unstyled".to_string()),
            },
            FxArticleBlock {
                text: Some(" ".to_string()),
                block_type: Some("atomic".to_string()),
            },
            FxArticleBlock {
                text: Some("After".to_string()),
                block_type: Some("unstyled".to_string()),
            },
        ];
        let result = article_blocks_to_markdown(&blocks);
        assert!(result.contains("Before"));
        assert!(result.contains("After"));
        // atomic blocks are skipped
        assert!(!result.contains("atomic"));
    }
}
