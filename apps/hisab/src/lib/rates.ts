// Exchange rates from open.er-api.com (free, no key, ~160 currencies),
// cached in the rates record so totals work offline. Hand-edited rates in
// Settings override fetched ones.
import { api } from "./api";
import { app, refresh } from "./state.svelte";

const URL = "https://open.er-api.com/v6/latest/USD";

export async function fetchRates(): Promise<boolean> {
  try {
    const r = await fetch(URL, { cache: "no-store" });
    if (!r.ok) return false;
    const j = await r.json();
    if (j.result !== "success" || !j.rates) return false;
    await api.ratesSave(j.base_code ?? "USD", j.rates);
    await refresh();
    return true;
  } catch {
    return false;
  }
}

/** Refresh when online and the cache is older than 12 hours. */
export async function maybeFetchRates() {
  const at = app.snap?.rates.fetched_at ?? 0;
  if (Date.now() - at > 12 * 3600_000 && navigator.onLine !== false) await fetchRates();
}
