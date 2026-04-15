// ── ACL ────────────────────────────────────────────────────────────

function renderAcl(docId, acl) {
  if (!acl.length) return '<p class="text-sm text-gray-400">No ACL rules defined.</p>';
  return `<div class="overflow-x-auto rounded-md border border-gray-200">
    <table class="min-w-full divide-y divide-gray-200 text-sm">
      <thead class="bg-gray-50"><tr>
        <th class="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
        <th class="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Principal</th>
        <th class="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Permission</th>
      </tr></thead>
      <tbody class="divide-y divide-gray-200 bg-white">
        ${acl.map(a => `<tr>
          <td class="px-3 py-2">${esc(a.principal_type)}</td>
          <td class="px-3 py-2 font-mono text-gray-600 text-xs">${a.principal_id ? esc(a.principal_id) : esc(a.role || '—')}</td>
          <td class="px-3 py-2">${esc(a.permission)}</td>
        </tr>`).join('')}
      </tbody>
    </table>
  </div>`;
}

let _aclDocId = null;

function promptEditAcl(docId) {
  _aclDocId = docId;
  document.getElementById('edit-acl-json').value = '';
  document.getElementById('modal-edit-acl').classList.remove('hidden');
}

function closeEditAclModal() {
  document.getElementById('modal-edit-acl').classList.add('hidden');
  _aclDocId = null;
}

function submitEditAcl() {
  const json = document.getElementById('edit-acl-json').value.trim();
  if (!json) { closeEditAclModal(); return; }
  let rules;
  try { rules = JSON.parse(json); } catch { toast('Invalid JSON', 'error'); return; }
  rules = rules.map(r => ({
    id: '00000000-0000-0000-0000-000000000000',
    document_id: _aclDocId,
    principal_type: r.principal_type || 'role',
    principal_id: r.principal_id || null,
    role: r.role || null,
    permission: r.permission || 'read',
  }));
  const docId = _aclDocId;
  api('/documents/' + docId + '/acl', { method: 'PUT', headers: headers(), body: JSON.stringify(rules) })
    .then(() => { closeEditAclModal(); toast('ACL updated'); openDocDetail(docId); })
    .catch(e => toast(e.message, 'error'));
}
