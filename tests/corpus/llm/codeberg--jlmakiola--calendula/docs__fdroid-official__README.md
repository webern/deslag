# Official F-Droid submission (draft)

Goal: publish Calendula to the **official F-Droid repo** *alongside* the
self-hosted repo, as **one app** — same signed binary in both channels, so
existing self-hosted users migrate with no reinstall and no data loss.

This folder holds the **draft** fdroiddata recipe. Nothing here is submitted
yet. The self-hosted pipeline (`../../fdroid-metadata/`, `.gitea/workflows/release.yaml`)
is unaffected by these files.

## Publishing model: reproducible build + developer-signed binary

F-Droid builds Calendula from source on its buildserver, then verifies the
output is byte-for-byte identical to **our** signed APK (fetched via the
`Binaries` URL). On a match it publishes **our** binary, signed with **our**
key — identified by `AllowedAPKSigningKeys`. Result: official and self-hosted
both carry the same signature. If a build ever fails to match, F-Droid simply
skips that version (fails safe — no bad publish).

## Verification status (2026-06-21)

- ✅ **Run-to-run reproducible** — `v2.7.0` built twice from a clean worktree
  (R8 minify + resource-shrink, build cache off) → byte-for-byte identical
  (`sha256 568c944a…`).
- ✅ **Cross-JDK reproducible** — rebuilt with JDK 17 and JDK 21 → identical
  APK. JDK version is not a sensitivity, so it need not be pinned.
- ✅ **Tag self-consistent** — committed `versionCode`/`versionName` already
  equal the tag-derived values, so F-Droid building the tag as-is matches the
  CI-published binary (CI's `sed` substitution is a no-op).
- ✅ **App signing cert SHA-256 extracted** from published APKs (identical
  across v1.0.0 / v2.0.0 / v2.4.0): `5cdaee8e…`. No keystore needed.
- ✅ **Eligibility** — MIT, 100% FOSS deps (no Play Services / Firebase /
  analytics / billing), no `INTERNET` permission, no native code.
- ✅ **End-to-end reproducible vs the DISTRIBUTED binary** — built tag `v2.7.1`
  from source and compared against the published `calendula_v2.7.1.apk`: every
  app entry (dex / resources / assets / manifest) byte-identical. The only
  difference was `META-INF/version-control-info.textproto` (AGP's git metadata,
  env-dependent). Fixed with `vcsInfo { include = false }` on the release build;
  re-validated → 0 non-signature differences. **Takes effect from the first
  release built after that change — submit the recipe starting at that version**
  (v2.7.1 and earlier still embed the textproto and won't verify).
- ⚠️ **Not yet proven**: cross-host / fixed-build-path reproducibility on
  F-Droid's buildserver (low risk for a no-NDK pure-JVM app; with vcsInfo
  disabled, the env-dependent field is gone; F-Droid confirms at review).
- ✅ **Buildserver toolchain supported** (checked 2026-06-21):
  - **Gradle 9.5.1** is in F-Droid's gradle-transparency-log (`checksums.json`)
    with a verified sha256, so `gradlew.py` will download + verify + run it.
  - **AGP 9.2** is in `gradlew.py`'s `MIN_GRADLE_VERSION` map (`9.2 -> 9.4.1`).
  - **JDK 17** is standard on the buildserver (AGP 9.2 requires exactly 17).
  - **build-tools 36.0.0 / android-37**: not statically preinstalled (baseline
    stops at 33), but `provision-android-sdk` makes `$ANDROID_HOME/build-tools`
    and `/platforms` group-writable so AGP/Gradle install newer ones on demand.

## Before submitting — checklist

1. Confirm the `Binaries` URL is publicly reachable and stable long-term, e.g.
   `https://apps.dev.jeanlucmakiola.de/dev/fdroid/repo/calendula_v2.7.0.apk`
   resolves to the dev-signed APK. F-Droid re-fetches it on every build.
2. Confirm F-Droid's buildserver supports the toolchain (see timing risk).
3. (Recommended) Reproduce one published release end-to-end: build the tag from
   source, strip signatures, and diff against the downloaded `calendula_v<ver>.apk`
   to confirm the from-source build matches the *distributed* binary — not just
   another local build.

## Submission steps (fdroiddata, GitLab)

1. Fork `https://gitlab.com/fdroid/fdroiddata`.
2. Copy `de.jeanlucmakiola.calendula.yml` to `metadata/` in the fork.
3. Test locally with `fdroid build -v de.jeanlucmakiola.calendula` and
   `fdroid lint de.jeanlucmakiola.calendula` (and `fdroid readmeta`).
4. Open a Merge Request. Initial review can take weeks to months; the
   self-hosted repo keeps serving in the meantime, so there's no rush.

## Listing metadata (descriptions, screenshots, icon)

These are **not** in the recipe `.yml`. F-Droid's `fdroid update` harvests them
automatically from the fastlane tree in the app's source repo:

    fastlane/metadata/android/<locale>/
      short_description.txt   full_description.txt   title.txt
      images/icon.png         images/phoneScreenshots/*.png
      changelogs/<versionCode>.txt   (optional, per release)

This is the single source of truth: en-US + de-DE already exist there, and the
self-hosted repo is generated from the same tree at release time via
`scripts/fastlane_to_fdroid_localized.sh`. Nothing extra to add to fdroiddata —
F-Droid picks the listing up from source. (Screenshots may not be committed to
the fdroiddata repo anyway, so the source-tree fastlane layout is required.)

Per-version changelogs: the self-hosted release workflow generates
`changelogs/<versionCode>.txt` at release time. For the official repo to show a
changelog, that file must be committed into `fastlane/metadata/android/<locale>/changelogs/`
at the tagged commit — wire this into the release/version-bump step, or accept
no in-client changelog for the official listing initially.
