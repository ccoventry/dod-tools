import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';

// Windows toasts default to silent when `sound` is omitted from the options
// (tauri-plugin-notification -> notify-rust -> winrt-notification's
// Toast::sound(None) emits <audio silent="true" />), so notify() never sets
// `sound` — no explicit silencing needed. See issue #98.
let permissionGranted = false;

export async function initOsNotifications() {
  permissionGranted = await isPermissionGranted();
  if (!permissionGranted) {
    const permission = await requestPermission();
    permissionGranted = permission === 'granted';
  }
  return permissionGranted;
}

export function notify(title, body) {
  if (!permissionGranted) return;
  sendNotification({ title, body });
}
