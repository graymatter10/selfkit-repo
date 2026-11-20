# Hisab · loans

- Lent or borrowed; person, title, description, amount + currency, date given, optional due date, optional linked account, proof photos.
- Repayments are added one by one (amount, date, note; "All" fills the rest); each can be removed (undo). Remaining = amount − repayments; repayments can't exceed the loan.
- Status (computed): **finished** (nothing left), **overdue** (past due, red), **due soon** (≤ 7 days), **ongoing**.
- A linked account changes its balance: lent takes the amount out on the date given, repayments put it back (borrowed the other way round). Net worth counts money lent as an asset and money borrowed as a debt.
- Reminder: `remind_days` before the due date (on the day, 1, 2, 3, 5, 7, 14) at `remind_time`; only while unpaid and while loan reminders are on. See README § Reminders.
- Code: `components/Loans.svelte`, `LoanSheet.svelte`; `stats::loans_view`, `lib.rs` `reminders`.
- **New loan** starts on the tab it was opened from: the Borrowed tab preselects "I borrowed" (L2-28).
