import init, { analyzeDemo } from './pkg/web_analyzer.js';
import { STRINGS } from './strings.js';
import { setReport, initSubtabs, renderActiveTab } from './render.js';

const dropzone = document.querySelector('#dropzone');
const fileInput = document.querySelector('#file-input');
const fileIndicator = document.querySelector('#current-file');
const tabContent = document.querySelector('#analyzer-tab-content');

let wasmReady = init({ module_or_path: new URL('./pkg/web_analyzer_bg.wasm', import.meta.url) });

function setFileIndicator(text) {
  if (fileIndicator) fileIndicator.textContent = text || STRINGS.ANALYZER.NO_DEMO_LOADED;
}

async function loadDemoFile(file) {
  if (!file) return;
  setFileIndicator(STRINGS.ANALYZER.ANALYZING_ELLIPSIS);
  if (tabContent) tabContent.innerHTML = `<p class="analyzer-empty">${STRINGS.ANALYZER.ANALYZING_DEMO_ELLIPSIS}</p>`;

  try {
    await wasmReady;
    const bytes = new Uint8Array(await file.arrayBuffer());

    const onProgress = (processed, total) => {
      if (!total) return;
      const pct = Math.min(100, Math.round((processed / total) * 100));
      setFileIndicator(STRINGS.ANALYZER.analyzingPct(pct));
      if (tabContent) tabContent.innerHTML = `<p class="analyzer-empty">${STRINGS.ANALYZER.analyzingDemoPct(pct)}</p>`;
    };

    const analysis = analyzeDemo(bytes, onProgress);

    setReport({
      file_name: file.name,
      file_size_mb: file.size / 1_048_576,
      file_created_unix_secs: file.lastModified ? Math.floor(file.lastModified / 1000) : 0,
      demo_info: analysis.demo_info,
      state: analysis.state,
    });
    setFileIndicator(file.name);
    renderActiveTab();
  } catch (err) {
    console.error('Failed to analyze demo:', err);
    if (tabContent) {
      tabContent.innerHTML = `<p class="analyzer-empty" style="color:#f44336;">${STRINGS.ANALYZER.analyzeFailed(String(err))}</p>`;
    }
    setFileIndicator('');
  }
}

dropzone?.addEventListener('click', () => fileInput?.click());
fileInput?.addEventListener('change', () => {
  if (fileInput.files && fileInput.files[0]) loadDemoFile(fileInput.files[0]);
});

['dragenter', 'dragover'].forEach((evt) => {
  dropzone?.addEventListener(evt, (e) => {
    e.preventDefault();
    dropzone.classList.add('dragover');
  });
});
['dragleave', 'drop'].forEach((evt) => {
  dropzone?.addEventListener(evt, (e) => {
    e.preventDefault();
    dropzone.classList.remove('dragover');
  });
});
dropzone?.addEventListener('drop', (e) => {
  const file = e.dataTransfer?.files?.[0];
  if (file) loadDemoFile(file);
});

initSubtabs();
setFileIndicator('');
renderActiveTab();
