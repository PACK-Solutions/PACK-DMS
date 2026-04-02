// ── Preview ───────────────────────────────────────────────────────

const PREVIEWABLE_TYPES = [
  'application/pdf',
  'image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/svg+xml', 'image/bmp',
  'text/plain', 'text/html', 'text/css', 'text/javascript', 'text/csv', 'text/xml',
  'application/json', 'application/xml',
  'video/mp4', 'video/webm',
  'audio/mpeg', 'audio/ogg', 'audio/wav',
];

function isPreviewable(mime) {
  if (!mime) return false;
  return PREVIEWABLE_TYPES.some(t => mime.startsWith(t));
}

function previewVersion(docId, vid, mime, filename) {
  const url = `/documents/${docId}/versions/${vid}/download`;
  const modal = document.getElementById('preview-modal');
  const title = document.getElementById('preview-title');
  const body = document.getElementById('preview-body');
  title.textContent = filename || 'Preview';
  body.innerHTML = '<p class="text-sm text-gray-400">Loading…</p>';
  modal.classList.remove('hidden');

  const authHeaders = { 'Authorization': 'Bearer ' + getToken() };

  if (mime.startsWith('text/') || mime === 'application/json' || mime === 'application/xml') {
    fetch(url, { headers: authHeaders })
      .then(r => { if (!r.ok) throw new Error(r.statusText); return r.text(); })
      .then(text => {
        body.innerHTML = `<pre class="bg-gray-900 text-gray-100 p-4 rounded text-sm overflow-auto" style="max-height:75vh; white-space:pre-wrap; word-break:break-word;">${esc(text)}</pre>`;
      })
      .catch(() => { body.innerHTML = '<p class="text-red-500">Failed to load preview.</p>'; });
  } else {
    fetch(url, { headers: authHeaders })
      .then(r => { if (!r.ok) throw new Error(r.statusText); return r.blob(); })
      .then(blob => {
        const objUrl = URL.createObjectURL(blob);
        if (mime === 'application/pdf') {
          body.innerHTML = `<iframe src="${objUrl}" class="w-full h-full rounded" style="min-height:75vh;"></iframe>`;
        } else if (mime.startsWith('image/')) {
          body.innerHTML = `<div class="flex items-center justify-center h-full"><img src="${objUrl}" alt="${esc(filename)}" class="max-w-full max-h-[75vh] rounded shadow"></div>`;
        } else if (mime.startsWith('video/')) {
          body.innerHTML = `<video controls class="w-full max-h-[75vh] rounded"><source src="${objUrl}" type="${mime}">Your browser does not support video playback.</video>`;
        } else if (mime.startsWith('audio/')) {
          body.innerHTML = `<div class="flex items-center justify-center h-64"><audio controls><source src="${objUrl}" type="${mime}">Your browser does not support audio playback.</audio></div>`;
        } else {
          body.innerHTML = '<p class="text-gray-500">Preview not available for this file type.</p>';
        }
      })
      .catch(() => { body.innerHTML = '<p class="text-red-500">Failed to load preview.</p>'; });
  }
}

function closePreview() {
  const modal = document.getElementById('preview-modal');
  modal.classList.add('hidden');
  document.getElementById('preview-body').innerHTML = '';
}
