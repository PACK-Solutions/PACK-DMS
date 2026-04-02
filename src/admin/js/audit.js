// ── Audit ──────────────────────────────────────────────────────────

async function loadAudit() {
  if (!getToken()) { toast('Set a bearer token first', 'error'); return; }
  try {
    const logs = await api('/audit?limit=100', { headers: headers() });
    const tbody = document.getElementById('audit-table-body');
    if (!logs.length) {
      tbody.innerHTML = '<tr><td colspan="5" class="px-4 py-8 text-center text-sm text-gray-400">No audit logs found.</td></tr>';
      return;
    }
    tbody.innerHTML = logs.map(l => `<tr class="hover:bg-gray-50">
      <td class="px-4 py-3 text-sm text-gray-500">${fmtDate(l.created_at)}</td>
      <td class="px-4 py-3 text-sm font-medium">${esc(l.action)}</td>
      <td class="px-4 py-3 text-sm text-gray-600">${esc(l.resource_type)}/${shortId(l.resource_id)}</td>
      <td class="px-4 py-3 text-sm text-gray-500 font-mono">${shortId(l.actor_id)}</td>
      <td class="px-4 py-3 text-sm text-gray-500 font-mono text-xs">${l.details ? esc(JSON.stringify(l.details)) : '—'}</td>
    </tr>`).join('');
  } catch (e) { toast(e.message, 'error'); }
}
