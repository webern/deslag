# Issue 033 — TanStack Query — odświeżanie listy po edycji

**Data:** 2026-07-10  
**Status:** `done` (2026-07-10)  
**Commit:** `8caaa6f`  
**Component:** `EditCongregationPage.vue`, `ChurchBranchesSection.vue`, `AdminCongregationsPage.vue`  
**Z tego samego promptu:** [#028](./2026-07-10--028--congregation-create-from-list.md), [#032](./2026-07-10--032--congregation-delete-from-list.md), [#034](./2026-07-10--034--export-in-three-dot-menu.md)

## Prompt (Claude Code)

> 2. Check and fix refreshing after editing, Tanstack/Query

*(sesja `52a186bf`)*

## Decyzja

Mutacje (edycja zboru, placówki, panel admina) muszą **`invalidateQueries`** dla klucza listy zborów — inaczej użytkownik widzi stare dane do ręcznego reloadu.

## Implementacja

- `8caaa6f` — `invalidateQueries` po zapisie w:
  - `EditCongregationPage.vue`
  - `ChurchBranchesSection.vue`
  - `AdminCongregationsPage.vue`

## Weryfikacja

- Zmiana nazwy zboru → powrót na listę pokazuje nową nazwę
- Dodanie/usunięcie placówki odświeża listę
