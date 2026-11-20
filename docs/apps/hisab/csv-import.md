# Hisab · CSV import

Next to CSV export: **More → Import CSV** or **Settings → Data → Import CSV**. Added 2026-11-15 (replaces the removed SMS import).

## Flow
1. **Choose a CSV file**: `<input type=file accept=.csv,text/csv,…>` — the Android system file picker, the desktop file dialog. Read as text in the webview (≤ 20 MB) and sent to Rust.
2. **Columns**: Rust guesses the mapping (`csv_preview` with `mapping: null`).
   - **Hisab's own export** (`date,time,type,amount,currency,amount_main,main_currency,account,to_account,to_amount,category,tags,note,…`) is recognised by its header and read as is: type, currency, account and category by name, tags, transfers (`to_account`, `to_amount`). No mapping step.
   - **Any other CSV** (bank statements): date, amount *or* money-out/money-in columns, description, optional category, account, currency, time. Each can be changed in the sheet; date format too (detected); "Positive amounts are spending" (guessed on when a single amount column has no negative numbers).
   - Delimiter `, ; TAB |` detected; quoted fields, `""`, line breaks in quotes, BOM; amounts like `1,234.50`, `1.234,50`, `12,5`, `(12.00)`, `12.00 DR` (DR/CR only as their own word, so `150000 IDR` stays positive — C25), `৳ -850`; dates like `2026-11-15`, `15/11/2026` (day-first unless a value proves month-first), `15 Nov 2026`, with an optional time after it.
3. **Into account**: rows without an account column (or naming an unknown account) go to the chosen account (default: the add screen's default account).
4. **Preview**: counts "N to import · N already in Hisab (skipped) · N with problems (skipped)" and the first 60 rows (date, description, category · account, amount), errors with their line number ("Line 5: Date “31/02/2026” not understood").
   - **Dates**: one format is detected for the column; a row in another format is still read when it can't be misread (month names like `16 Nov 2026`, or year first); row errors wrap instead of being cut (P36).
   - **Splits**: a Hisab export writes a split as `Food 60.00 + Bills 40.00` in the category column; import reads the parts back into `splits` when every name is a category and the parts add up to the amount (L2-15).
   - **Duplicates**: same date + amount (cents) + note (trimmed, case-insensitive) + kind + account as an entry already in Hisab, each existing entry matching one row. Equal rows inside one file (two ATM withdrawals of 1,000 on one day) are all imported (C22); importing the file again marks them all as duplicates.
   - Every row is checked with the same `model::normalize` as a hand-made entry.
   - Unknown categories go to your "Other" category of that kind (the file's name is shown in quotes).
   - **Currency**: the file's currency column if it holds a 3-letter code, else the **main currency**.
5. **Import N entries** → toast "Imported N entries · Undo" (15 s). Undo removes the whole import at once (`csv_import_undo` with all new ids).

## Data
New `transactions` get `source: "import"`, `import_batch: "import-<UTC ms>"` and `created_at`. Nothing else is stored; the file isn't kept.

## Code
- `src-tauri/src/csvimport.rs`: `parse`, `detect_delimiter`, `records`, `guess` (`Mapping`), `parse_date` / `detect_date_format`, `parse_amount`, `preview` (`Row`, `Preview`, duplicates, validation), `import`.
- `src-tauri/src/lib.rs`: commands `csv_preview(text, mapping?, account)`, `csv_import(rows)`, `csv_import_undo(ids)`.
- `src/components/ImportSheet.svelte`, types in `src/lib/api.ts` (`CsvMapping`, `CsvRow`, `CsvPreview`).
- Tests: `csvimport::tests` (quoting/delimiters, amounts/dates, Hisab export round trip with duplicates and undo, bank CSV with debit/credit + decimal commas + errors, positive-only amounts).

## Status
Unit-tested on CI. Not yet tried on a phone (system file picker) — needs a phone re-check.
