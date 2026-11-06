class ThumbnailCache {
  private cache = new Map<string, string>();
  private readonly MAX_ENTRIES = 80;

  get(id: string): string | undefined {
    const hit = this.cache.get(id);
    if (hit) {
      // Re-insert for LRU ordering
      this.cache.delete(id);
      this.cache.set(id, hit);
    }
    return hit;
  }

  put(id: string, blobUrl: string) {
    if (this.cache.size >= this.MAX_ENTRIES) {
      const oldestKey = this.cache.keys().next().value;
      if (oldestKey) {
        const urlToRevoke = this.cache.get(oldestKey);
        if (urlToRevoke && urlToRevoke.startsWith('blob:')) {
          URL.revokeObjectURL(urlToRevoke);
        }
        this.cache.delete(oldestKey);
      }
    }
    this.cache.set(id, blobUrl);
  }

  clear() {
    for (const url of this.cache.values()) {
      if (url.startsWith('blob:')) {
        URL.revokeObjectURL(url);
      }
    }
    this.cache.clear();
  }
}

export const thumbs = new ThumbnailCache();
