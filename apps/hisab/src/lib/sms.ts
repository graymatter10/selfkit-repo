// Android SMS import: pull new messages (inbox since the last scan, plus
// ones the SMS receiver queued while the app was closed) into the Rust
// review queue. Runs only when the user turned it on in the SMS screen.
import { api } from "./api";
import { readSms, setSmsEnabled, smsAllowed, takeQueued } from "./native";
import { app, refresh, setPrefs } from "./state.svelte";

export async function syncSms(full = false) {
  if (!app.prefs.sms || !smsAllowed()) return 0;
  setSmsEnabled(true);
  const since = full ? Date.now() - 30 * 86400000 : (app.prefs.sms_since ?? Date.now() - 30 * 86400000);
  const msgs = [...readSms(since), ...takeQueued()];
  const n = msgs.length ? await api.smsIngest(msgs) : 0;
  await setPrefs({ sms_since: Date.now() - 60_000 });
  if (n) await refresh();
  return n;
}
