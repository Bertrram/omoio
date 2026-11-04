/// Calls to the emulator under way or waiting their turn. They go one at a
/// time, in the order asked: each works in the emulator's own window, and
/// two at once would close it under each other. The menu can close and
/// open again while one runs.
let pending = 0;
let line: Promise<unknown> = Promise.resolve();
/// Whether something the user asked for is under way. Putting a trap on can
/// take two changes, and a press in between would pick the same free slot.
export let acting = false;

export function inTurn<T>(call: () => Promise<T>): Promise<T> {
  pending += 1;
  const run = line.then(() => call()).finally(() => {
    pending -= 1;
  });
  line = run.catch(() => undefined);
  return run;
}

/// Does what the user asked for, unless the emulator is busy.
export async function act(what: () => Promise<unknown>) {
  if (acting || pending > 0) return;
  acting = true;
  try {
    await what();
  } finally {
    acting = false;
  }
}
