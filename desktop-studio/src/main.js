import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';

window.addEventListener("DOMContentLoaded", () => {
  document.getElementById('select-demo-btn').addEventListener('click', async () => {
    const selected = await open({
      multiple: false,
      filters: [{
        name: 'Half-Life Demo',
        extensions: ['dem']
      }]
    });

    if (selected) {
      document.getElementById('selected-file-path').innerText = selected;
    }
  });

  let responseLogEl = document.querySelector("#response-log");
  document.querySelector("#test-btn").addEventListener("click", async () => {
    responseLogEl.textContent = await invoke("test_bridge", { path: "C:/demos/test.dem" });
  });
});