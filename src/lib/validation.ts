import type { EntryDraft } from './models';

/** Space copies the selected row in the finder, so a name with a space can't be typed there. */
export const NAME_SPACES_MESSAGE =
  "Names can't contain spaces: Space copies the selected result in the finder. Use a dash instead, e.g. wireguard-off.";

export function nameHasSpaces(name: string): boolean {
  return /\s/.test(name.trim());
}

export function validateDraft(d: EntryDraft): string | null {
  if (d.name.trim().length === 0) return 'Name is required.';
  if (nameHasSpaces(d.name)) return NAME_SPACES_MESSAGE;
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
