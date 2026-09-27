# people/15_employees-event-decoupling

**Status:** Phase 1 complete (2026-05-16). Verified by plug-out experiment. {claud/opus-4.7}
**Last Updated:** 2026-08-05
**Owners:** claud/opus-4.7
**Sources:**
- `docs/plans/people/12_attendance-event-decoupling.md` — canonical pattern.
- `docs/plans/people/13_leave-event-decoupling.md` / `14_claim-event-decoupling.md` — sibling plans.
- `app/Modules/People/Employees/Services/EmployeePayrollReadinessService.php` — directly imports `PayrollEmployeeStatutoryProfile`. Found during the plug-out experiment after plans 12–14 landed.
- `app/Modules/Core/Employee/Models/Employee.php:154` — Core/Employee defines a `statutoryProfiles()` HasMany pointing at the Payroll model. Out of scope for plan 15 (Core decoupling is its own plan); see Notes.

**Agents:** claud/opus-4.7

---

## Problem

Plans 12–14 cleaned Attendance, Leave, and Claim. The plug-out experiment after plan 14 (move Payroll's ServiceProvider aside; run People test suite) exposed a remaining coupling: the **Employees** sub-module's readiness service imports `PayrollEmployeeStatutoryProfile`. With Payroll's classes removed from disk, the service fails to autoload; any page that hits the workbench crashes.

Distinct from Attendance/Leave/Claim because Employees is not a payroll **producer** — it's a *reader* of payroll-side data. The fix shape is not "dispatch events" but "query through a boundary-safe path."

## Desired Outcome

1. No file under `app/Domains/People/Employees/` imports anything from `App\Domains\People\Payroll\`.
2. The readiness summary still works when Payroll is installed: same blocker codes, same UI behaviour.
3. When Payroll is uninstalled (table absent), the readiness service degrades gracefully: every employee shows as "blocked — missing statutory profile" rather than crashing.
4. The architectural test (`EmployeesDoesNotImportPayrollTest`) gates the boundary on every CI run.

## Out of Scope (deferred)

- **Core/Employee's `statutoryProfiles()` relation** (line 154 of `Employee.php`). This puts a Payroll-model reference in Core. A bigger fix; warrants its own plan (call it 16). For plan 15, no code path in Employees calls the relation — the import is latent. Remove it as part of a Core-decoupling plan, not here.
- **Moving the readiness UI to a Payroll-side panel.** The architectural ideal is that Payroll registers a workbench-extension slot and renders its own readiness card; Employees workbench composes whatever's registered. That's the right long-term shape but multiplicatively more work than the in-place decoupling. Defer until plugin extraction.

## Approach

Replace the Eloquent-class import with a `DB::table('people_payroll_employee_statutory_profiles')` query wrapped in `Schema::hasTable()`. Same pattern previously used by `InteractsWithAttendanceScreen::payrollPayItems()` for the now-superseded pay-item dropdown.

When Payroll is installed: the table exists; the service queries it and returns the same shape it does today.

When Payroll is uninstalled: the table doesn't exist; the service returns null for the latest statutory profile and the readiness summary reports "missing_statutory_profile" for all employees. That is the correct semantics — there is no payroll, so no payroll readiness.

## Phases

### Phase 1 — Decouple the readiness service

- [x] Rewrite `EmployeePayrollReadinessService::latestStatutoryProfile` to use `DB::table` + `Schema::hasTable`, returning a generic `stdClass` or null. {claud/opus-4.7}
- [x] Rewrite `applyStateFilter` and `applyBlockerFilter` to use `whereExists` subqueries; both wrapped by a `statutoryProfileTableExists()` guard. {claud/opus-4.7}
- [x] Remove `use App\Modules\People\Payroll\Models\PayrollEmployeeStatutoryProfile;` from the service. {claud/opus-4.7}
- [x] Remove `'statutoryProfiles'` from `Show.php` and `EmployeeWorkbenchQuery.php` eager-load lists. {claud/opus-4.7}
- [x] Add `EmployeesDoesNotImportPayrollTest`. {claud/opus-4.7}
- [x] Run full People test suite (186 passed). {claud/opus-4.7}

**Plug-out experiment (after this phase):**

Renamed `app/Modules/People/Payroll/ServiceProvider.php` → `.disabled` and ran the source-module suites (Attendance, Leave, Claim, Settings, Employees): **143 passed, 3 failed**. All 3 failures are the same cross-module integration assertions that fail in the prior experiment (each asserts a `PayrollInput` row was written by Payroll's listener). Employees suite now passes cleanly with Payroll's classes still autoloadable but its provider disabled.

**Phase 1 implementation notes:**

- The readiness service's degradation mode when the statutory table is missing: every employee reports as blocked with the `missing_statutory_profile` code. Semantically correct — there is no payroll, so no payroll readiness.
- The `validation_messages` field on statutory profile rows is JSON-encoded text in the raw DB row. The service decodes it itself rather than relying on the Eloquent model's `array` cast.
- The Core/Employee `statutoryProfiles()` relation (line 154 of `Employee.php`) is left untouched. After this plan, no Employees code path calls it — the import is latent. Removing the relation belongs in a Core-decoupling plan.

**Exit criterion:**
- [x] Employees imports nothing from `App\Domains\People\Payroll\`.
- [x] Plug-out experiment with Payroll's ServiceProvider disabled: zero source-module crashes; only the expected cross-module integration assertions fail.

## Notes

- **Core/Employee finding** — `app/Modules/Core/Employee/Models/Employee.php` line 154 keeps `public function statutoryProfiles(): HasMany` returning `PayrollEmployeeStatutoryProfile::class`. The `use` import at line 13 is referenced only inside this method body and inside `::class` resolution. PHP defers class resolution until method invocation, so the import is harmless as long as no code path calls the relation. After plan 15, no Employees code calls it. The remaining latent import is the subject of a separate Core-decoupling plan, not Plan 15.
- **The architectural test approach for Employees mirrors plans 12–14.** Same Finder-based scan; same prohibition string.
