/** Per-device alarm audio and per-track videos. IndexedDB keeps binary data out of settings JSON. */
async function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("sudo-media", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("files");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

export async function readMedia(key: string): Promise<File | undefined> {
  const db = await open();
  try {
    return await new Promise((resolve, reject) => {
      const request = db.transaction("files", "readonly").objectStore("files").get(key);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  } finally { db.close(); }
}

export async function writeMedia(key: string, file?: File): Promise<void> {
  const db = await open();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("files", "readwrite");
      tx.oncomplete = () => resolve();
      tx.onabort = () => reject(tx.error);
      const store = tx.objectStore("files");
      if (file) store.put(file, key);
      else store.delete(key);
    });
  } finally { db.close(); }
}
