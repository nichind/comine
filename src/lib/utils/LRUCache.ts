export class LRUCache<K, V> {
  private readonly cache = new Map<K, V>();
  private readonly maxSize: number;
  private onMutate: (() => void) | null = null;

  constructor(maxSize: number) {
    if (maxSize <= 0) throw new Error('LRU cache size must be positive');
    this.maxSize = maxSize;
  }

  setMutationCallback(cb: () => void): void {
    this.onMutate = cb;
  }

  get(key: K): V | undefined {
    if (!this.cache.has(key)) return undefined;

    const value = this.cache.get(key)!;
    this.cache.delete(key);
    this.cache.set(key, value);
    return value;
  }

  set(key: K, value: V): void {
    if (this.cache.has(key)) {
      this.cache.delete(key);
    } else if (this.cache.size >= this.maxSize) {
      const oldestKey = this.cache.keys().next().value;
      if (oldestKey !== undefined) {
        this.cache.delete(oldestKey);
      }
    }
    this.cache.set(key, value);
    this.onMutate?.();
  }

  has(key: K): boolean {
    return this.cache.has(key);
  }

  delete(key: K): boolean {
    const deleted = this.cache.delete(key);
    if (deleted) {
      this.onMutate?.();
    }
    return deleted;
  }

  clear(): void {
    this.cache.clear();
    this.onMutate?.();
  }

  get size(): number {
    return this.cache.size;
  }

  *entries(): IterableIterator<[K, V]> {
    yield* this.cache.entries();
  }

  *keys(): IterableIterator<K> {
    yield* this.cache.keys();
  }

  *values(): IterableIterator<V> {
    yield* this.cache.values();
  }

  toJSON(): Array<[K, V]> {
    return Array.from(this.cache.entries());
  }

  fromJSON(data: Array<[K, V]>): void {
    this.cache.clear();
    for (const [key, value] of data) {
      this.cache.set(key, value);
    }
    while (this.cache.size > this.maxSize) {
      const oldestKey = this.cache.keys().next().value;
      if (oldestKey !== undefined) {
        this.cache.delete(oldestKey);
      } else {
        break;
      }
    }
    this.onMutate?.();
  }
}
