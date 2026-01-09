// System client - Tauri invoke wrappers for system operations

import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// Types
// ============================================================================

export interface PlatformInfo {
  os: string;
  arch: string;
}

// ============================================================================
// System Client
// ============================================================================

/**
 * Show a file in the system file manager (Finder/Explorer)
 */
export async function showInFolder(path: string): Promise<void> {
  await invoke('show_in_folder', { path });
}

/**
 * Open a URL in the default browser
 */
export async function openExternal(url: string): Promise<void> {
  await invoke('open_external', { url });
}

/**
 * Get the current app version
 */
export async function getAppVersion(): Promise<string> {
  return invoke<string>('get_app_version');
}

/**
 * Get platform information (OS and architecture)
 */
export async function getPlatformInfo(): Promise<PlatformInfo> {
  return invoke<PlatformInfo>('get_platform_info');
}
