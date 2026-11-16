function ({ token, marker, protocol, host }) {
  try {
    // Tauri also injects into external verification windows and Windows frames.
    if (window.top !== window || location.protocol !== protocol || location.host !== host) return;
    const storage = window.localStorage;
    if (storage.getItem(marker) === token) return;
    const keys = [];
    for (let index = 0; index < storage.length; index++) {
      const key = storage.key(index);
      if (key !== null && key !== marker) keys.push(key);
    }
    // Persist consumption before deletion: a failed marker write must never
    // repeatedly erase preferences chosen in a later manager session.
    storage.setItem(marker, token);
    if (storage.getItem(marker) !== token) return;
    for (const key of keys) storage.removeItem(key);
  } catch {
    // Storage restrictions must not prevent the manager from starting.
    console.warn('Unable to finish the manager preference reset.');
  }
}
