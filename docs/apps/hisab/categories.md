# Hisab · categories

- Two separate lists: expense and income (`categories.kind`).
- Unlimited. Add from the add screen ("+ New" at the end of the category row; the new one is picked right away) or from More → Categories.
- Each has a name, icon (34 line icons), colour (12) and, for expenses, an optional monthly budget in the main currency. Budget progress shows in the manager, in Insights and (total) on the home summary.
- Ranking (Settings or manager, pref `rank`): **My order** (drag the ⋮⋮ handle, or focus it and press ↑/↓; saved as `order`) or **Most used** (count of all entries, `usage`). The add screen shows them in that order.
- The category row on the add screen scrolls sideways; a fade on the right/left edge shows there is more.
- Delete = archive (`archived: true`, undo toast). Archived categories leave the add screen and the manager's main list, but old entries keep them, so History, Insights and exports stay correct (shown "archived"). Restore from "Archived · n" in the manager.
- Code: `components/Categories.svelte`, `CategorySheet.svelte`, `CategoryRow.svelte`; `stats::categories_view`.
