declare global {
  interface Window {
    __TAURI__?: object
    __TAURI_INTERNALS__?: object
  }
}

export function isTauriRuntime() {
  return typeof window !== 'undefined' &&
    (window.__TAURI__ !== undefined || window.__TAURI_INTERNALS__ !== undefined)
}
