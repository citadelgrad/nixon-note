const btn = document.getElementById('clipBtn');
const statusEl = document.getElementById('status');
const urlEl = document.getElementById('pageUrl');
const existingActionsEl = document.getElementById('existingActions');
const openExistingBtn = document.getElementById('openExistingBtn');

let currentTab;
let apiBaseUrl = '';
let duplicateNoteId = null;

function showConfigPrompt(container) {
  container.textContent = '';
  container.appendChild(document.createTextNode('Set up API URL and token in '));
  const link = document.createElement('a');
  link.href = '#';
  link.textContent = 'settings';
  link.addEventListener('click', (e) => {
    e.preventDefault();
    chrome.runtime.openOptionsPage();
  });
  container.appendChild(link);
  container.appendChild(document.createTextNode('.'));
  container.className = 's-unconfigured';
  container.style.display = '';
}

chrome.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
  currentTab = tab;
  urlEl.textContent = tab?.url || '';

  chrome.storage.local.get(['apiBaseUrl'], (data) => {
    apiBaseUrl = (data.apiBaseUrl || '').replace(/\/+$/, '');
    if (!apiBaseUrl) {
      btn.disabled = true;
      showConfigPrompt(statusEl);
    } else {
      btn.focus();
    }
  });
});

function showStatus(message, className) {
  statusEl.textContent = message;
  statusEl.className = className;
  statusEl.style.display = '';
}

function hideDuplicateAction() {
  duplicateNoteId = null;
  existingActionsEl.style.display = 'none';
}

function showDuplicateAction(noteId, message = '') {
  let parsedId = Number.isFinite(noteId) ? noteId : null;
  if (!parsedId && typeof message === 'string') {
    const match = message.match(/note\s*#?(\d+)/i);
    if (match) {
      const fromMessage = Number.parseInt(match[1], 10);
      parsedId = Number.isFinite(fromMessage) ? fromMessage : null;
    }
  }

  duplicateNoteId = parsedId;
  existingActionsEl.style.display = '';
  openExistingBtn.textContent = duplicateNoteId ? 'Open existing note' : 'Open notes';
}

openExistingBtn.addEventListener('click', () => {
  if (!apiBaseUrl) return;
  const url = duplicateNoteId
    ? `${apiBaseUrl}/#/search?note=${duplicateNoteId}`
    : `${apiBaseUrl}/#/search`;
  chrome.tabs.create({ url });
  window.close();
});

document.getElementById('clipForm').addEventListener('submit', (e) => {
  e.preventDefault();
  if (!currentTab?.url) return;

  btn.disabled = true;
  btn.textContent = 'Sending...';
  statusEl.className = '';
  statusEl.style.display = 'none';
  hideDuplicateAction();

  chrome.runtime.sendMessage(
    { action: 'clip', url: currentTab.url, tabId: currentTab.id },
    (response) => {
      btn.disabled = false;
      btn.textContent = 'Clip to NixonNote';

      if (!response) {
        showStatus('No response from background worker.', 's-error');
        return;
      }

      switch (response.status) {
        case 'success':
          showStatus(response.message, 's-success');
          break;
        case 'duplicate':
          showStatus(response.message, 's-duplicate');
          showDuplicateAction(response.noteId, response.message);
          break;
        case 'unconfigured':
          showConfigPrompt(statusEl);
          break;
        default:
          showStatus(response.message, 's-error');
      }
    }
  );
});
