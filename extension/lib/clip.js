// URL detection and config helpers

const YOUTUBE_RE = /youtube\.com\/(watch\?v=|embed\/|shorts\/)|youtu\.be\//i;
const TWITTER_RE = /(?:x\.com|twitter\.com)\/.+\/status\//i;

/**
 * Check if a URL is a YouTube video URL.
 */
export function isYouTubeUrl(url) {
  return YOUTUBE_RE.test(url);
}

/**
 * Check if a URL is a Twitter/X tweet URL.
 */
export function isTwitterUrl(url) {
  return TWITTER_RE.test(url);
}

/**
 * Validate that a URL is clippable (http or https).
 */
export function isClippableUrl(url) {
  return /^https?:\/\//i.test(url);
}

/**
 * Get stored API config.
 */
export async function getConfig() {
  return chrome.storage.local.get(['apiBaseUrl', 'apiToken']);
}
