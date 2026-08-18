// take_index.js
//
// Stable identity for highlights, so status survives things that rebuild the
// demo objects underneath it (a re-scan, a project save/load round trip).
//
// Highlights have no id of their own — nothing in CaptureStreak is unique and
// durable — so this derives a content-addressed one. Kill frames are used
// rather than start_tick/end_tick because editing a highlight's Kill Range
// moves the ticks but never mutates the kills array, so the uid stays put
// across range edits.

/**
 * Content-addressed id for one highlight, stable across re-scans, project
 * save/load, and Kill Range edits. Cached onto the streak after first use.
 *
 * Falls back to tick bounds when a streak has no kills (only really happens
 * for synthetic/test data) — still stable, just less precise.
 */
export function streakUid(demoPath, streak) {
  if (streak.uid) return streak.uid;

  const kills = streak.kills || [];
  const firstKill = kills.length ? kills[0][0] : streak.start_tick;
  const lastKill = kills.length ? kills[kills.length - 1][0] : streak.end_tick;
  const uid = `${demoPath}#${streak.player_index}#${firstKill}#${lastKill}`;

  // Non-enumerable so it never leaks into the capture payload sent over IPC,
  // where the Rust side would reject an unknown field.
  Object.defineProperty(streak, 'uid', { value: uid, enumerable: false, writable: true });
  return uid;
}

/**
 * Copies the user-owned, non-derivable state from a previously-loaded version
 * of a demo onto a freshly-scanned one, matching highlights by uid.
 *
 * A re-scan re-parses the demo and produces brand new streak objects, so
 * without this every status, selection, and note on that demo is silently
 * destroyed. Only fields the user set are carried over — everything derived
 * from the demo file itself comes from the fresh scan.
 */
export function preserveHighlightState(previousDemo, freshDemo) {
  if (!previousDemo?.streaks?.length || !freshDemo?.streaks?.length) return freshDemo;

  const previousByUid = new Map(
    previousDemo.streaks.map(s => [streakUid(previousDemo.path, s), s])
  );

  freshDemo.streaks.forEach(fresh => {
    const previous = previousByUid.get(streakUid(freshDemo.path, fresh));
    if (!previous) return;
    if (previous.status !== undefined) fresh.status = previous.status;
    if (previous.selected !== undefined) fresh.selected = previous.selected;
    if (previous.notes !== undefined) fresh.notes = previous.notes;
    // Kill Range edits are user edits too, not scan output.
    if (previous.start_index !== undefined) fresh.start_index = previous.start_index;
    if (previous.end_index !== undefined) fresh.end_index = previous.end_index;
  });

  return freshDemo;
}
