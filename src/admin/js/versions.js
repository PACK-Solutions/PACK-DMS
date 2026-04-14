// ── Versions ──────────────────────────────────────────────────────

function renderVersions(docId, versions) {
  if (!versions.length) return '<p class="text-sm text-gray-400">No versions uploaded yet.</p>';
  return `<div class="overflow-hidden rounded-md border border-gray-200">
    <table class="min-w-full table-fixed divide-y divide-gray-200 text-sm">
      <thead class="bg-gray-50"><tr>
        <th class="w-[8%] px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Version</th>
        <th class="w-[28%] px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Filename</th>
        <th class="w-[10%] px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Size</th>
        <th class="w-[10%] px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Status</th>
        <th class="w-[18%] px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Created</th>
        <th class="w-[26%] px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">Actions</th>
      </tr></thead>
      <tbody class="divide-y divide-gray-200 bg-white">
        ${versions.map(v => `<tr>
          <td class="px-3 py-2 font-medium">v${v.version_number}</td>
          <td class="px-3 py-2 text-gray-600 truncate" title="${esc(v.original_filename)}">${esc(v.original_filename)}</td>
          <td class="px-3 py-2 text-gray-500">${formatBytes(v.size_bytes)}</td>
          <td class="px-3 py-2">${statusBadge(v.status)}</td>
          <td class="px-3 py-2 text-gray-500">${fmtDate(v.created_at)}</td>
          <td class="px-3 py-2 space-x-2 whitespace-nowrap">
            ${isPreviewable(v.mime_type) ? `<button onclick="previewVersion('${docId}','${v.id}','${esc(v.mime_type)}','${esc(v.original_filename)}')" class="text-emerald-600 hover:text-emerald-800 font-medium">Preview</button>` : ''}
            <button onclick="downloadVersion('${docId}','${v.id}','${esc(v.original_filename)}')" class="text-indigo-600 hover:text-indigo-800 font-medium">Download</button>
            ${v.status !== 'deleted' ? `<button onclick="deleteVersion('${docId}','${v.id}')" class="text-red-600 hover:text-red-800 font-medium">Delete</button>` : ''}
          </td>
        </tr>`).join('')}
      </tbody>
    </table>
  </div>`;
}

let _uploadDocId = null;

function promptUploadVersion(docId) {
  _uploadDocId = docId;
  document.getElementById('upload-version-file').value = '';
  document.getElementById('modal-upload-version').classList.remove('hidden');
}

function closeUploadVersionModal() {
  document.getElementById('modal-upload-version').classList.add('hidden');
  _uploadDocId = null;
}

async function submitUploadVersion() {
  const input = document.getElementById('upload-version-file');
  const file = input.files[0];
  if (!file) { toast('Please select a file', 'error'); return; }
  const form = new FormData();
  form.append('file', file);
  try {
    const resp = await fetch('/documents/' + _uploadDocId + '/versions', {
      method: 'POST',
      headers: multipartHeaders(),
      body: form
    });
    if (!resp.ok) {
      let msg = resp.statusText;
      try { const b = await resp.json(); msg = b.detail || msg; } catch {}
      throw new Error(msg);
    }
    const docId = _uploadDocId;
    closeUploadVersionModal();
    toast('Version uploaded');
    openDocDetail(docId);
  } catch (e) { toast(e.message, 'error'); }
}

async function downloadVersion(docId, vid, filename) {
  try {
    const resp = await fetch(`/documents/${docId}/versions/${vid}/download`, { headers: headers() });
    if (!resp.ok) {
      let msg = resp.statusText;
      try { const b = await resp.json(); msg = b.detail || msg; } catch {}
      throw new Error(msg);
    }
    const blob = await resp.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename || 'download';
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  } catch (e) { toast(e.message, 'error'); }
}

let _deleteDocId = null;
let _deleteVersionId = null;

function deleteVersion(docId, vid) {
  _deleteDocId = docId;
  _deleteVersionId = vid;
  document.getElementById('modal-confirm-delete').classList.remove('hidden');
}

function closeConfirmDeleteModal() {
  document.getElementById('modal-confirm-delete').classList.add('hidden');
  _deleteDocId = null;
  _deleteVersionId = null;
}

async function confirmDeleteVersion() {
  try {
    await api('/documents/' + _deleteDocId + '/versions/' + _deleteVersionId, { method: 'DELETE', headers: headers() });
    const docId = _deleteDocId;
    closeConfirmDeleteModal();
    toast('Version deleted');
    openDocDetail(docId);
  } catch (e) { toast(e.message, 'error'); }
}
