// ── Documents ──────────────────────────────────────────────────────

async function loadDocuments() {
  if (!getToken()) { toast('Set a bearer token first', 'error'); return; }
  try {
    const q = document.getElementById('search-q').value.trim();
    let url = '/documents?limit=100';
    if (q) url += '&q=' + encodeURIComponent(q);
    const docs = await api(url, { headers: headers() });
    const tbody = document.getElementById('doc-table-body');
    if (!docs.length) {
      tbody.innerHTML = '<tr><td colspan="6" class="px-4 py-8 text-center text-sm text-gray-400">No documents found.</td></tr>';
      return;
    }
    tbody.innerHTML = docs.map(d => `
      <tr class="hover:bg-gray-50 cursor-pointer" onclick="openDocDetail('${d.id}')">
        <td class="px-4 py-3 text-sm font-medium text-gray-900">${esc(d.title)}</td>
        <td class="px-4 py-3 text-sm">${statusBadge(d.status)}</td>
        <td class="px-4 py-3 text-sm text-gray-500 font-mono">${shortId(d.owner_id)}</td>
        <td class="px-4 py-3 text-sm">${d.legal_hold ? '<span class="text-red-600 font-semibold">Yes</span>' : 'No'}</td>
        <td class="px-4 py-3 text-sm text-gray-500">${fmtDate(d.updated_at)}</td>
        <td class="px-4 py-3 text-sm">
          <button onclick="event.stopPropagation(); openDocDetail('${d.id}')" class="text-indigo-600 hover:text-indigo-800 font-medium">View</button>
        </td>
      </tr>
    `).join('');
  } catch (e) { toast(e.message, 'error'); }
}

async function createDocument() {
  try {
    const title = document.getElementById('create-title').value.trim();
    const metaStr = document.getElementById('create-metadata').value.trim();
    if (!title) { toast('Title is required', 'error'); return; }
    let metadata;
    try { metadata = JSON.parse(metaStr); } catch { toast('Invalid JSON metadata', 'error'); return; }
    await api('/documents', {
      method: 'POST',
      headers: headers(),
      body: JSON.stringify({ title, metadata })
    });
    closeModals();
    toast('Document created');
    loadDocuments();
  } catch (e) { toast(e.message, 'error'); }
}

// ── Document Detail ────────────────────────────────────────────────

async function openDocDetail(id) {
  currentDocId = id;
  try {
    const doc = await api('/documents/' + id, { headers: headers() });
    document.getElementById('detail-title').textContent = doc.title;

    let versionsHtml = '';
    let aclHtml = '';
    try {
      const versions = await api('/documents/' + id + '/versions', { headers: headers() });
      versionsHtml = renderVersions(id, versions);
    } catch { versionsHtml = '<p class="text-sm text-gray-400">Could not load versions.</p>'; }
    try {
      const acl = await api('/documents/' + id + '/acl', { headers: headers() });
      aclHtml = renderAcl(id, acl);
    } catch { aclHtml = '<p class="text-sm text-gray-400">Could not load ACL.</p>'; }

    document.getElementById('detail-content').innerHTML = `
      <!-- Info Grid -->
      <div class="grid grid-cols-2 gap-4 text-sm">
        <div><span class="font-medium text-gray-500">ID:</span> <span class="font-mono">${doc.id}</span></div>
        <div><span class="font-medium text-gray-500">Status:</span> ${statusBadge(doc.status)}</div>
        <div><span class="font-medium text-gray-500">Owner:</span> <span class="font-mono">${shortId(doc.owner_id)}</span></div>
        <div><span class="font-medium text-gray-500">Legal Hold:</span> ${doc.legal_hold ? '<span class="text-red-600 font-semibold">Yes</span>' : 'No'}</div>
        <div><span class="font-medium text-gray-500">Retention Until:</span> ${doc.retention_until ? fmtDate(doc.retention_until) : '—'}</div>
        <div><span class="font-medium text-gray-500">Created:</span> ${fmtDate(doc.created_at)}</div>
        <div><span class="font-medium text-gray-500">Updated:</span> ${fmtDate(doc.updated_at)}</div>
        <div><span class="font-medium text-gray-500">Archived:</span> ${fmtDate(doc.archived_at)}</div>
      </div>

      <!-- Metadata -->
      <div>
        <h4 class="text-sm font-semibold text-gray-700 mb-2">Metadata</h4>
        <pre class="bg-gray-50 rounded-md p-3 text-xs font-mono overflow-x-auto border">${esc(JSON.stringify(doc.metadata, null, 2))}</pre>
      </div>

      <!-- Actions -->
      <div>
        <h4 class="text-sm font-semibold text-gray-700 mb-2">Actions</h4>
        <div class="flex flex-wrap gap-2">
          ${renderStatusActions(doc)}
          <button onclick="promptPatchDoc('${doc.id}')" class="rounded-md border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50">Edit Metadata</button>
          <button onclick="promptLegalHold('${doc.id}', ${doc.legal_hold})" class="rounded-md border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50">${doc.legal_hold ? 'Remove' : 'Set'} Legal Hold</button>
          <button onclick="promptRetention('${doc.id}')" class="rounded-md border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50">Set Retention</button>
        </div>
      </div>

      <!-- Versions -->
      <div>
        <div class="flex items-center justify-between mb-2">
          <h4 class="text-sm font-semibold text-gray-700">Versions</h4>
          <button onclick="promptUploadVersion('${doc.id}')" class="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700">Upload Version</button>
        </div>
        ${versionsHtml}
      </div>

      <!-- ACL -->
      <div>
        <div class="flex items-center justify-between mb-2">
          <h4 class="text-sm font-semibold text-gray-700">Access Control List</h4>
          <button onclick="promptEditAcl('${doc.id}')" class="rounded-md border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50">Edit ACL</button>
        </div>
        ${aclHtml}
      </div>
    `;
    document.getElementById('modal-detail').classList.remove('hidden');
  } catch (e) { toast(e.message, 'error'); }
}

function renderStatusActions(doc) {
  const transitions = {
    draft: ['active', 'deleted'],
    active: ['archived', 'deleted'],
    archived: ['active', 'deleted'],
    deleted: [],
  };
  const allowed = transitions[doc.status] || [];
  let btns = '';
  if (doc.status === 'deleted') {
    btns += `<button onclick="restoreDoc('${doc.id}')" class="rounded-md bg-green-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-green-700">Restore</button>`;
  }
  for (const s of allowed) {
    const color = s === 'deleted' ? 'bg-red-600 hover:bg-red-700' : 'bg-indigo-600 hover:bg-indigo-700';
    btns += `<button onclick="changeStatus('${doc.id}','${s}')" class="rounded-md ${color} px-3 py-1.5 text-xs font-medium text-white">→ ${s}</button>`;
  }
  return btns;
}

// ── Document Actions ───────────────────────────────────────────────

async function changeStatus(id, status) {
  try {
    await api('/documents/' + id + '/status', {
      method: 'POST', headers: headers(),
      body: JSON.stringify({ status })
    });
    toast('Status changed to ' + status);
    openDocDetail(id);
    loadDocuments();
  } catch (e) { toast(e.message, 'error'); }
}

async function restoreDoc(id) {
  try {
    await api('/documents/' + id + '/restore', { method: 'POST', headers: headers() });
    toast('Document restored');
    openDocDetail(id);
    loadDocuments();
  } catch (e) { toast(e.message, 'error'); }
}

let _editMetaDocId = null;

function promptPatchDoc(id) {
  _editMetaDocId = id;
  document.getElementById('edit-meta-title').value = '';
  document.getElementById('edit-meta-json').value = '';
  document.getElementById('modal-edit-metadata').classList.remove('hidden');
}

function closeEditMetadataModal() {
  document.getElementById('modal-edit-metadata').classList.add('hidden');
  _editMetaDocId = null;
}

function submitPatchDoc() {
  const title = document.getElementById('edit-meta-title').value.trim();
  const metaStr = document.getElementById('edit-meta-json').value.trim();
  const body = {};
  if (title) body.title = title;
  if (metaStr) {
    try { body.metadata = JSON.parse(metaStr); } catch { toast('Invalid JSON', 'error'); return; }
  }
  if (!Object.keys(body).length) { closeEditMetadataModal(); return; }
  const docId = _editMetaDocId;
  api('/documents/' + docId, { method: 'PATCH', headers: headers(), body: JSON.stringify(body) })
    .then(() => { closeEditMetadataModal(); toast('Document updated'); openDocDetail(docId); loadDocuments(); })
    .catch(e => toast(e.message, 'error'));
}

async function promptLegalHold(id, current) {
  try {
    await api('/documents/' + id + '/legal-hold', {
      method: 'POST', headers: headers(),
      body: JSON.stringify({ hold: !current })
    });
    toast('Legal hold ' + (!current ? 'enabled' : 'removed'));
    openDocDetail(id);
    loadDocuments();
  } catch (e) { toast(e.message, 'error'); }
}

let _retentionDocId = null;

function promptRetention(id) {
  _retentionDocId = id;
  document.getElementById('retention-date').value = '';
  document.getElementById('modal-retention').classList.remove('hidden');
}

function closeRetentionModal() {
  document.getElementById('modal-retention').classList.add('hidden');
  _retentionDocId = null;
}

function submitRetention() {
  const val = document.getElementById('retention-date').value;
  const body = { retention_until: val ? new Date(val).toISOString() : null };
  const docId = _retentionDocId;
  api('/documents/' + docId + '/retention', { method: 'POST', headers: headers(), body: JSON.stringify(body) })
    .then(() => { closeRetentionModal(); toast('Retention updated'); openDocDetail(docId); loadDocuments(); })
    .catch(e => toast(e.message, 'error'));
}
