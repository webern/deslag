# Language Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a persisted application language setting that defaults to English and can switch the first app chrome slice to Simplified Chinese.

**Architecture:** Reuse the existing `AppSettings` / `settings_get` / `settings_update` / `settings_changed` pipeline. Add a small local typed dictionary under `web/src/i18n`, keep `en` as the stable fallback, and migrate only the FEAT-02 first chrome slice.

**Tech Stack:** Rust + rusqlite KV settings + ts-rs bindings, SolidJS + TypeScript, Vitest/jsdom.

---

## File Structure

- Modify `crates/core/src/app_settings.rs`: add `language` to `AppSettings` and `SettingsUpdateRequest`; validate persisted strings.
- Modify generated binding files after Rust changes: `web/src/bindings/AppSettings.ts`, `web/src/bindings/SettingsUpdateRequest.ts`, and `web/src/bindings/index.ts` if ts-rs changes export order.
- Modify `web/src/stores/settings.ts`: add default `language` during settings-contract GREEN so generated `AppSettings` stays type-complete; later include it in `updateSettings` and call i18n document sync after settings load/update/event.
- Create `web/src/i18n/dictionaries.ts`: local `en` and `zh-Hans` dictionaries.
- Create `web/src/i18n/index.ts`: `Language`, `normalizeLanguage`, `t`, `getDictionary`, `setDocumentLanguage`.
- Create `web/tests/i18n/language.test.ts`: dictionary parity and fallback tests.
- Create `web/tests/stores/settings-language.test.ts`: settings store language update behavior where practical; if direct store mocking is too coupled, cover this through component tests instead.
- Modify `web/src/panels/Settings/AppearanceGroup.tsx`: add Language selector.
- Create `web/tests/panels/Settings/language-selector.test.tsx`: mock Tauri `invoke`, use `reloadSettings()`, render selector, assert `settings_update` payload and `document.lang` sync.
- Migrate first chrome slice to `t()`: `web/src/panels/Settings/*`, `web/src/components/PrimarySidebar.tsx`, `web/src/components/ActivityStrip.tsx`, `web/src/components/TopBar.tsx`, `web/src/components/BottomPanel.tsx`, selected fixed status labels in `web/src/App.tsx`, and selected common chrome/dialog labels where FEAT-02.4 checklist explicitly includes them.
- Update docs after implementation: `docs/tasks/FEAT-02-language-settings.md`, `docs/adr/ADR-025-frontend-i18n-dictionary.md`.

---

## Ready Gate Review Notes

- 2026-06-05 Grok FEAT-02.1 audit correction: `crates/core/src/app_settings.rs` already has a test-local `setup()` helper; do not use a nonexistent `crate::db::create_test_pool()`.
- All FEAT-02.1 Rust test names must share the `app_settings_language_` prefix so the focused RED/GREEN command actually executes the intended tests.
- FEAT-02.1 GREEN owns the immediate generated-contract fallout: regenerate bindings and add `language: "en"` to `web/src/stores/settings.ts` `DEFAULTS` so `AppSettings` remains type-complete before i18n/UI work starts.
- 2026-06-06 Grok FEAT-02.1 follow-up: after the plan fixes above, FEAT-02.1 may enter RED; the whole FEAT-02 remains Draft because FEAT-02.2-02.5 and ADR-025 still need their gates. For FEAT-02.1 binding generation, `cargo check -p vibestation-app` is sufficient because `crates/app/build.rs` already watches `../core/src/app_settings.rs` and exports `AppSettings` / `SettingsUpdateRequest`; do not use `cargo check --workspace` just to refresh bindings.
- FEAT-02.1 GREEN must add `language: "en"` to `web/src/stores/settings.ts` `DEFAULTS` and update the known `ExternalTerminalGroup.test.tsx` fixture if typecheck reports it. Do not add `updateSettings` language request mapping in FEAT-02.1; that wiring belongs to the i18n/selector slices after the typed frontend language helper exists.
- 2026-06-06 Kimi FEAT-02 spec review verdict: APPROVE-WITH-NITS, no Ready-blocking fixes. Nits folded into this plan: explicit AC7 checklist boundary, no read-time DB writeback for invalid persisted language, dictionary key naming, runtime fallback, and a minimal interpolation convention if first-slice inventory proves it is needed.
- 2026-06-06 prior FEAT-02 UI/test audit correction: component tests must mock Tauri `invoke` and use the real settings store with `reloadSettings()`; do not mock `useSettings()` with a static object because that bypasses Solid store responsiveness and cannot prove instant language refresh.
- 2026-06-06 OpenCode app chrome audit verdict: READY-WITH-NITS, read-only inventory. Folded into FEAT-02.4: explicit Settings / sidebar / activity / topbar / status / common dialog checklist, existing hardcoded Chinese copy must be extracted when in scope, common frontend errors stay limited to listed chrome/dialog errors, and long privacy/telemetry prose moves to a follow-up `content.privacy.*` namespace.
- 2026-06-06 Kimi FEAT-02.2 Ready gate verdict: READY-WITH-NITS after test-plan fixes. Required before RED: selected-locale miss must fall back to `en`, nested dot-notation lookup must be tested, settings store document-language side effect must run through centralized helper on init/reload/event/update, and Vitest DOM support must be confirmed before relying on `document.documentElement`.
- 2026-06-06 OpenCode runner diagnostic verdict: DIAGNOSED. The Windows `file:///@solid-refresh` crash reproduces across TSX component tests under both `pnpm --filter @vibestation/web exec vitest ...` and `pnpm --dir web exec vitest ...`; discovery works, pure TS tests do not hit the Solid transform, and the root cause is the bare `solid()` plugin in `web/vitest.config.ts`.
- 2026-06-06 FEAT-02.3 runner gate: Windows component tests fail before test collection when Vitest loads Solid HMR runtime as `file:///@solid-refresh`. FEAT-02.3 RED requires `web/vitest.config.ts` to use `solid({ hot: false })` so failures reflect selector behavior rather than the local runner. Fixed in `84f252d test(web): 禁用 Vitest Solid HMR runtime`; post-fix verification: `pnpm --dir web exec vitest run tests/panels/Settings/ExternalTerminalGroup.test.tsx` passes 5/5, `pnpm --dir web exec vitest run tests/i18n/language.test.ts` passes 10/10, and `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts` passes 10/10.
- 2026-06-06 FEAT-02.3 scope guardrail: only the new AppearanceGroup language selector may use i18n in FEAT-02.3, and only these keys: `settings.appearance.language`, `settings.appearance.english`, `settings.appearance.simplifiedChinese`. Existing Appearance labels such as `Theme` / `Font family`, other Settings groups, sidebar/status/app chrome strings, and `Git Log` remain FEAT-02.4.
- 2026-06-06 FEAT-02.3 replanning: the external UI-test auditor is removed from the active dispatch path for this work. Codex owns FEAT-02.3 RED/GREEN locally; OpenCode is the only external follow-up and is limited to read-only mechanical scope audit after implementation via `spike-tmp/dispatch/FEAT-02-3-selector-scope-opencode-prompt.md`.

### Task 1: Settings Contract RED

**Files:**

- Modify: `crates/core/src/app_settings.rs`

- [x] **Step 1: Add failing Rust tests for language defaults, persistence, and fallback**

Add these tests inside the existing `#[cfg(test)]` module in `crates/core/src/app_settings.rs`. Reuse the existing local `setup()` helper; do not introduce `crate::db::create_test_pool()`.

```rust
#[test]
fn app_settings_language_default_is_en() {
    let settings = AppSettings::default();
    assert_eq!(settings.language, "en");
}

#[test]
fn app_settings_language_get_all_defaults_to_en_when_empty() {
    let (_dir, pool) = setup();
    let settings = AppSettingsStore::get_all(&pool);
    assert_eq!(settings.language, "en");
}

#[test]
fn app_settings_language_persists_across_get_all() {
    let (_dir, pool) = setup();
    let req = SettingsUpdateRequest {
        language: Some("zh-Hans".to_string()),
        ..Default::default()
    };

    AppSettingsStore::update(&pool, &req).expect("language update succeeds");

    let settings = AppSettingsStore::get_all(&pool);
    assert_eq!(settings.language, "zh-Hans");
}

#[test]
fn app_settings_language_persists_across_pool_reopen() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("persist_language.db");

    {
        let pool = db::open_pool(&db_path).unwrap();
        let req = SettingsUpdateRequest {
            language: Some("zh-Hans".to_string()),
            ..Default::default()
        };
        AppSettingsStore::update(&pool, &req).expect("language update succeeds");
    }

    let pool2 = db::open_pool(&db_path).unwrap();
    let settings = AppSettingsStore::get_all(&pool2);
    assert_eq!(settings.language, "zh-Hans");
}

#[test]
fn app_settings_language_invalid_value_falls_back_to_en() {
    let (_dir, pool) = setup();

    for invalid in ["fr", "", "zh"] {
        AppSettingsStore::set(&pool, "language", invalid).expect("seed invalid language");
        let settings = AppSettingsStore::get_all(&pool);
        assert_eq!(settings.language, "en");
    }
}
```

These tests intentionally fail to compile before GREEN because `AppSettings` and `SettingsUpdateRequest` do not yet expose `language`.

- [x] **Step 2: Run RED test and confirm failure**

Run:

```powershell
cargo test -p vibestation-core app_settings_language -- --nocapture
```

Expected: FAIL, and the failure must be caused by the missing `language` contract rather than "0 tests run".

- [x] **Step 3: Commit RED**

```powershell
git add crates/core/src/app_settings.rs
git commit -m "test(settings): 加语言设置 RED 测试

Co-authored-by: Codex CLI <noreply@openai.com>"
```

---

### Task 2: Settings Contract GREEN

**Files:**

- Modify: `crates/core/src/app_settings.rs`
- Modify: `web/src/stores/settings.ts`
- Modify: `web/src/bindings/AppSettings.ts`
- Modify: `web/src/bindings/SettingsUpdateRequest.ts`

- [x] **Step 1: Add language field and validation helper**

In `crates/core/src/app_settings.rs`, add `pub language: String` to `AppSettings`, `pub language: Option<String>` to `SettingsUpdateRequest`, default `"en"`, and these helpers near the settings parsing helpers:

```rust
fn normalize_language(raw: &str) -> &'static str {
    match raw.trim() {
        "en" => "en",
        "zh-Hans" => "zh-Hans",
        _ => "en",
    }
}

fn get_language(pool: &DbPool) -> String {
    let raw = AppSettingsStore::get(pool, "language").unwrap_or_else(|_| "en".to_string());
    normalize_language(&raw).to_string()
}
```

Update `AppSettingsStore::get_all` to use:

```rust
let language = get_language(pool);
```

and include `language` in the returned `AppSettings`.

Update `AppSettingsStore::update`:

```rust
if let Some(ref v) = req.language {
    Self::set(pool, "language", normalize_language(v))?;
}
```

`get_all` must not write corrected values back to `app_settings`; reads stay side-effect free. `update` owns persistence normalization, so invalid update input is stored as `"en"`.

- [x] **Step 2: Run Rust tests**

Run:

```powershell
cargo test -p vibestation-core app_settings_language -- --nocapture
```

Expected: PASS for the five FEAT-02 language tests.

- [x] **Step 3: Regenerate ts-rs bindings**

Run:

```powershell
cargo check -p vibestation-app
```

Expected: PASS and generated bindings include `language`.

`cargo check -p vibestation-app` runs `crates/app/build.rs`, which already has `cargo:rerun-if-changed=../core/src/app_settings.rs` and exports both settings types. Keep `web/src/bindings/index.ts` in the GREEN `git add` path even when its export lines do not change.

- [x] **Step 4: Add frontend default for the new contract field**

In `web/src/stores/settings.ts`, add `language: "en"` to `DEFAULTS`. Do not wire i18n imports, `document.lang`, or `updateSettings` request mapping in FEAT-02.1; those belong to FEAT-02.2/FEAT-02.3 after the frontend `Language` helper exists. The current `updateSettings` function uses `as SettingsUpdateRequest`, so omitting `req.language` in FEAT-02.1 should not by itself fail typecheck.

If `pnpm typecheck` reports existing `AppSettings` test fixtures missing `language`, update those fixtures in the same GREEN slice. Known expected type fallout for FEAT-02.1 is limited to:

- `web/src/stores/settings.ts`
- `web/tests/panels/Settings/ExternalTerminalGroup.test.tsx`

- [x] **Step 5: Verify generated binding shape**

Check:

```powershell
Select-String -LiteralPath "web/src/bindings/AppSettings.ts","web/src/bindings/SettingsUpdateRequest.ts" -Pattern "language"
```

Expected: both files contain `language`.

- [x] **Step 6: Run contract fallout check**

Run:

```powershell
pnpm typecheck
```

Expected: PASS, or only fail for a documented pre-existing issue unrelated to FEAT-02.1.

- [x] **Step 7: Commit GREEN**

```powershell
git add crates/core/src/app_settings.rs web/src/stores/settings.ts web/src/bindings/AppSettings.ts web/src/bindings/SettingsUpdateRequest.ts web/src/bindings/index.ts web/tests/panels/Settings/ExternalTerminalGroup.test.tsx
git commit -m "feat(settings): 持久化应用语言设置

Co-authored-by: Codex CLI <noreply@openai.com>"
```

Execution evidence (2026-06-06):

- RED commit: `8a8c087 test(settings): 增加语言设置契约 RED 测试`
- GREEN commit: `1ffc9fa feat(settings): 持久化应用语言设置`
- Review follow-up commit: `a73d84b test(settings): 补语言设置持久化不变量测试`
- RED command: `cargo test -p vibestation-core app_settings_language -- --nocapture` failed with missing `AppSettings.language` / `SettingsUpdateRequest.language`.
- GREEN/review commands passed: `cargo test -p vibestation-core app_settings -- --nocapture` (20 passed), `cargo check -p vibestation-app`, `pnpm typecheck`, `git diff --check`.
- Extra component test attempt: `pnpm --filter @vibestation/web exec vitest run tests/panels/Settings/ExternalTerminalGroup.test.tsx` failed before running tests on existing Windows Vitest/Solid refresh import `file:///@solid-refresh`; this is recorded as a test-runner diagnostic item, not a FEAT-02.1 contract failure.

---

### Task 3: I18n Core RED

**Files:**

- Create: `web/tests/i18n/language.test.ts`

- [ ] **Step 1: Write failing i18n tests**

```ts
import { describe, expect, it } from "vitest";

const loadI18n = () => import("../../src/i18n");

const collectKeys = (value: unknown, prefix = ""): string[] => {
  // Leaf path rule: FEAT-02 dictionaries are nested objects with string leaves.
  // Functions or rich values are not allowed in this first slice.
  if (typeof value !== "object" || value === null) {
    return [prefix];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(
    ([key, child]) => collectKeys(child, prefix ? `${prefix}.${key}` : key),
  );
};

describe("FEAT-02 i18n core", () => {
  it("TEST-FEAT-02.1: vitest exposes jsdom document", () => {
    expect(document.documentElement).toBeInstanceOf(HTMLHtmlElement);
  });

  it("TEST-FEAT-02.5: en and zh-Hans dictionaries have identical leaf keys", async () => {
    const { dictionaries } = await loadI18n();
    const enKeys = collectKeys(dictionaries.en).sort();
    const zhKeys = collectKeys(dictionaries["zh-Hans"]).sort();
    expect(zhKeys).toEqual(enKeys);
  });

  it("TEST-FEAT-02.4: invalid language falls back to en", async () => {
    const { normalizeLanguage } = await loadI18n();
    expect(normalizeLanguage("en")).toBe("en");
    expect(normalizeLanguage("zh-Hans")).toBe("zh-Hans");
    expect(normalizeLanguage("fr")).toBe("en");
    expect(normalizeLanguage("")).toBe("en");
    expect(normalizeLanguage("zh-CN")).toBe("en");
    expect(normalizeLanguage("ZH-HANS")).toBe("en");
  });

  it("TEST-FEAT-02.1: document lang syncs to selected language", async () => {
    const { setDocumentLanguage } = await loadI18n();
    setDocumentLanguage("zh-Hans");
    expect(document.documentElement.lang).toBe("zh-Hans");
    setDocumentLanguage("en");
    expect(document.documentElement.lang).toBe("en");
  });

  it("TEST-FEAT-02.6: translates known app chrome key", async () => {
    const { t } = await loadI18n();
    expect(t("settings.title", "en")).toBe("Preferences");
    expect(t("settings.title", "zh-Hans")).toBe("偏好设置");
  });

  it("TEST-FEAT-02.6: translates nested dot-notation key", async () => {
    const { t } = await loadI18n();
    expect(t("settings.appearance.language", "en")).toBe("Language");
    expect(t("settings.appearance.language", "zh-Hans")).toBe("语言");
  });

  it("TEST-FEAT-02.6: returns zh-Hans dictionary when selected", async () => {
    const { getDictionary } = await loadI18n();
    expect(getDictionary("zh-Hans").settings.title).toBe("偏好设置");
  });

  it("TEST-FEAT-02.6: returns en dictionary for invalid language", async () => {
    const { getDictionary } = await loadI18n();
    expect(getDictionary("fr").settings.title).toBe("Preferences");
  });

  it("TEST-FEAT-02.6: falls back to en when selected locale misses a key", async () => {
    const { dictionaries, t } = await loadI18n();
    const zhSettings = dictionaries["zh-Hans"].settings as Record<
      string,
      unknown
    >;
    const original = zhSettings.title;

    try {
      delete zhSettings.title;
      expect(t("settings.title", "zh-Hans")).toBe("Preferences");
    } finally {
      zhSettings.title = original;
    }
  });

  it("TEST-FEAT-02.6: returns key when no locale contains the key", async () => {
    const { t } = await loadI18n();
    expect(t("missing.key", "zh-Hans")).toBe("missing.key");
  });
});
```

- [x] **Step 2: Run RED test and confirm failure**

Run:

```powershell
pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts
```

Expected: FAIL because `web/src/i18n` does not exist, while the jsdom document test passes. If the document test fails, fix the Vitest environment before continuing GREEN.

Actual: RED confirmed on 2026-06-06. `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts` exited 1 with 1/10 passing; the jsdom document test passed, and the remaining tests failed because `web/src/i18n` did not exist.

- [x] **Step 3: Commit RED**

```powershell
git add web/tests/i18n/language.test.ts
git commit -m "test(i18n): 加语言字典 RED 测试

Co-authored-by: Codex CLI <noreply@openai.com>"
```

Actual RED commit: `5db114c test(i18n): 增加语言字典 RED 测试`

---

### Task 4: I18n Core GREEN

**Files:**

- Create: `web/src/i18n/dictionaries.ts`
- Create: `web/src/i18n/index.ts`
- Modify: `web/src/stores/settings.ts`

- [x] **Step 1: Create local dictionaries**

Create `web/src/i18n/dictionaries.ts`:

```ts
export const dictionaries = {
  en: {
    settings: {
      title: "Preferences",
      import: "Import...",
      close: "Close settings",
      groups: {
        appearance: "Appearance",
        terminal: "Terminal",
        externalTerminal: "External Terminal",
        git: "Git",
        privacy: "Privacy",
      },
      appearance: {
        language: "Language",
        english: "English",
        simplifiedChinese: "Simplified Chinese",
        theme: "Theme",
        auto: "Auto",
        light: "Light",
        dark: "Dark",
        fontFamily: "Font family",
        fontSize: "Font size",
        backgroundOpacity: "Background opacity",
        backgroundBlur: "Background blur",
        windowPaddingX: "Window padding X",
        windowPaddingY: "Window padding Y",
        cursorStyle: "Cursor style",
        cursorBlock: "Block",
        cursorBar: "Bar",
        cursorUnderline: "Underline",
        cursorBlink: "Cursor blink",
      },
    },
  },
  "zh-Hans": {
    settings: {
      title: "偏好设置",
      import: "导入...",
      close: "关闭设置",
      groups: {
        appearance: "外观",
        terminal: "终端",
        externalTerminal: "外部终端",
        git: "Git",
        privacy: "隐私",
      },
      appearance: {
        language: "语言",
        english: "English",
        simplifiedChinese: "简体中文",
        theme: "主题",
        auto: "自动",
        light: "浅色",
        dark: "深色",
        fontFamily: "字体",
        fontSize: "字号",
        backgroundOpacity: "背景不透明度",
        backgroundBlur: "背景模糊",
        windowPaddingX: "窗口水平内边距",
        windowPaddingY: "窗口垂直内边距",
        cursorStyle: "光标样式",
        cursorBlock: "块状",
        cursorBar: "竖线",
        cursorUnderline: "下划线",
        cursorBlink: "光标闪烁",
      },
    },
  },
} as const;

export type Dictionaries = typeof dictionaries;
export type Language = keyof Dictionaries;
```

- [x] **Step 2: Create i18n helper**

Create `web/src/i18n/index.ts`:

```ts
import { dictionaries, type Language } from "./dictionaries";

export { dictionaries, type Language };

export function normalizeLanguage(value: string | null | undefined): Language {
  return value === "zh-Hans" ? "zh-Hans" : "en";
}

export function getDictionary(language: string | null | undefined) {
  return dictionaries[normalizeLanguage(language)];
}

export function setDocumentLanguage(language: string | null | undefined): void {
  document.documentElement.lang = normalizeLanguage(language);
}

function lookup(dictionary: unknown, key: string): string | undefined {
  const value = key
    .split(".")
    .reduce<unknown>(
      (current, part) =>
        typeof current === "object" && current !== null
          ? (current as Record<string, unknown>)[part]
          : undefined,
      dictionary,
    );
  return typeof value === "string" ? value : undefined;
}

export function t(key: string, language: string | null | undefined): string {
  const selected = getDictionary(language);
  return lookup(selected, key) ?? lookup(dictionaries.en, key) ?? key;
}
```

If the OpenCode app chrome inventory finds a first-slice string that truly needs interpolation, extend `t()` in this task with `{name}` placeholder replacement and a `Record<string, string | number>` values argument. Do not add ICU plural/date/number formatting in FEAT-02.

- [x] **Step 3: Wire settings store**

In `web/src/stores/settings.ts`, import helpers:

```ts
import { normalizeLanguage, setDocumentLanguage } from "../i18n";
```

In `applyCssVars`, after theme sync:

```ts
setDocumentLanguage(normalizeLanguage(s.language));
```

This centralized side effect must run for initial `loadSettings()`, explicit `reloadSettings()`, `settings_changed` events, and successful `updateSettings()` responses because all four paths already call `applyCssVars`. Do not synchronize `document.lang` directly in UI `onChange` handlers.

In `updateSettings`, include:

```ts
if (partial.language !== undefined)
  req.language = normalizeLanguage(partial.language);
```

- [x] **Step 4: Run i18n tests**

Run:

```powershell
pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts
pnpm typecheck
```

Expected: PASS.

Actual GREEN verification on 2026-06-06:

- `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts` PASS, 10/10 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/i18n/dictionaries.ts" "web/src/i18n/index.ts" "web/src/stores/settings.ts" "web/tests/i18n/language.test.ts"` PASS.
- `git diff --check` PASS.
- `pnpm --filter @vibestation/web lint` FAILS on 114 pre-existing formatted-source warnings outside this slice; touched files pass targeted Prettier check.

- [x] **Step 5: Commit GREEN**

```powershell
git add web/src/i18n/dictionaries.ts web/src/i18n/index.ts web/src/stores/settings.ts
git commit -m "feat(i18n): 添加本地语言字典核心

Co-authored-by: Codex CLI <noreply@openai.com>"
```

Actual GREEN commit: `3f806ad feat(i18n): 添加本地语言字典核心`

---

### Task 5: Language Selector RED

**Files:**

- Create: `web/tests/panels/Settings/language-selector.test.tsx`

**Owner:** Codex. No external UI-test gate. After GREEN, optionally send `spike-tmp/dispatch/FEAT-02-3-selector-scope-opencode-prompt.md` to OpenCode for read-only scope verification.

**Precondition:** `pnpm --dir web exec vitest run tests/panels/Settings/ExternalTerminalGroup.test.tsx` must collect and pass. If it fails with `file:///@solid-refresh`, fix `web/vitest.config.ts` first by disabling Solid HMR in Vitest (`solid({ hot: false })`); do not start FEAT-02.3 RED while the runner itself is red.

- [x] **Step 1: Write failing selector test**

Use the existing `ExternalTerminalGroup.test.tsx` style: mock Tauri `invoke`, import the real settings store, and call `reloadSettings()` before rendering. Do not mock `useSettings()` directly.

```tsx
import { render, screen, fireEvent, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "../../../src/bindings/AppSettings";

const { mockAppSettings, resetMockSettings } = vi.hoisted(() => {
  const defaultFixture = (): AppSettings => ({
    language: "en",
    theme: "dark",
    fontFamily: "JetBrains Mono",
    fontSize: 14,
    defaultShell: "/bin/bash",
    pasteProtection: true,
    telemetryOptIn: null,
    gitUserName: null,
    gitUserEmail: null,
    bgOpacity: 0.85,
    bgBlur: 20,
    windowPaddingX: 2,
    windowPaddingY: 2,
    cursorStyle: "block",
    cursorBlink: false,
    unfocusedPaneOpacity: 0.7,
    ptyPoolEnabled: true,
    ptyPoolSize: 1,
    primaryWidth: 236,
    secondaryWidth: 400,
    bottomHeight: 240,
    externalTermPreferred: null,
    externalTermDontAskAgain: false,
  });
  const mockAppSettings: AppSettings = defaultFixture();
  return {
    mockAppSettings,
    resetMockSettings: () => {
      Object.assign(mockAppSettings, defaultFixture());
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: { req?: Partial<AppSettings> }) => {
    if (cmd === "settings_get") {
      return { ...mockAppSettings };
    }
    if (cmd === "settings_update") {
      Object.assign(mockAppSettings, args?.req ?? {});
      return { ...mockAppSettings };
    }
    return null;
  }),
}));

import { invoke } from "@tauri-apps/api/core";
import { reloadSettings } from "../../../src/stores/settings";
import { AppearanceGroup } from "../../../src/panels/Settings/AppearanceGroup";

beforeEach(async () => {
  resetMockSettings();
  vi.mocked(invoke).mockClear();
  await reloadSettings();
});

describe("FEAT-02 Language selector", () => {
  it("TEST-FEAT-02.2: renders language selector and updates zh-Hans", async () => {
    render(() => <AppearanceGroup />);

    const select = (await screen.findByLabelText(
      "Language",
    )) as HTMLSelectElement;
    expect(select.value).toBe("en");
    expect(screen.getByRole("option", { name: "English" })).toHaveValue("en");
    expect(
      screen.getByRole("option", { name: "Simplified Chinese" }),
    ).toHaveValue("zh-Hans");

    fireEvent.change(select, { target: { value: "zh-Hans" } });

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "settings_update",
        expect.objectContaining({
          req: expect.objectContaining({ language: "zh-Hans" }),
        }),
      );
    });
    await waitFor(() => {
      expect(document.documentElement.lang).toBe("zh-Hans");
    });
    const translatedSelect = screen.getByLabelText("语言") as HTMLSelectElement;
    expect(translatedSelect.value).toBe("zh-Hans");
    expect(screen.getByRole("option", { name: "简体中文" })).toHaveValue(
      "zh-Hans",
    );
  });
});
```

- [x] **Step 2: Run RED test and confirm failure**

Run:

```powershell
pnpm --filter @vibestation/web exec vitest run tests/panels/Settings/language-selector.test.tsx
```

Expected: FAIL because AppearanceGroup does not render a Language selector yet.

Actual: RED confirmed on 2026-06-06. `pnpm --filter @vibestation/web exec vitest run tests/panels/Settings/language-selector.test.tsx` collected 1 test and failed because Testing Library could not find label `Language`.

- [x] **Step 3: Commit RED**

```powershell
git add web/tests/panels/Settings/language-selector.test.tsx
git commit -m "test(settings): 加语言选择器 RED 测试

Co-authored-by: Codex CLI <noreply@openai.com>"
```

Actual RED commit: `c9f8ec3 test(settings): 加语言选择器 RED 测试`

---

### Task 6: Language Selector GREEN

**Files:**

- Modify: `web/src/panels/Settings/AppearanceGroup.tsx`

**Owner:** Codex. No external UI-test gate. Keep OpenCode as a post-GREEN read-only scope auditor only.

- [x] **Step 1: Add language selector**

Scope guardrail: do not migrate existing `AppearanceGroup` labels or any other app chrome in FEAT-02.3. The only UI text wired through `t()` in this slice is the new selector label/options using `settings.appearance.language`, `settings.appearance.english`, and `settings.appearance.simplifiedChinese`. Leave `Theme`, `Auto`, `Light`, `Dark`, `Font family`, `Git Log`, sidebar labels, and other fixed strings unchanged until FEAT-02.4.

In `AppearanceGroup.tsx`, import:

```ts
import { t, normalizeLanguage, type Language } from "../../i18n";
```

Add near the top of the component:

```ts
const language = () => normalizeLanguage(settings.language);
const label = (key: string) => t(key, language());
const languages = (): { value: Language; label: string }[] => [
  { value: "en", label: label("settings.appearance.english") },
  { value: "zh-Hans", label: label("settings.appearance.simplifiedChinese") },
];
```

Render this field before Theme:

```tsx
<label class="vs-settings-field">
  <span class="vs-settings-label">{label("settings.appearance.language")}</span>
  <select
    class="vs-settings-select"
    value={language()}
    aria-label={label("settings.appearance.language")}
    onChange={(e) =>
      updateSettings({ language: e.currentTarget.value as Language })
    }
  >
    <For each={languages()}>
      {(item) => <option value={item.value}>{item.label}</option>}
    </For>
  </select>
</label>
```

- [x] **Step 2: Run selector test**

Run:

```powershell
pnpm --filter @vibestation/web exec vitest run tests/panels/Settings/language-selector.test.tsx
pnpm typecheck
```

Expected: PASS.

Actual GREEN verification on 2026-06-06:

- `pnpm --filter @vibestation/web exec vitest run tests/panels/Settings/language-selector.test.tsx` PASS, 1/1 test.
- `pnpm --dir web exec vitest run tests/i18n/language.test.ts tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx` PASS, 16/16 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/panels/Settings/AppearanceGroup.tsx" "web/tests/panels/Settings/language-selector.test.tsx"` PASS.
- `git diff --check` PASS.
- Scope grep confirmed `t()` usage in `AppearanceGroup.tsx` is limited to `settings.appearance.language`, `settings.appearance.english`, and `settings.appearance.simplifiedChinese`; existing `Theme` / `Font family` labels remain hardcoded for FEAT-02.4.

- [x] **Step 3: Commit GREEN**

```powershell
git add web/src/panels/Settings/AppearanceGroup.tsx web/tests/panels/Settings/language-selector.test.tsx
git commit -m "feat(settings): 添加语言选择器

Co-authored-by: Codex CLI <noreply@openai.com>"
```

Actual GREEN commit: `5efd8cf feat(settings): 添加语言选择器`

Post-GREEN OpenCode scope audit on 2026-06-06: APPROVE.

- Required fixes: none.
- Scope result: `AppearanceGroup.tsx` wires `t()` only for `settings.appearance.language`, `settings.appearance.english`, and `settings.appearance.simplifiedChinese`.
- Out-of-scope migrations: none. `Theme`, `Font family`, `Auto` / `Light` / `Dark`, cursor labels, sidebar/status/dialog/chrome strings remain out of FEAT-02.3.
- Verification evidence reported by OpenCode: `pnpm --dir web exec vitest run tests/panels/Settings/language-selector.test.tsx` PASS, `pnpm --dir web exec vitest run tests/i18n/language.test.ts tests/panels/Settings/ExternalTerminalGroup.test.tsx` PASS, `pnpm typecheck` PASS.

---

### Task 7: App Chrome Migration

Status: Settings shell/Appearance/remaining-controls, workspace chrome, and Main/Secondary + common-dialog phase 1 slices completed; Config Import and Git operation dialog chrome remain pending behind the next FEAT-02.4c slice.

**Files:**

- Modify: `web/src/panels/Settings/SettingsPanel.tsx`
- Modify: `web/src/panels/Settings/AppearanceGroup.tsx`
- Modify: `web/src/panels/Settings/TerminalGroup.tsx`
- Modify: `web/src/panels/Settings/ExternalTerminalGroup.tsx`
- Modify: `web/src/panels/Settings/GitGroup.tsx`
- Modify: `web/src/panels/Settings/PrivacyGroup.tsx`
- Modify: `web/src/components/PrimarySidebar.tsx`
- Modify: `web/src/components/ActivityStrip.tsx`
- Modify: `web/src/components/TopBar.tsx`
- Modify: `web/src/components/BottomPanel.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/components/MainContent.tsx`
- Modify: `web/src/components/SecondarySidebar.tsx`
- Modify: `web/src/dialogs/TelemetryOptIn/TelemetryOptInModal.tsx`
- Modify: `web/src/dialogs/PopToExternal/PopToExternalDialog.tsx`
- Modify: `web/src/dialogs/BranchSwitcher/BranchSwitcher.tsx`
- Modify: `web/src/i18n/dictionaries.ts`
- Create: `web/tests/i18n/chrome-copy.test.ts`
- Create: `web/tests/components/chrome-copy.test.tsx`
- Create: `web/tests/dialogs/chrome-copy.test.tsx`
- Modify: `web/tests/panels/Settings/language-selector.test.tsx`
- Create: `web/tests/panels/Settings/settings-panel-copy.test.tsx`
- Pending FEAT-02.4c follow-up: Config Import and Git operation dialog common chrome items only when explicitly listed by the external inventory/review.

- [x] **Step 1: Write Settings-slice text coverage tests**

Before editing UI copy, use this OpenCode-derived checklist as the FEAT-02.4 scope boundary. Only strings listed here are in FEAT-02.4 scope; other fixed English strings remain follow-up candidates. Existing hardcoded Chinese copy in these areas must also move into the dictionary so the default locale stays English.

- Settings shell: `Preferences`, `Import...`, close button aria-label, `Appearance` / `Terminal` / `External Terminal` / `Git` / `Privacy`.
- Settings Appearance: `Language`, `Theme`, `Auto`, `Light`, `Dark`, `Font family`, `Font size`, `Background opacity`, `Background blur`, `Window padding`, `Cursor style`, `Block`, `Bar`, `Underline`, `Cursor blink`.
- Settings Terminal / External Terminal / Git / Privacy first-level controls: `Default shell`, `Paste protection`, `PTY warm pool`, `Don't ask again`, `Ask every time`, `Telemetry`, `Collection endpoint`, `View what we collect`; long `We collect` / `We do NOT collect` prose moves to follow-up `content.privacy.*`.
- Primary Sidebar: `Workspaces`, `No workspaces yet.`, `Import settings from another terminal`, `Create workspace` aria-label.
- Activity Strip / Bottom Panel: `Git Log`, `Git Status`, `Output`, `Diff`, panel toggle tooltip / aria-label.
- TopBar / StatusBar / App chrome: `Minimize`, `Maximize`, `Close`, `Toggle primary sidebar`, `remote`, `Merge`, settings gear label, `vX · alpha`, `ipc: connecting...`, `ipc error`, `Dismiss error`.
- Main / Secondary chrome: `Select or create a workspace to get started`, `Back to Terminal`, resize/sidebar aria-label.
- Dialog common chrome: `Help improve Vibestation`, `Decline`, `Accept`, `Open in External Terminal`, `Don't ask again`, `Cancel`, `Retry`, `No external terminals detected.`, `No branch matched`, `Loading branches...`, `Switch branch`, `Close <name> dialog` aria-label.
- Existing non-English hardcoded copy found by OpenCode: App remote aria Chinese sentence, CreateBranch `取消` / `确认`, delete modal description, and loading errors only if they fall into the chrome/dialog/common error list above.
- Exclude: class / role / type / internal data values, font names, env lists, dynamic backend labels, backend raw errors, Git content, branch names, commit messages, terminal output, and Tauri native menu labels.

OpenCode inventory response on 2026-06-06: `INVENTORY-COMPLETE`. It confirmed `SettingsPanel.tsx` and `AppearanceGroup.tsx` were already migrated, identified remaining Settings first-level controls in `TerminalGroup.tsx`, `ExternalTerminalGroup.tsx`, `GitGroup.tsx`, and `PrivacyGroup.tsx`, and kept font names, env list values, dynamic backend labels, Git content, terminal output, and long privacy prose out of scope.

Actual Settings-slice RED coverage on 2026-06-06:

- `web/tests/i18n/chrome-copy.test.ts` covers `settings.title`, `settings.import`, `settings.close`, Settings group labels, and Appearance labels already present in ADR-025 dictionaries.
- `web/tests/panels/Settings/settings-panel-copy.test.tsx` renders the real `SettingsPanel` with `mockAppSettings.language = "zh-Hans"` and asserts the Settings shell/group labels are Chinese.
- `web/tests/panels/Settings/language-selector.test.tsx` was extended to assert a live selector change updates existing Appearance labels (`Theme` -> `主题`, `Font family` -> `字体`) without remounting.
- RED command: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/panels/Settings/settings-panel-copy.test.tsx tests/panels/Settings/language-selector.test.tsx` exited 1 with 2 failed component tests. Failures were expected: current UI still rendered `Preferences`, `Theme`, `Font family`, and other hardcoded English labels.
- RED commit: `ebbe03f test(i18n): 加首批设置文案迁移 RED 测试`.

- [x] **Step 1b: Add component-level refresh coverage**

Extend `web/tests/panels/Settings/language-selector.test.tsx` or add a focused Settings component test using the same Tauri `invoke` + `reloadSettings()` pattern. After FEAT-02.4 migrates Appearance labels, changing the selector to `zh-Hans` must update at least one already-rendered label without remounting, for example `Theme` -> `主题`.

Actual: implemented in `web/tests/panels/Settings/language-selector.test.tsx`; the test uses real `reloadSettings()` and `updateSettings()` behavior with a hoisted Tauri `invoke` mock.

- [x] **Step 2: Migrate SettingsPanel group labels**

In `SettingsPanel.tsx`, derive:

```ts
const { settings } = useSettings();
const language = () => normalizeLanguage(settings.language);
const label = (key: string) => t(key, language());
```

Change group definitions to store `titleKey` instead of `title`, for example:

```ts
{ id: "appearance", titleKey: "settings.groups.appearance", component: AppearanceGroup }
```

Render:

```tsx
<span class="vs-settings-group-title">{label(group.titleKey)}</span>
```

Change dialog labels:

```tsx
aria-label={label("settings.title")}
<h2 class="vs-settings-title">{label("settings.title")}</h2>
```

Actual: `SettingsPanel.tsx` now derives `label()` from `useSettings().settings.language`, stores `titleKey` per group, renders translated dialog title/import/close labels, and adds explicit group button `aria-label`s.

- [x] **Step 3: Migrate AppearanceGroup existing labels**

Replace Theme, Auto, Light, Dark, Font family, Font size, Background opacity, Background blur, Window padding X/Y, Cursor style, Block, Bar, Underline, Cursor blink with `label("settings.appearance.<key>")`. Use `settings.terminal.*`, `settings.externalTerminal.*`, `settings.git.*`, and `settings.privacy.*` for first-level controls listed above; defer long privacy prose to a later `content.privacy.*` task.

Actual: AppearanceGroup migrated the existing labels listed above. Settings Terminal / External Terminal / Git / Privacy first-level controls were migrated in the follow-up settings-controls slice below. Non-settings app chrome remains pending FEAT-02.4 follow-up.

- [x] **Step 4: Run focused tests**

Run:

```powershell
pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/panels/Settings/settings-panel-copy.test.tsx tests/panels/Settings/language-selector.test.tsx
pnpm --dir web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx
pnpm typecheck
```

Expected: PASS.

Actual GREEN verification on 2026-06-06:

- `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/panels/Settings/settings-panel-copy.test.tsx tests/panels/Settings/language-selector.test.tsx` PASS, 5/5 tests.
- `pnpm --dir web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx` PASS, 20/20 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/panels/Settings/SettingsPanel.tsx" "web/src/panels/Settings/AppearanceGroup.tsx" "web/tests/i18n/chrome-copy.test.ts" "web/tests/panels/Settings/settings-panel-copy.test.tsx" "web/tests/panels/Settings/language-selector.test.tsx"` PASS.
- `git diff --check` PASS.
- Scope grep confirmed the touched UI source uses only Settings dictionary keys and does not migrate `Git Log`, terminal output, Git content, branch names, commit messages, or Tauri native menu labels.
- `pnpm --filter @vibestation/web lint` still FAILS on 113 pre-existing Prettier warnings outside this slice; touched files pass targeted Prettier check.

- [x] **Step 5: Commit migration slice**

```powershell
git add web/src/panels/Settings/AppearanceGroup.tsx web/src/panels/Settings/SettingsPanel.tsx web/tests/panels/Settings/settings-panel-copy.test.tsx
git commit -m "feat(i18n): 迁移设置面板文案" -m "Co-authored-by: Codex CLI <noreply@openai.com>"
```

GREEN commit: `985efc7 feat(i18n): 迁移设置面板文案`.

Grok implementation review on 2026-06-06: `APPROVE-WITH-NITS` for the Settings shell + Appearance labels slice. Required fixes: none. Non-blocking follow-ups applied in `9c69a42 test(settings): 补外观文案审计跟进`: `settings-panel-copy.test.tsx` now asserts initial `zh-Hans` Appearance inner labels (`主题`, `字体`), and `AppearanceGroup.tsx` renames the theme `For` callback parameter from `t` to `theme` to avoid shadowing the imported translation helper.

Additional Settings-controls slice on 2026-06-06:

- RED tests: `web/tests/i18n/chrome-copy.test.ts` now covers `settings.terminal.*`, `settings.externalTerminal.*`, `settings.git.*`, and `settings.privacy.*`; `web/tests/panels/Settings/settings-panel-copy.test.tsx` renders real `SettingsPanel` in `zh-Hans` and asserts remaining Settings controls.
- RED command: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/panels/Settings/settings-panel-copy.test.tsx` exited 1. Expected failures: missing dictionary key returned raw `settings.terminal.defaultShell`, and component UI still rendered English `Default shell`.
- RED commit: `8f3c84d test(i18n): 加设置控件文案迁移 RED 测试`.
- GREEN implementation: added dictionary namespaces for remaining Settings controls and wired `TerminalGroup.tsx`, `ExternalTerminalGroup.tsx`, `GitGroup.tsx`, and `PrivacyGroup.tsx` through `label()`. Dynamic shell/terminal labels, env whitelist values, Git content, terminal output, backend raw strings, and long privacy prose remain excluded.
- GREEN verification: `pnpm --dir web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx` PASS, 22/22 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/i18n/dictionaries.ts" "web/src/panels/Settings/TerminalGroup.tsx" "web/src/panels/Settings/ExternalTerminalGroup.tsx" "web/src/panels/Settings/GitGroup.tsx" "web/src/panels/Settings/PrivacyGroup.tsx" "web/tests/i18n/chrome-copy.test.ts" "web/tests/panels/Settings/settings-panel-copy.test.tsx"` PASS.
- `git diff --check` PASS.
- Scope grep confirmed touched Settings group code uses only `settings.terminal.*`, `settings.externalTerminal.*`, `settings.git.*`, and `settings.privacy.*`; it still contains excluded long privacy prose mentioning commit messages, which remains intentionally unmigrated for `content.privacy.*`.
- `pnpm --filter @vibestation/web lint` still FAILS on 109 pre-existing Prettier warnings outside this slice; touched files pass targeted Prettier check and are no longer listed.
- GREEN commit: `3451e48 feat(i18n): 迁移设置控件文案`.

Additional workspace-chrome slice on 2026-06-06:

- RED tests: `web/tests/i18n/chrome-copy.test.ts` now covers `chrome.sidebars.*`, `chrome.activity.*`, `chrome.bottom.*`, `chrome.topbar.*`, `chrome.window.*`, and `chrome.status.*`; `web/tests/components/chrome-copy.test.tsx` renders real `PrimarySidebar`, `ActivityStrip`, `BottomPanel`, and `TopBar` with `mockAppSettings.language = "zh-Hans"`.
- RED command: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx` exited 1. Expected failures: missing dictionary key returned raw `chrome.sidebars.primary`, and component UI still rendered English `Primary sidebar`, `Panel toggles`, `Bottom panel`, and `Toggle primary sidebar`.
- RED commit: `8cf7478 test(i18n): 加工作台 chrome 文案 RED 测试`.
- GREEN implementation: added `chrome.*` dictionary namespaces and wired `PrimarySidebar.tsx`, `ActivityStrip.tsx`, `BottomPanel.tsx`, `TopBar.tsx`, and selected `App.tsx` status labels through `label()`. Workspace names, paths, Git content, branch/commit/message text, backend raw strings, terminal output, and Tauri native menu labels remain excluded.
- GREEN verification: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx` PASS, 8/8 tests.
- Regression verification: `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx` PASS, 26/26 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/i18n/dictionaries.ts" "web/src/components/PrimarySidebar.tsx" "web/src/components/ActivityStrip.tsx" "web/src/components/BottomPanel.tsx" "web/src/components/TopBar.tsx" "web/src/App.tsx" "web/tests/i18n/chrome-copy.test.ts" "web/tests/components/chrome-copy.test.tsx"` PASS.
- `git diff --check` PASS.
- `pnpm lint` still FAILS on 108 pre-existing Prettier warnings outside this slice; touched files pass targeted Prettier check and are not listed.
- GREEN commit: `73ec6ca feat(i18n): 迁移工作台 chrome 文案`.

Additional Main/Secondary + common-dialog phase 1 slice on 2026-06-06:

- RED tests: `web/tests/i18n/chrome-copy.test.ts` now covers `chrome.main.*`, `chrome.sidebars.secondary`, `dialogs.telemetry.*`, `dialogs.popToExternal.*`, and `dialogs.branchSwitcher.*`; `web/tests/components/chrome-copy.test.tsx` renders `MainContent` and `SecondarySidebar`; `web/tests/dialogs/chrome-copy.test.tsx` renders Telemetry opt-in, Pop to External, and Branch Switcher with `mockAppSettings.language = "zh-Hans"`.
- RED command: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx tests/dialogs/chrome-copy.test.tsx` exited 1. Expected failures: missing dictionary key returned raw `chrome.main.contentArea`, and component UI still rendered English `Main content area`, `Secondary sidebar`, `Help improve Vibestation`, `Open in External Terminal`, and `Switch branch`.
- RED commit: `3bcbfa4 test(i18n): 加剩余 chrome 文案 RED 测试`.
- GREEN implementation: added `chrome.main.*` and selected `dialogs.*` namespaces, then wired `MainContent.tsx`, `SecondarySidebar.tsx`, `TelemetryOptInModal.tsx`, `PopToExternalDialog.tsx`, and `BranchSwitcher.tsx` through `label()`. Long telemetry prose, Git operation business errors, backend raw errors, terminal output, branch names, and commit messages remain excluded.
- GREEN verification: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx tests/dialogs/chrome-copy.test.tsx` PASS, 16/16 tests.
- Existing dialog regression: `pnpm --filter @vibestation/web exec vitest run tests/dialogs/PopToExternal/PopToExternalDialog.test.tsx` PASS, 6/6 tests.
- FEAT-02 regression: `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx tests/dialogs/chrome-copy.test.tsx tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx` PASS, 34/34 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/i18n/dictionaries.ts" "web/src/components/MainContent.tsx" "web/src/components/SecondarySidebar.tsx" "web/src/dialogs/TelemetryOptIn/TelemetryOptInModal.tsx" "web/src/dialogs/PopToExternal/PopToExternalDialog.tsx" "web/src/dialogs/BranchSwitcher/BranchSwitcher.tsx" "web/tests/i18n/chrome-copy.test.ts" "web/tests/components/chrome-copy.test.tsx" "web/tests/dialogs/chrome-copy.test.tsx"` PASS.
- `git diff --check` PASS.
- `pnpm lint` still FAILS on 103 pre-existing Prettier warnings outside this slice; touched files pass targeted Prettier check and are not listed.
- GREEN commit: `decb2e3 feat(i18n): 迁移剩余主界面与常见对话框文案`.

Additional Git operation dialog phase 2 slice on 2026-06-06:

- RED tests: `web/tests/i18n/chrome-copy.test.ts` now covers `dialogs.common.*`, `dialogs.createBranch.*`, `dialogs.cherryPick.*`, `dialogs.merge.*`, and `dialogs.remoteSelector.*`; `web/tests/dialogs/chrome-copy.test.tsx` renders Create Branch, Cherry Pick, Merge, and Remote Selector chrome labels.
- RED command: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/dialogs/chrome-copy.test.tsx` exited 1. Expected failures: missing dictionary keys and still-hardcoded English/Chinese dialog chrome, including Create Branch `取消` / `确认` and Merge/CherryPick/RemoteSelector headings/actions.
- RED commit: `c1de708 test(i18n): 加 Git 对话框 chrome RED 测试`.
- GREEN implementation: added common dialog and Git operation dialog dictionary keys, then wired `CreateBranchDialog.tsx`, `CherryPickDialog.tsx`, `MergeDialog.tsx`, and `RemoteSelector.tsx` through `label()`. Branch names, commit messages, remote names/URLs, strategy enum labels, and backend/raw operation errors remain excluded.
- GREEN verification: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/dialogs/chrome-copy.test.tsx` PASS, 16/16 tests.
- FEAT-02 regression: `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx tests/dialogs/chrome-copy.test.tsx tests/dialogs/PopToExternal/PopToExternalDialog.test.tsx tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx` PASS, 45/45 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/i18n/dictionaries.ts" "web/src/dialogs/CreateBranchDialog/CreateBranchDialog.tsx" "web/src/dialogs/CherryPickDialog/CherryPickDialog.tsx" "web/src/dialogs/MergeDialog/MergeDialog.tsx" "web/src/dialogs/RemoteSelector/RemoteSelector.tsx" "web/tests/i18n/chrome-copy.test.ts" "web/tests/dialogs/chrome-copy.test.tsx"` PASS.
- `git diff --check` PASS.
- `pnpm lint` still FAILS on 99 pre-existing repository-wide Prettier warnings outside this slice; touched TSX source/test files pass targeted Prettier check.
- GREEN commit: `2658179 feat(i18n): 迁移 Git 对话框通用文案`.

Additional remaining dialog phase 3 slice on 2026-06-06:

- RED tests: `web/tests/i18n/chrome-copy.test.ts` now covers `dialogs.forceDelete.*`, `dialogs.forcePush.*`, `dialogs.dirtyTree.*`, `dialogs.gitSync.*`, `dialogs.auth.*`, and `dialogs.configImport.*`; `web/tests/dialogs/chrome-copy.test.tsx` renders Force Delete, Force Push, Dirty Tree, Git Sync Progress, Auth, and Config Import chrome labels.
- RED command: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/dialogs/chrome-copy.test.tsx` exited 1. Expected failures: missing dictionary keys and hardcoded Chinese/English UI in destructive branch dialogs, dirty tree actions, Git sync progress, auth, and config import chrome.
- RED commit: `b0a0ae5 test(i18n): 加剩余对话框 chrome RED 测试`.
- GREEN implementation: added remaining dialog dictionary namespaces, then wired `ForceDeleteDialog.tsx`, `ForcePushDialog.tsx`, `DirtyTreeDialog.tsx`, `GitSyncProgressDialog.tsx`, `AuthDialog.tsx`, and `ConfigImportDialog.tsx` through `label()`. Branch names, remote names/URLs, commit messages, file paths, credential values, backend/raw errors, and longer business-process prose remain excluded.
- GREEN verification: `pnpm --filter @vibestation/web exec vitest run tests/i18n/chrome-copy.test.ts tests/dialogs/chrome-copy.test.tsx` PASS, 24/24 tests.
- FEAT-02 regression: `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx tests/dialogs/chrome-copy.test.tsx tests/dialogs/PopToExternal/PopToExternalDialog.test.tsx tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx` PASS, 53/53 tests.
- `pnpm typecheck` PASS.
- `npx prettier --check "web/src/i18n/dictionaries.ts" "web/src/dialogs/ForceDeleteDialog/ForceDeleteDialog.tsx" "web/src/dialogs/ForcePushDialog/ForcePushDialog.tsx" "web/src/dialogs/DirtyTreeDialog/DirtyTreeDialog.tsx" "web/src/dialogs/GitSyncProgress/GitSyncProgressDialog.tsx" "web/src/dialogs/AuthDialog/AuthDialog.tsx" "web/src/dialogs/ConfigImport/ConfigImportDialog.tsx" "web/tests/i18n/chrome-copy.test.ts" "web/tests/dialogs/chrome-copy.test.tsx"` PASS.
- `git diff --check` PASS.
- GREEN commit: `463916e feat(i18n): 迁移剩余对话框通用文案`.

---

### Task 8: Verification and Spec Closure

**Files:**

- Modify: `docs/tasks/FEAT-02-language-settings.md`
- Modify: `docs/adr/ADR-025-frontend-i18n-dictionary.md`

- [ ] **Step 1: Run full verification**

Run:

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
pnpm typecheck
pnpm --filter @vibestation/web exec vitest run
```

Expected: PASS, or document exact pre-existing failures with branch-vs-main evidence before claiming green.

- [ ] **Step 2: Runtime smoke**

Run:

```powershell
pnpm tauri:dev
```

Verify:

- First launch with no `language` key displays English.
- Preferences → Appearance → Language can select `简体中文`.
- Settings panel labels switch without restart.
- Restart keeps `zh-Hans`.
- Terminal output and Git content remain original.

- [ ] **Step 3: Update FEAT-02 completion notes**

Set `§10 Completion Notes` with the actual RED/GREEN commits and verification output summary. Keep frontmatter `status: in-progress` during implementation; only flip to `done` after review/Arbiter approval. If the shipped FEAT-02 dictionary still does not need interpolation, explicitly record interpolation as deferred rather than adding unused helper surface.

- [ ] **Step 4: ADR-025 acceptance**

If Arbiter approves the i18n architecture, change ADR-025 status from `proposed` to `accepted` and add the approval date. If not approved, update FEAT-02 to the chosen architecture before implementation.

Progress on 2026-06-06:

- Non-interactive verification run:
  - `cargo test --workspace` PASS.
  - `cargo clippy --workspace --all-targets -- -D warnings` PASS.
  - `cargo fmt --all -- --check` PASS.
  - `pnpm typecheck` PASS.
  - FEAT-02 focused regression `pnpm --filter @vibestation/web exec vitest run tests/i18n/language.test.ts tests/i18n/chrome-copy.test.ts tests/components/chrome-copy.test.tsx tests/dialogs/chrome-copy.test.tsx tests/dialogs/PopToExternal/PopToExternalDialog.test.tsx tests/panels/Settings/ExternalTerminalGroup.test.tsx tests/panels/Settings/language-selector.test.tsx tests/panels/Settings/settings-panel-copy.test.tsx` PASS, 53/53 tests.
- Full web vitest status:
  - `pnpm --filter @vibestation/web exec vitest run` FAILS in pre-existing script suites outside FEAT-02: `tests/scripts/setup-git-hooks.test.ts` 3 failures from Rolldown parsing transformed shebang, and `tests/scripts/validate-runtime-evidence.test.ts` 7 failures from temporary report `ENOENT`.
  - The same full run reported 68 passed files / 567 passed tests before the 10 script failures.
- Lint status:
  - `pnpm lint` FAILS on 94 pre-existing repository-wide Prettier warnings outside FEAT-02 touched files.
- Launch smoke:
  - `pnpm tauri:dev` PASS for controlled launch: Vite ready at `http://localhost:1420/`, `cargo run` dev build finished, and `vibestation-app.exe` started before the process tree was stopped.
  - Observed runtime logs included pre-existing warnings `cleanups created outside a createRoot` and `tab not found ... database not initialized`; manual UI language switch / restart persistence smoke is still pending.
- ADR status:
  - `docs/adr/ADR-025-frontend-i18n-dictionary.md` changed from `proposed` to `accepted` after FEAT-02.4 implementation and multi-agent review.

- [ ] **Step 5: Commit docs closure**

```powershell
git add docs/tasks/FEAT-02-language-settings.md docs/adr/ADR-025-frontend-i18n-dictionary.md
git commit -m "docs(spec): 回填 FEAT-02 语言设置完成记录

Co-authored-by: Codex CLI <noreply@openai.com>"
```

---

## Self-Review

**Spec coverage:** AC1/AC4/AC5 are covered by Tasks 1-2; AC6/AC8 by Tasks 3-4; AC2/AC3 by Tasks 5-6; AC7 by Task 7; full §9 by Task 8.

**Placeholder scan:** The plan contains no TBD/TODO implementation steps. Runtime evidence is intentionally deferred to Task 8 execution because it requires the finished app.

**Type consistency:** The same `Language = "en" | "zh-Hans"` model is used in Rust persistence, generated TS bindings, i18n helper, settings store, and selector UI.
