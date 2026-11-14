# Hisab · SMS import (Android)

- Off by default. More → SMS imports explains what happens and asks for READ_SMS + RECEIVE_SMS only when you tap "Allow SMS access".
- When on: the last 30 days of the inbox are scanned once, then every time the app opens/returns it reads newer inbox messages plus messages the `SmsReceiver` queued while the app was closed.
- `sms::parse` keeps only money messages: an amount with a currency marker (Tk, ৳, BDT, $, USD, Rs, INR, PKR, EUR, GBP, …) that isn't a balance/fee/limit, plus a direction word (debited/spent/paid/payment/sent/cash out… → expense; credited/received/cash in/deposited/refund → income). Merchant = text after "at / to / from / Sender:". OTP and chat messages are ignored. Duplicates are skipped.
- Parsed messages wait in the review list (`sms` records, status `pending`). Nothing is saved until you pick a category and tap Save (source `sms`); "Not a transaction" dismisses. Both undoable.
- The account is guessed from the sender name (e.g. "bKash"); change it before saving.
- Tested with `adb emu sms send` on the emulator.
- Code: `src-tauri/src/sms.rs`, `src/lib/sms.ts`, `components/SmsReview.svelte`, `MainActivity.kt`, `SmsReceiver.kt`.
