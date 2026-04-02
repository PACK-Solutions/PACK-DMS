// ── State & Auth ──────────────────────────────────────────────────
let currentDocId = null;

function getToken() { return document.getElementById('token').value.trim(); }

function saveToken(v) {
  localStorage.setItem('packdms_token', v);
  document.getElementById('auth-status').textContent = v ? '✓ Token set' : '';
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

// Restore token on load
document.addEventListener('DOMContentLoaded', () => {
  const saved = localStorage.getItem('packdms_token');
  if (saved) { document.getElementById('token').value = saved; saveToken(saved); }
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
