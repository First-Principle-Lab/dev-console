export type Kind = 'command' | 'secret';
export type FieldDisplay = 'text' | 'multiline' | 'url';

export interface ExtraField {
  id: string;
  label: string;
  value: string;
  display: FieldDisplay;
}

export interface EntryBase {
  id: string;
  revision: number;
  name: string;
  type: string;
  tags: string[];
  description: string;
  notes: string;
  fields: ExtraField[];
  pinned: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface CommandEntry extends EntryBase {
  kind: 'command';
  command: string;
  workingDirectory: string;
}

export interface SecretEntry extends EntryBase {
  kind: 'secret';
  value: string;
}

export type Entry = CommandEntry | SecretEntry;

/** Metadata that is safe to return for a secret while the vault is unlocked. Never carries `value`. */
export interface EntryMeta {
  id: string;
  kind: Kind;
  revision: number;
  name: string;
  type: string;
  tags: string[];
  description: string;
  pinned: boolean;
  updatedAt: string;
  usageCount: number;
  lastUsedAt: string | null;
}

/** Exactly what `search_entries` returns (models.rs SearchResult): no revision or updatedAt. */
export interface SearchResult {
  id: string;
  kind: Kind;
  name: string;
  type: string;
  tags: string[];
  description: string;
  completion?: string | null;
  pinned: boolean;
  usageCount: number;
  lastUsedAt: string | null;
}

export interface SearchResponse {
  requestId: number;
  vaultGeneration: number;
  results: SearchResult[];
}

/** Draft as edited in the UI. `expectedRevision` is 0 for new entries. */
export interface EntryDraft {
  kind: Kind;
  id: string | null;
  expectedRevision: number;
  name: string;
  type: string;
  tags: string[];
  description: string;
  notes: string;
  fields: ExtraField[];
  pinned: boolean;
  command: string;
  workingDirectory: string;
  /** Secret value; only ever populated for an explicit reveal/edit action. */
  value: string;
}

export interface EntryDetails {
  meta: EntryMeta;
  command?: string;
  workingDirectory?: string;
  notes: string;
  fields: ExtraField[];
  /** Present only for an explicit secret reveal while unlocked. */
  value?: string;
  executionApproved: boolean;
}

export interface RunOutput {
  /** null when the command was killed (timeout) or ended by a signal. */
  exitCode: number | null;
  stdout: string;
  stderr: string;
  truncated: boolean;
  timedOut: boolean;
  durationMs: number;
}

export interface VaultState {
  unlocked: boolean;
  manualLock: boolean;
  keyringAvailable: boolean;
  keyringUnlocked: boolean;
  vaultAvailable: boolean;
  generation: number;
  authenticationRequired: boolean;
  reason?: string;
}

export interface AppStateInfo {
  vault: VaultState;
  version: string;
  authenticatedUnlockAvailable: boolean;
  clipboardExclusion: ClipboardExclusion;
  backupDir: string;
}

export interface ArchiveSummary {
  source: string;
  createdAt: string;
  commands: number;
  secrets: number;
  sizeBytes: number;
}

export interface RecoveryKeyInfo {
  secret: string;
  path: string;
}

export interface ClipboardExclusion {
  supported: boolean;
  mechanism: string;
  detail: string;
}

export interface Settings {
  theme: Theme;
  autostart: boolean;
  clipboardExpirySeconds: number;
  terminal: string[];
  /** Offer frequently used commands from the shell's own history as new entries. */
  historySuggestionsEnabled: boolean;
}

/**
 * One command from the shell's history worth offering as a new entry. `key` is a SHA-256 digest
 * of the normalised command - it is what a dismissal refers to, so dismissing never writes the
 * command itself anywhere.
 */
export interface HistorySuggestion {
  key: string;
  command: string;
  count: number;
  suggestedName: string;
}

export interface TerminalOption {
  id: string;
  name: string;
  command: string[];
}

export interface RunResult {
  mode: 'inline' | 'terminal';
  stdout: string;
  stderr: string;
  exitCode: number | null;
  truncated: boolean;
  timedOut: boolean;
}

export const OMARCHY_THEMES = [
  ['catppuccin', 'Catppuccin'], ['catppuccin-latte', 'Catppuccin Latte'],
  ['ethereal', 'Ethereal'], ['everforest', 'Everforest'],
  ['flexoki-light', 'Flexoki Light'], ['gruvbox', 'Gruvbox'],
  ['hackerman', 'Hackerman'], ['kanagawa', 'Kanagawa'],
  ['last-horizon', 'Last Horizon'], ['lumon', 'Lumon'],
  ['lupine', 'Lupine'], ['matte-black', 'Matte Black'],
  ['miasma', 'Miasma'], ['nord', 'Nord'],
  ['osaka-jade', 'Osaka Jade'], ['retro-82', 'Retro 82'],
  ['ristretto', 'Ristretto'], ['rose-pine', 'Rose Pine'],
  ['solitude', 'Solitude'], ['tokyo-night', 'Tokyo Night'],
  ['vantablack', 'Vantablack'], ['white', 'White']
] as const;

export type Theme = 'system' | 'light' | 'dark' | `omarchy-${typeof OMARCHY_THEMES[number][0]}`;

export interface SavedEntry {
  id: string;
  revision: number;
  kind: Kind;
}

export interface CopyResult {
  kind: Kind;
  id: string;
  expiresAtMs: number;
  excluded: boolean;
  exclusion: ClipboardExclusion;
}

export interface WindowIntent {
  action: 'new-entry' | 'open-entry';
  kind: Kind;
  entryId?: string;
}

export interface EntriesChanged {
  kind: Kind;
  id: string;
  revision: number;
  operation: 'created' | 'updated' | 'deleted';
  vaultGeneration: number;
}

export const EMPTY_DRAFT = (kind: Kind): EntryDraft => ({
  kind,
  id: null,
  expectedRevision: 0,
  name: '',
  type: kind === 'command' ? 'Command' : 'Secret',
  tags: [],
  description: '',
  notes: '',
  fields: [],
  pinned: false,
  command: '',
  workingDirectory: '',
  value: ''
});

export function draftFromDetails(details: EntryDetails, kind: Kind): EntryDraft {
  const entry = details;
  return {
    kind,
    id: entry.meta.id,
    expectedRevision: entry.meta.revision,
    name: entry.meta.name,
    type: entry.meta.type,
    tags: [...entry.meta.tags],
    description: entry.meta.description,
    notes: entry.notes,
    fields: entry.fields.map((f) => ({ ...f })),
    pinned: entry.meta.pinned,
    command: entry.command ?? '',
    workingDirectory: entry.workingDirectory ?? '',
    value: entry.value ?? ''
  };
}

export interface BackupInfo {
  name: string;
  size: number;
  modifiedAtMs: number;
}

export interface BackupOutcome {
  source: string;
  commands: number;
  secrets: number;
}
