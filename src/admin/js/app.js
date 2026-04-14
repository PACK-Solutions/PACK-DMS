// ── State & Auth ──────────────────────────────────────────────────
let currentDocId = null;
let _lastTokenProfile = null; // tracks which profile was last generated

function getToken() { return document.getElementById('token').value.trim(); }

function decodeJwtPayload(token) {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return null;
    const payload = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    return JSON.parse(atob(payload));
  } catch (_) {
    return null;
  }
}

function formatExpiry(exp) {
  if (!exp) return null;
  const d = new Date(exp * 1000);
  const now = Date.now();
  const diff = d.getTime() - now;
  if (diff <= 0) return '⚠ Expired';
  const days = Math.floor(diff / 86400000);
  if (days > 365) return `~${Math.round(days / 365)} years`;
  if (days > 30) return `~${Math.floor(days / 30)} months`;
  if (days > 0) return `${days} day${days > 1 ? 's' : ''}`;
  const hrs = Math.floor(diff / 3600000);
  if (hrs > 0) return `${hrs}h`;
  return `${Math.floor(diff / 60000)}m`;
}

function updateUserInfo(token) {
  const infoEl = document.getElementById('user-info');
  if (!token) { infoEl.classList.add('hidden'); return; }
  const claims = decodeJwtPayload(token);
  if (!claims) { infoEl.classList.add('hidden'); return; }
  document.getElementById('user-id').textContent = claims.sub || '—';
  document.getElementById('user-email').textContent = claims.email || '—';
  document.getElementById('user-scopes').textContent = claims.scope || '—';

  const expiryEl = document.getElementById('token-expiry');
  const expiryVal = document.getElementById('token-expiry-value');
  if (claims.exp) {
    const label = formatExpiry(claims.exp);
    expiryVal.textContent = label;
    expiryVal.className = label.startsWith('⚠') ? 'text-red-600 font-semibold' : '';
    expiryEl.classList.remove('hidden');
  } else {
    expiryEl.classList.add('hidden');
  }

  infoEl.classList.remove('hidden');
}

function saveToken(v) {
  localStorage.setItem('packdms_token', v);
  document.getElementById('auth-status').textContent = v ? '✓ Token set' : '';
  updateUserInfo(v);
  // Enable/disable refresh button
  const refreshBtn = document.getElementById('btn-refresh-token');
  if (refreshBtn) refreshBtn.disabled = !_lastTokenProfile;
}

function headers() {
  return { 'Authorization': 'Bearer ' + getToken(), 'Content-Type': 'application/json' };
}

function multipartHeaders() {
  return { 'Authorization': 'Bearer ' + getToken() };
}

function toggleTokenVisibility() {
  const el = document.getElementById('token');
  el.type = el.type === 'password' ? 'text' : 'password';
}

function copyToken() {
  const token = getToken();
  if (!token) { toast('No token to copy', 'error'); return; }
  navigator.clipboard.writeText(token).then(
    () => toast('Token copied to clipboard'),
    () => toast('Failed to copy', 'error')
  );
}

function clearToken() {
  document.getElementById('token').value = '';
  _lastTokenProfile = null;
  saveToken('');
  toast('Token cleared');
}

function toggleGenerateMenu() {
  const menu = document.getElementById('generate-menu');
  menu.classList.toggle('hidden');
  // Close on outside click
  if (!menu.classList.contains('hidden')) {
    setTimeout(() => {
      document.addEventListener('click', function handler(e) {
        if (!menu.contains(e.target) && e.target.id !== 'btn-generate') {
          menu.classList.add('hidden');
          document.removeEventListener('click', handler);
        }
      });
    }, 0);
  }
}

async function generateToken(profile) {
  document.getElementById('generate-menu').classList.add('hidden');
  document.getElementById('auth-status').textContent = '⏳ Generating…';
  try {
    const resp = await fetch('/admin/api/generate-token?profile=' + encodeURIComponent(profile));
    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(text || resp.statusText);
    }
    const data = await resp.json();
    document.getElementById('token').value = data.token;
    _lastTokenProfile = profile;
    saveToken(data.token);
    document.getElementById('btn-refresh-token').disabled = false;
    toast(`${profile.charAt(0).toUpperCase() + profile.slice(1)} token generated`);
  } catch (e) {
    document.getElementById('auth-status').textContent = '';
    toast('Token generation failed: ' + e.message, 'error');
  }
}

function refreshToken() {
  if (_lastTokenProfile) generateToken(_lastTokenProfile);
}

// Restore token on load
document.addEventListener('DOMContentLoaded', () => {
  const saved = localStorage.getItem('packdms_token');
  if (saved) { document.getElementById('token').value = saved; saveToken(saved); }
  updateUserInfo(getToken());
});

// ── Tabs ───────────────────────────────────────────────────────────
function showTab(name) {
  document.querySelectorAll('.tab-panel').forEach(p => p.classList.add('hidden'));
  document.querySelectorAll('.tab-btn').forEach(b => {
    b.classList.remove('bg-indigo-800', 'text-white');
    b.classList.add('text-indigo-200');
  });
  document.getElementById('panel-' + name).classList.remove('hidden');
  const btn = document.getElementById('tab-' + name);
  btn.classList.add('bg-indigo-800', 'text-white');
  btn.classList.remove('text-indigo-200');
  if (name === 'audit') loadAudit();
}

// ── Toast ──────────────────────────────────────────────────────────
function toast(msg, type = 'success') {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.className = 'fixed bottom-4 right-4 z-50 rounded-lg px-4 py-3 text-sm font-medium text-white shadow-lg ' +
    (type === 'error' ? 'bg-red-600' : 'bg-green-600');
  el.classList.remove('hidden');
  setTimeout(() => el.classList.add('hidden'), 3000);
}

// ── Modals ─────────────────────────────────────────────────────────
function closeModals() {
  document.getElementById('modal-create').classList.add('hidden');
  document.getElementById('modal-detail').classList.add('hidden');
}

function openCreateModal() {
  document.getElementById('create-title').value = '';
  document.getElementById('create-metadata').value = '{}';
  document.getElementById('modal-create').classList.remove('hidden');
}

// ── API helper ─────────────────────────────────────────────────────
async function api(path, opts = {}) {
  const resp = await fetch(path, opts);
  if (!resp.ok) {
    let msg = resp.statusText;
    try { const body = await resp.json(); msg = body.detail || body.title || msg; } catch {}
    throw new Error(msg);
  }
  if (resp.status === 204) return null;
  const ct = resp.headers.get('content-type') || '';
  if (ct.includes('json')) return resp.json();
  return resp;
}
