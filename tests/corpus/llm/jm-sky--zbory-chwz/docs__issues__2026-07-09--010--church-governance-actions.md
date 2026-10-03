# Church governance — people, services, invite

**Status:** `done`  
**Created:** 2026-07-09  
**Plan:** [2026-07-09--church-platform-implementation.md](../plans/2026-07-09--church-platform-implementation.md) (Phase 5)  
**Plan zadań:** [2026-07-27--governance-ui-tasks.md](../plans/2026-07-27--governance-ui-tasks.md) — **G0–G13, źródło prawdy dla realizacji**  
**Spec:** [2026-07-09--church-people-and-services.md](../plans/2026-07-09--church-people-and-services.md)  
**Depends on:** [#006](./2026-07-09--006--org-hierarchy-data-model.md), [#007](./2026-07-09--007--acl-roles-permissions.md)

## Scope

- [x] People form: imię, nazwisko, email, telefon (optional) + search existing person
- [x] Służba select + „Inna” + opis
- [x] Checkbox „Utwórz konto” + permission picker (suggested from service, editable) — picker
      wyjątków `user_permissions` dowieziony w G9–G10 (`UserPermissionsPanel.vue`)
- [x] Pastor: checkbox pre-checked, inactive account, invite action — dowiezione w G1–G3
- [x] Create church, move region
- [x] Remove assignment (cascade ACL via `source_assignment_id`)

## UI wireframe (logical)

```
[+ Dodaj osobę]  [🔍 Wybierz istniejącą]

Imię [    ]  Nazwisko [    ]  Email [    ]  Tel [    ]
Służba [ Diakon ▼ ]  lub Inna: [____________]
Opis   [ Skarbnik / odpowiedzialny za finanse... ]

☐ Utwórz konto użytkownika
   Uprawnienia: [podpowiedź: Diacon] [edytuj...]
```

## Acceptance criteria

- Diakon ze służbą ale bez konta — widoczny na profilu, brak logowania
- Diakon + konto + custom permissions ≠ domyślne Diacon
- Ta sama osoba dodana do drugiego zboru bez duplikacji `persons`

## Decisions (2026-07-25)

- **Invite flow:** `POST /churches/{church_id}/service-assignments/{assignment_id}/invite`, za tym
  samym uprawnieniem co utworzenie przypisania (patrz
  [acl-architecture.md §5.2](../plans/2026-07-25--acl-architecture.md)). Wysyła jednorazowy token
  ustawienia hasła na adres osoby; aktywacja konta (`is_active = true`) następuje po ustawieniu
  hasła. **ACL nadaje się przy tworzeniu przypisania, nie przy aktywacji** — zgodnie z decyzją
  z 2026-07-09 („Pastor ACL before `is_active`”), więc invite nie dotyka uprawnień.
- **Endpoint jest idempotentny w sensie „ponów zaproszenie”** — kolejne wywołanie unieważnia
  poprzedni token i wysyła nowy.

## Stan (2026-07-27) — odblokowane

**Blokada z #007 odpadła.** Silnik ACL (T1–T12) wszedł w `a69366c`, `9f970d7`, `aafb77d`:
`PermissionService`, `user_permissions` (migracja 078), cache Redis, enforcement na zapisach
zborowych, governance API (`POST /churches`, `PATCH .../region`, `PATCH .../visibility`),
`GET /churches/me/permissions` i `usePermissions().can()`. Reguły nadawania (podzbiór, bramka
na role ponad-zborowe) egzekwuje `acl_grant_rules.py`, więc picker ma czego pilnować.

Rozbicie na zadania: **[2026-07-27--governance-ui-tasks.md](../plans/2026-07-27--governance-ui-tasks.md)**.

Zrobione od 2026-07-25: wyszukiwarka istniejącej osoby (P-7) przez
`src/shared/composables/usePersonAutocomplete.ts`; kasowanie ACL po `source_assignment_id`
(`repositories.py:594`); tworzenie zboru i przeniesienie między rejonami (T9).

Brak — treść tego issue: invite flow (G1–G3), ekran nadawania ról dla biskupów (G4–G7),
picker wyjątków `user_permissions` (G9–G10), audit log zmian ACL (G8, G11).

### Sprostowanie do decyzji z 2026-07-25

Powyżej zapisano, że wyjątki `allow`/`deny` są „dostępne wyłącznie dla admina (na start przez CLI)".
W kodzie **nie ma ani CLI, ani API** do `user_permissions` — tabela jest wyłącznie czytana przez
`PermissionService` (`permission_service.py:179`) i kasowana kaskadowo. Jedyną dzisiejszą drogą
zapisu jest ręczny `INSERT`. G9 buduje tę ścieżkę od zera.

### Warunek wstępny

Plan zadań otwiera faza **G0** — cztery błędy wykryte w kodzie po T1–T12, bez których UI governance
nie może działać: brak inwalidacji cache'a przy zmianie grantów (`PermissionCache.invalidate_user`
ma zero wywołań), zasięg `branch` gubiony w `scope_chain`, `can()` nierozwijające łańcucha zasięgów
(biskup nie zobaczy żadnego zboru), oraz UI oferujące role ponad-zborowe szerzej, niż dopuszcza
`assert_can_grant_role`.

## Stan (2026-07-27) — zakończone

Wszystkie zadania z [governance-ui-tasks.md](../plans/2026-07-27--governance-ui-tasks.md) (G0–G13)
dowiezione:

- **G0** — cztery blokujące błędy naprawione: inwalidacja cache'a wpięta we wszystkie ścieżki
  zapisu grantów; `scope_chain("branch", …)` dokłada zasięg `branch`; `/churches/me/permissions`
  zwraca efektywne uprawnienia per zbór; `GET /churches/grantable-roles` zgodny z regułami
  nadawania.
- **G1–G3** — invite flow: token jednorazowy na `users` (migracja 081), `POST
  /churches/{church_id}/service-assignments/{assignment_id}/invite`, `POST
  /auth/accept-invite`, e-mail z szablonem, status konta i przycisk zaproszenia w UI listy osób.
- **G4–G7** — nowy moduł `backend/app/modules/governance/`: katalog ról (`GET /churches/roles`),
  CRUD nadań ról (`GET/POST /governance/role-assignments`, `DELETE .../{id}`), ekran
  `GovernanceRolesPage.vue` (wybór zasięgu, tabela nadań, dialog nadawania), `permissionGuard`
  chroniący trasę `/governance/roles` (`requiresPermission: 'services.manage'`).
- **G8, G11** — append-only `acl_audit_log` (migracja 082, dane osobowe szyfrowane
  `EncryptedString`), `GET /governance/audit-log` (grupowanie po `batch_id`, gate
  `services.manage` w zasięgu, admin/owner widzi wszystko), `AclAuditSection.vue` osadzona na
  ekranie ról.
- **G9–G10** — CRUD wyjątków `user_permissions` (`GET/PUT/DELETE /governance/user-permissions`),
  zasada podzbioru przeniesiona z ról na pojedyncze uprawnienia (patrz sprostowanie niżej i
  [acl-architecture.md §6](../plans/2026-07-25--acl-architecture.md)), panel
  `UserPermissionsPanel.vue` (stan Dziedziczone/Zezwól/Odmów, ostrzeżenie przy zasięgu
  szerszym niż zbór).
- **G12** — macierz uprawnień rozszerzona o nadanie/odebranie roli i wyjątki `allow`/`deny` dla
  każdego typu aktora; test spójności cache'a z działającym Redisem (grant → natychmiastowy
  dostęp → odebranie → natychmiastowa odmowa); test zgodności `grantable-roles` z tym, co
  faktycznie przechodzi przez `POST /governance/role-assignments`.

**Świadomie odłożone poza MVP:** zarządzanie wieloma wspólnotami jako osobny produkt
(„multi-community admin"). Admin/owner na ekranie ról widzi już zasięgi
`community` / `region` / `church` (wspólnota jak biskup; rejony i zbory oznaczone w UI
jako rozszerzony dostęp administratora). Osobny produkt multi-community (przełączanie
między wspólnotami) nadal poza zakresem.
