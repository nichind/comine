import { invoke } from '@tauri-apps/api/core';

export async function openFile(filePath: string): Promise<boolean> {
  try {
    return await invoke<boolean>('open_file', { path: filePath });
  } catch (e) {
    console.error('Failed to open file:', e);
    return false;
  }
}

export async function revealFile(filePath: string): Promise<boolean> {
  try {
    return await invoke<boolean>('reveal_file', { path: filePath });
  } catch (e) {
    console.error('Failed to reveal file:', e);
    return false;
  }
}
