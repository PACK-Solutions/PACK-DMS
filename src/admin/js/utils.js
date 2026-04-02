// ── Utilities ──────────────────────────────────────────────────────

function esc(s) {
  if (s == null) return '';
  const d = document.createElement('div');
  d.textContent = String(s);
  return d.innerHTML;
}

function formatBytes(b) {
  if (b === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(b) / Math.log(k));
  return parseFloat((b / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function statusBadge(s) {
  const colors = {
    draft: 'bg-yellow-100 text-yellow-800',
    active: 'bg-green-100 text-green-800',
    archived: 'bg-blue-100 text-blue-800',
    deleted: 'bg-red-100 text-red-800',
  };
  return `<span class="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ${colors[s] || 'bg-gray-100 text-gray-800'}">${s}</span>`;
}

function fmtDate(d) {
  if (!d) return '—';
  return new Date(d).toLocaleString();
}

function shortId(id) {
  return id ? id.substring(0, 8) + '…' : '—';
}
