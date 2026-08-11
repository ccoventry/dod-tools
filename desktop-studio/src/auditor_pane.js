import { listen } from '@tauri-apps/api/event';
import { runDemoAudit, deleteAuditFiles } from './ipc_bridge.js';
import { showToast } from './toast.js';

export function initAuditorPane(getScanPaths) {
  let currentAuditResults = [];

  const runAuditBtn = document.querySelector('#run-audit-btn');
  const deleteBtn = document.querySelector('#delete-selected-audits-btn');
  const selectAllCb = document.querySelector('#audit-select-all');
  const statusEl = document.querySelector('#audit-status');
  const footerEl = document.querySelector('#audit-list-footer');
  const tbody = document.querySelector('#audit-results-body');

  listen('audit_progress', (event) => {
    const p = event.payload;
    if (statusEl) {
      statusEl.textContent = `Status: ${p.status} (${p.files_checked} files checked)`;
    }
  });

  if (runAuditBtn) {
    runAuditBtn.addEventListener('click', async () => {
      const paths = getScanPaths();
      if (!paths || paths.length === 0) {
        showToast("No scan paths configured for auditing.", "error");
        return;
      }

      runAuditBtn.disabled = true;
      if (deleteBtn) deleteBtn.disabled = true;
      if (tbody) tbody.innerHTML = '<tr><td colspan="4" class="table-empty">Auditing in progress...</td></tr>';
      
      try {
        currentAuditResults = await runDemoAudit(paths);
        renderAuditResults();
      } catch (e) {
        if (tbody) tbody.innerHTML = `<tr><td colspan="4" class="table-empty">Audit failed: ${e}</td></tr>`;
      } finally {
        runAuditBtn.disabled = false;
      }
    });
  }

  function renderAuditResults() {
    if (!tbody) return;

    if (currentAuditResults.length === 0) {
      tbody.innerHTML = '<tr><td colspan="4" class="table-empty">No duplicates found! Your demos are clean.</td></tr>';
      if (deleteBtn) deleteBtn.disabled = true;
      if (footerEl) footerEl.textContent = `Duplicates Found: 0 | Wasted Space: 0.00 GB`;
      return;
    }

    tbody.innerHTML = '';
    let totalWastedBytes = 0;
    let duplicateCount = 0;

    currentAuditResults.forEach((group, groupIdx) => {
      const sizeMb = (group.size / (1024 * 1024)).toFixed(2);
      const hashStr = group.header_hash.toString(16);
      
      const wastedBytes = group.size * (group.files.length - 1);
      totalWastedBytes += wastedBytes;
      duplicateCount += (group.files.length - 1);

      group.files.forEach((file, fileIdx) => {
        const tr = document.createElement('tr');
        
        const tdCb = document.createElement('td');
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.className = 'audit-row-cb';
        cb.dataset.file = file;
        
        // Auto-select duplicates (all but the first file in the group)
        if (fileIdx > 0) {
          cb.checked = true;
        } else {
          cb.disabled = true; // Prevent deleting the original
          cb.title = "Original file (kept)";
        }
        
        cb.addEventListener('change', updateDeleteButtonState);
        tdCb.appendChild(cb);

        const tdFile = document.createElement('td');
        tdFile.textContent = `[Group ${groupIdx + 1}] ${file}`;
        if (fileIdx > 0) {
          tdFile.style.color = '#ff9999';
        } else {
          tdFile.style.fontWeight = 'bold';
        }

        const tdSize = document.createElement('td');
        tdSize.textContent = `${sizeMb} MB`;

        const tdHash = document.createElement('td');
        tdHash.textContent = hashStr;

        tr.appendChild(tdCb);
        tr.appendChild(tdFile);
        tr.appendChild(tdSize);
        tr.appendChild(tdHash);
        tbody.appendChild(tr);
      });
    });

    const wastedGb = (totalWastedBytes / (1024 * 1024 * 1024)).toFixed(2);
    if (footerEl) footerEl.textContent = `Duplicates Found: ${duplicateCount} | Wasted Space: ${wastedGb} GB`;

    updateDeleteButtonState();
  }

  function updateDeleteButtonState() {
    if (!deleteBtn) return;
    const checked = document.querySelectorAll('.audit-row-cb:checked');
    deleteBtn.disabled = checked.length === 0;
    deleteBtn.textContent = `Delete ${checked.length} Selected File(s)`;
  }

  if (selectAllCb) {
    selectAllCb.addEventListener('change', (e) => {
      const isChecked = e.target.checked;
      document.querySelectorAll('.audit-row-cb:not([disabled])').forEach(cb => {
        cb.checked = isChecked;
      });
      updateDeleteButtonState();
    });
  }

  if (deleteBtn) {
    deleteBtn.addEventListener('click', async () => {
      const checked = document.querySelectorAll('.audit-row-cb:checked');
      const filesToDelete = Array.from(checked).map(cb => cb.dataset.file);
      
      if (filesToDelete.length === 0) return;
      if (!confirm(`Are you sure you want to permanently delete ${filesToDelete.length} files?`)) return;

      deleteBtn.disabled = true;
      try {
        await deleteAuditFiles(filesToDelete);
        showToast(`Successfully deleted ${filesToDelete.length} duplicate files.`, 'success');
        
        // Remove deleted files from current state to avoid re-auditing immediately
        currentAuditResults = currentAuditResults.map(g => {
          return {
            ...g,
            files: g.files.filter(f => !filesToDelete.includes(f))
          };
        }).filter(g => g.files.length > 1); // Remove group if no duplicates remain

        renderAuditResults();
        if (selectAllCb) selectAllCb.checked = false;
      } catch (e) {
        showToast(`Deletion failed: ${e}`, 'error');
        updateDeleteButtonState();
      }
    });
  }
}
