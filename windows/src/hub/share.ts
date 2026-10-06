// What the island shares with Jinx: plain files in ~/.hermes/state/coucou/ on the server (see share.rs).
// Off when sharing is switched off in Settings or no server is set.

import { Bridge } from "../core/bridge";
import { State } from "../core/state";

const README = `# Coucou (the island on Alan's laptop) shares these files with you

- notes.md       Alan's scratch notes, as he typed them
- calendar.json  his calendar for the next days (title, start, end, all-day)
- pomodoro.json  his focus timer statistics (today, week, streak, best hours)
- tasks.json     his to-do list with reminders (title, done, remindAt in unix seconds)

They are rewritten whenever they change, so they are current. Read-only for you: the island overwrites them.
`;

export function sharing(): boolean {
  return State.settings.jinxShare !== false && !!State.settings.serverHost?.trim();
}

/** Writes the README once per run, so Jinx can find out what the other files are. */
let wroteReadme = false;
export function shareReadmeOnce() {
  if (wroteReadme || !sharing()) return;
  wroteReadme = true;
  void Bridge.sharePush("readme.md", README).catch(() => {
    wroteReadme = false;
  });
}
