# Hisab · currencies

- Main currency = shared Selfkit setting `main_currency` (default USD), changeable in Settings. Totals, budgets and charts are in the main currency.
- Every account, transaction, loan and goal has its own currency. Transfers between accounts in different currencies store the arriving amount (`to_amount`); if not typed it is converted.
- Rates come from open.er-api.com (free, no key) when online and older than 12 h, or with "Update now"; they are cached in the synced `rates` record, so everything works offline. Typing a rate in Settings ("1 USD = 120 BDT") stores a hand override (`manual`) that wins over fetched rates; clearing it goes back to the fetched one.
- A currency without any rate counts 1:1 and Insights shows a warning naming it.
- Code: `src-tauri/src/fx.rs`, `src/lib/rates.ts`, `components/Settings.svelte`, `CurrencySheet.svelte`.
