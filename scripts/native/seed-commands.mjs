// Seeds four synthetic COMMANDS (never secrets) into a Dev Console SQLite database so the
// native capture session shows a populated palette. Secret metadata lives in the vault, which
// this session never unlocks; these rows are the same shape and synthetic names the e2e fixtures
// use, and command text is non-secret metadata that e2e captures already show.
//
//   bun scripts/native/seed-commands.mjs <path/to/commands.sqlite3>
import { Database } from 'bun:sqlite';

const target = process.argv[2];
if (!target) {
  console.error('usage: seed-commands.mjs <commands.sqlite3>');
  process.exit(2);
}

const db = new Database(target);
const stamp = '2026-11-02T09:00:00Z';
const rows = [
  ['nc1', 1, 'docker', 'Docker', '[]', '', 0, 'docker ps', 9],
  ['nc2', 1, 'docker compose up', 'Docker', '[]', '', 0, 'docker compose up', 2],
  ['nc3', 1, 'git revert HEAD', 'Git', '[]', '', 0, 'git revert HEAD', 1],
  ['nc4', 1, 'Restart containers', 'Docker', '["work"]', 'restarts everything', 1, 'docker compose restart', 0]
];
for (const [id, revision, name, type, tags, description, pinned, command, usage] of rows) {
  db.run(
    `INSERT OR REPLACE INTO entries
       (id, kind, revision, name, type, tags, description, notes, fields, pinned, command,
        working_directory, created_at, updated_at)
     VALUES (?1,'command',?2,?3,?4,?5,?6,'','[]',?7,?8,'',?9,?9)`,
    [id, revision, name, type, tags, description, pinned, command, stamp]
  );
  db.run('INSERT OR REPLACE INTO usage (id, count, last_used_at) VALUES (?1,?2,?3)', [
    id,
    usage,
    usage > 0 ? stamp : null
  ]);
}
const count = db.query("SELECT count(*) AS n FROM entries WHERE kind = 'command'").get();
db.close();
console.log(`seeded ${count.n} commands into ${target}`);
