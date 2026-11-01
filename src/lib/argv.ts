/**
 * POSIX-style word splitting for the terminal command setting.
 *
 * The old behaviour was `value.split(' ')`, which broke any launcher whose
 * program or arguments contain spaces (`/usr/bin/my terminal -e`) and silently
 * mangled quoted arguments (`foot --title "Dev Console" -e` became four
 * nonsense argv entries). This parser handles single quotes, double quotes and
 * backslash escapes; unbalanced quoting is an error the UI can show instead of
 * a silently wrong argv.
 */
export function parseArgv(input: string): string[] {
  const words: string[] = [];
  let current = '';
  let started = false;
  let index = 0;
  while (index < input.length) {
    const char = input[index];
    if (char === ' ' || char === '\t' || char === '\n') {
      if (started) {
        words.push(current);
        current = '';
        started = false;
      }
      index += 1;
      continue;
    }
    if (char === "'") {
      started = true;
      index += 1;
      while (index < input.length && input[index] !== "'") {
        current += input[index];
        index += 1;
      }
      if (index >= input.length) {
        throw new Error('unbalanced single quote');
      }
      index += 1;
      continue;
    }
    if (char === '"') {
      started = true;
      index += 1;
      while (index < input.length && input[index] !== '"') {
        if (input[index] === '\\' && index + 1 < input.length) {
          index += 1;
        }
        current += input[index];
        index += 1;
      }
      if (index >= input.length) {
        throw new Error('unbalanced double quote');
      }
      index += 1;
      continue;
    }
    if (char === '\\' && index + 1 < input.length) {
      started = true;
      index += 1;
      current += input[index];
      index += 1;
      continue;
    }
    started = true;
    current += char;
    index += 1;
  }
  if (started) {
    words.push(current);
  }
  return words;
}

/** True when the string can be shown in a single input field without ambiguity. */
export function needsQuoting(word: string): boolean {
  return word.length === 0 || /[\s'"]/.test(word);
}

/** Render argv back into an input field: empty words become `""`, spaced words get quoted. */
export function formatArgv(argv: string[]): string {
  return argv.map((word) => (needsQuoting(word) ? `"${word.replace(/"/g, '\\"')}"` : word)).join(' ');
}
