import type { EntryDraft } from './models';

export function validateDraft(d: EntryDraft): string | null {
  if (d.name.trim().length === 0) return 'Name is required.';
  if (d.type.trim().length === 0) return 'Type is required.';
  if (d.kind === 'command' && d.command.trim().length === 0) return 'Command text is required.';
  if (d.kind === 'secret' && d.value.length === 0) return 'Secret value is required.';
  for (const field of d.fields) {
    if (field.label.trim().length === 0) return 'Every extra field needs a label.';
    if (
      field.display === 'url' &&
      field.value.trim().length > 0 &&
      !/^https?:\/\/[^\s]+$/i.test(field.value.trim())
    ) {
      return `Field "${field.label}" must be an http(s) URL.`;
    }
  }
  return null;
}

export function isHttpUrl(value: string): boolean {
  return /^https?:\/\/[^\s]+$/i.test(value.trim());
}
