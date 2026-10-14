import type { Settings } from './models';
import { api } from './api';

class UiState {
  theme = $state<'system' | 'light' | 'dark'>('system');
}

export const ui = new UiState();

export function applyTheme(theme: 'system' | 'light' | 'dark'): void {
  ui.theme = theme;
  if (typeof document === 'undefined') return;
  if (theme === 'system') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', theme);
}

export async function loadTheme(): Promise<Settings> {
  const settings = await api.settings();
  applyTheme(settings.theme);
  return settings;
}
