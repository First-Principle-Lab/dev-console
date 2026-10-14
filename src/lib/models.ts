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

export interface SearchResult extends EntryMeta {
  completion?: string;
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
}

export interface ClipboardExclusion {
  supported: boolean;
  mechanism: string;
  detail: string;
}

export interface Settings {
  theme: 'system' | 'light' | 'dark';
  autostart: boolean;
  clipboardExpirySeconds: number;
  terminal: string[];
}

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
  type: '',
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
