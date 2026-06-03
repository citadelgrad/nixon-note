const apiBaseUrlInput = document.getElementById('apiBaseUrl');
const apiTokenInput = document.getElementById('apiToken');
const saveBtn = document.getElementById('save');
const testBtn = document.getElementById('test');
const statusEl = document.getElementById('status');

function showStatus(message, type) {
  statusEl.textContent = message;
  statusEl.className = type === 'success' ? 'status-success' : 'status-error';
}

// Load existing values
chrome.storage.local.get(['apiBaseUrl', 'apiToken'], (data) => {
  apiBaseUrlInput.value = data.apiBaseUrl || '';
  apiTokenInput.value = data.apiToken || '';
});

saveBtn.addEventListener('click', async () => {
  const apiBaseUrl = apiBaseUrlInput.value.replace(/\/+$/, '');
  const apiToken = apiTokenInput.value.trim();

  if (!apiBaseUrl) {
    showStatus('API Base URL is required.', 'error');
    return;
  }

  await chrome.storage.local.set({ apiBaseUrl, apiToken });
  showStatus('Settings saved.', 'success');
});

testBtn.addEventListener('click', async () => {
  const apiBaseUrl = apiBaseUrlInput.value.replace(/\/+$/, '');
  const apiToken = apiTokenInput.value.trim();

  if (!apiBaseUrl) {
    showStatus('Enter an API Base URL first.', 'error');
    return;
  }

  testBtn.disabled = true;
  testBtn.textContent = 'Testing...';

  try {
    const headers = { 'Content-Type': 'application/json' };
    if (apiToken) {
      headers['Authorization'] = `Bearer ${apiToken}`;
    }

    const res = await fetch(`${apiBaseUrl}/api/status`, { headers });

    if (res.ok) {
      showStatus('Connected to NixonNote.', 'success');
    } else if (res.status === 401) {
      showStatus('Invalid token (401 Unauthorized).', 'error');
    } else {
      showStatus(`Server returned ${res.status}.`, 'error');
    }
  } catch (err) {
    showStatus(`Connection failed: ${err.message}`, 'error');
  } finally {
    testBtn.disabled = false;
    testBtn.textContent = 'Test Connection';
  }
});
