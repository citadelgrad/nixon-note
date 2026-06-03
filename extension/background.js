import { isYouTubeUrl, isTwitterUrl, isClippableUrl, getConfig } from './lib/clip.js';

// ===== Badge Feedback =====

function showBadge(tabId, text, color, duration = 3000, title = '') {
  chrome.action.setBadgeBackgroundColor({ color });
  chrome.action.setBadgeText({ tabId, text });
  if (title) {
    chrome.action.setTitle({ tabId, title });
  }
  setTimeout(() => {
    chrome.action.setBadgeText({ tabId, text: '' });
    chrome.action.setTitle({ tabId, title: 'NixonNote Clipper' });
  }, duration);
}

// ===== Core Clip Logic =====

const ERROR_MESSAGES = {
  401: 'Invalid token — check settings.',
};

async function clipUrl(url) {
  if (!isClippableUrl(url)) {
    return { status: 'invalid', message: "Can't clip this page." };
  }

  const { apiBaseUrl, apiToken } = await getConfig();
  if (!apiBaseUrl) {
    return { status: 'unconfigured', message: 'Set up API URL and token in settings.' };
  }

  const isYT = isYouTubeUrl(url);
  const isTweet = isTwitterUrl(url);
  const endpoint = isYT ? '/api/ingest/youtube' : '/api/ingest/url';
  const body = isYT ? { url } : { url, tags: isTweet ? ['tweet', 'browser-clip'] : ['browser-clip'] };

  const headers = { 'Content-Type': 'application/json' };
  if (apiToken) {
    headers['Authorization'] = `Bearer ${apiToken}`;
  }

  let res;
  try {
    res = await fetch(`${apiBaseUrl}${endpoint}`, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    });
  } catch {
    return { status: 'error', message: 'Network error — is NixonNote running?' };
  }

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    const duplicateMatch = text.match(/already clipped as note\s+(\d+)/i);
    if (duplicateMatch) {
      const noteId = Number.parseInt(duplicateMatch[1], 10);
      return {
        status: 'duplicate',
        message: Number.isFinite(noteId) ? `Already clipped (note #${noteId}).` : 'Already clipped.',
        noteId: Number.isFinite(noteId) ? noteId : undefined,
      };
    }
    if (text.toLowerCase().includes('already clipped')) {
      return { status: 'duplicate', message: 'Already clipped.' };
    }
    return {
      status: res.status === 401 ? 'auth_error' : 'error',
      message: ERROR_MESSAGES[res.status] || `Server error (${res.status}).`,
    };
  }

  const data = await res.json();
  return { status: 'success', message: `Clipped: ${data.title || 'Untitled'}`, data };
}

async function handleClip(url, tabId) {
  const result = await clipUrl(url);

  if (tabId) {
    if (result.status === 'success') {
      showBadge(tabId, '✓', '#22c55e', 3000, result.message);
    } else if (result.status === 'duplicate') {
      showBadge(tabId, '=', '#f59e0b', 5000, result.message);
    } else if (result.status === 'auth_error' || result.status === 'error') {
      showBadge(tabId, '!', '#ef4444', 5000, result.message);
    }
  }

  return result;
}

// ===== Context Menu =====

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: 'clip-page',
    title: 'Clip this page to NixonNote',
    contexts: ['page'],
    documentUrlPatterns: ['http://*/*', 'https://*/*'],
  });
  chrome.contextMenus.create({
    id: 'clip-link',
    title: 'Clip this link to NixonNote',
    contexts: ['link'],
    documentUrlPatterns: ['http://*/*', 'https://*/*'],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.menuItemId === 'clip-link' ? info.linkUrl : info.pageUrl;
  if (url) await handleClip(url, tab?.id);
});

// ===== Keyboard Shortcut =====

chrome.commands.onCommand.addListener(async (command) => {
  if (command !== 'clip-current-page') return;
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (tab?.url) await handleClip(tab.url, tab.id);
});

// ===== Message handler for popup =====

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg.action === 'clip') {
    handleClip(msg.url, msg.tabId)
      .then(sendResponse)
      .catch(err => sendResponse({ status: 'error', message: err.message }));
    return true; // keep channel open for async response
  }
});
