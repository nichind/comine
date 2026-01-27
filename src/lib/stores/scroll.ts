const scrollPositions = new Map<string, number>();
export function saveScrollPosition(path: string, position: number): void {
  scrollPositions.set(path, position);
}
export function getScrollPosition(path: string): number {
  return scrollPositions.get(path) ?? 0;
}
export function clearScrollPosition(path: string): void {
  scrollPositions.delete(path);
}
export function clearAllScrollPositions(): void {
  scrollPositions.clear();
}
