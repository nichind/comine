/**
 * Generic LRU (Least Recently Used) Cache implementation.
 * 
 * Uses Map's insertion order for O(1) get, set, and delete operations.
 * When the cache exceeds its maximum size, the least recently used entry is evicted.
 * 
 * @template K - Key type
 * @template V - Value type
 * 
 * @example
 * const cache = new LRUCache<string, number>(100);
 * cache.set('key1', 42);
 * cache.get('key1'); // returns 42 and moves 'key1' to most recently used
 */
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
    
    // Move to end (most recently used)
    const value = this.cache.get(key)!;
    this.cache.delete(key);
    this.cache.set(key, value);
    return value;
  }

  set(key: K, value: V): void {
    // If key exists, delete first to update order
    if (this.cache.has(key)) {
      this.cache.delete(key);
    } else if (this.cache.size >= this.maxSize) {
      // Evict oldest (first key)
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

  /** Iterate over entries (oldest to newest) */
  *entries(): IterableIterator<[K, V]> {
    yield* this.cache.entries();
  }

  /** Iterate over keys (oldest to newest) */
  *keys(): IterableIterator<K> {
    yield* this.cache.keys();
  }

  /** Iterate over values (oldest to newest) */
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
    // Ensure size limit compliance after bulk load
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
