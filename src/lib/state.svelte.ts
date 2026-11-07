import type { Theme } from './models';
import { api } from './api';

class UiState {
  theme = $state<Theme>('system');
}

export const ui = new UiState();

export function applyTheme(theme: Theme): void {
  ui.theme = theme;
  if (typeof document === 'undefined') return;
  if (theme === 'system') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', theme);
}

let themeRequest = 0;
export async function loadTheme(): Promise<void> {
  const request = ++themeRequest;
  const theme = await api.theme();
  if (request === themeRequest) applyTheme(theme);
}

export function acceptSavedTheme(theme: Theme): void {
  // A settings event supersedes an older initial-load request still in flight.
  themeRequest += 1;
  applyTheme(theme);
}
