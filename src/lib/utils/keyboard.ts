export function isTypingTarget(el: Element | null): boolean {
  if (!el) return false;

  const node = el as HTMLElement;
  const tag = node.tagName;

  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (node.isContentEditable) return true;

  return false;
}

export function isPrintableKey(
  e: KeyboardEvent,
  opts?: {
    /** Treat space as non-printable (useful for avoiding scroll/pause conflicts). */
    excludeSpace?: boolean;
  }
): boolean {
  if (e.ctrlKey || e.metaKey || e.altKey) return false;
  if (e.key.length !== 1) return false;

  if (opts?.excludeSpace && e.key === ' ') return false;

  // Filter out other control-ish characters.
  if (e.key === '\n' || e.key === '\r' || e.key === '\t') return false;

  return true;
}

export function matchesShortcut(
  e: KeyboardEvent,
  shortcut: {
    key: string;
    /** Matches Ctrl on Windows/Linux and ⌘ on macOS. */
    mod?: boolean;
    ctrl?: boolean;
    meta?: boolean;
    shift?: boolean;
    alt?: boolean;
  }
): boolean {
  const keyMatches = e.key.toLowerCase() === shortcut.key.toLowerCase();
  if (!keyMatches) return false;

  const modDown = e.ctrlKey || e.metaKey;
  if (shortcut.mod !== undefined && shortcut.mod !== modDown) return false;
  if (shortcut.ctrl !== undefined && shortcut.ctrl !== e.ctrlKey) return false;
  if (shortcut.meta !== undefined && shortcut.meta !== e.metaKey) return false;
  if (shortcut.shift !== undefined && shortcut.shift !== e.shiftKey) return false;
  if (shortcut.alt !== undefined && shortcut.alt !== e.altKey) return false;

  return true;
}

export function hasOpenAriaModal(): boolean {
  return Boolean(document.querySelector('[role="dialog"][aria-modal="true"]'));
}

export function hasOpenAriaMenu(): boolean {
  // Used to avoid capture-phase page shortcuts stealing Escape/typing from open menus.
  return Boolean(document.querySelector('[role="menu"]'));
}
