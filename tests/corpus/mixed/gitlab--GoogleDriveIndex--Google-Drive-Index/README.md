# Google Drive Index

A fast, modern, serverless Google Drive directory listing powered by **Cloudflare Workers**. Browse, search, stream, and share your Google Drive files through a beautiful web interface — no server required.

[![npm version](https://img.shields.io/npm/v/@googledrive/index.svg)](https://www.npmjs.com/package/@googledrive/index)
[![jsDelivr Hits/Month](https://data.jsdelivr.com/v1/package/npm/@googledrive/index/badge/month)](https://www.jsdelivr.com/package/npm/@googledrive/index)
[![jsDelivr Hits/Week](https://data.jsdelivr.com/v1/package/npm/@googledrive/index/badge/week)](https://www.jsdelivr.com/package/npm/@googledrive/index)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

---

## Table of Contents

- [Features](#features)
- [Architecture Overview](#architecture-overview)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
  - [Method A: OAuth 2.0 (Personal Drive)](#method-a-oauth-20-personal-drive)
  - [Method B: Service Account (Shared Drive)](#method-b-service-account-shared-drive)
- [Configuration Reference](#configuration-reference)
  - [authConfig](#authconfig)
  - [uiConfig](#uiconfig)
  - [player_config](#player_config)
- [Multiple Drives](#multiple-drives)
- [Per-Drive Credentials](#per-drive-credentials)
- [Cross-Drive ID Lookup](#cross-drive-id-lookup)
- [Service Account Setup](#service-account-setup)
- [Login & Authentication](#login--authentication)
  - [Local Username/Password](#local-usernamepassword)
  - [Google OAuth Login (Social)](#google-oauth-login-social)
  - [Cloudflare KV User Database](#cloudflare-kv-user-database)
  - [Session Security Options](#session-security-options)
- [Per-Folder Password Protection](#per-folder-password-protection)
- [Encryption Keys](#encryption-keys)
- [Download URL Protection](#download-url-protection)
- [Region & ASN Blocking](#region--asn-blocking)
- [Load Balancing (Multiple Download Domains)](#load-balancing-multiple-download-domains)
- [Themes](#themes)
- [Media Players](#media-players)
- [File Viewers](#file-viewers)
- [Search](#search)
- [Deployment](#deployment)
  - [Via Cloudflare Dashboard (No CLI)](#via-cloudflare-dashboard-no-cli)
  - [Via Wrangler CLI](#via-wrangler-cli)
- [Development Setup](#development-setup)
- [Build Process (CDN Assets)](#build-process-cdn-assets)
- [API Reference](#api-reference)
- [Troubleshooting / FAQ](#troubleshooting--faq)
- [Planned Features](#planned-features)
- [Changelog](#changelog)
- [Credits](#credits)
- [Sponsors](#sponsors)
- [License](#license)

---

## Features

### Core
- **Personal Drive** (My Drive) and **Shared/Team Drive** support
- **Multiple drives** on a single deployment with a unified homepage
- **Auto-discover Shared Drives** in the generator — fetch all drives from your account and add them with one click
- **Serverless** — runs entirely on Cloudflare Workers (free tier supported)
- **No database** required for basic operation
- **Infinite scroll** with paginated file listing
- **Full-text search** across one or all configured drives
- **File ID → path resolution** for bookmarking/sharing files
- **Google Workspace export** — Google Docs, Sheets, and Slides appear in the listing and can be exported to PDF, DOCX/XLSX/PPTX, TXT/CSV
- **Storage quota display** — optional bar showing Drive usage (disabled by default, enable via `show_quota`)

### Security
- **Username/password login** with encrypted session cookies (AES-CBC + HMAC-SHA256)
- **Google OAuth sign-in** for whitelisted Google accounts
- **Cloudflare KV** user database for dynamic user management
- **Per-folder `.password` file** protection
- **Encrypted, expiring download links** (no direct Google Drive exposure)
- **IP-locked download links** (optional)
- **Single-session enforcement** (optional)
- **IP-change logout** (optional)
- **Region/ASN blocking**
- **Referer-based direct-link protection**

### UI/UX
- **Dark and Light themes** with auto-detect from system preference
- **Responsive design** — mobile, tablet, desktop
- **Bootstrap 5** with 26+ Bootswatch themes available
- **Breadcrumb navigation**
- **Column sorting** by name, size, modified date
- **Folder filter** (real-time search within current directory)
- **Bulk file selection** and copy links
- **README.md rendering** below file list
- **HEAD.md rendering** above file list

### Media
- **Video player** — Plyr, Video.js, DPlayer, or JWPlayer
- **Audio player** — APlayer with auto-playlist for audio folders
- **PDF viewer** — PDF.js with page navigation and zoom
- **Image viewer** with lazy loading
- **Code viewer** with syntax-aware display (up to 2 MB)
- **HLS / m3u8 stream** support

---

## Architecture Overview

```
Browser
  │
  ▼
Cloudflare Workers (src/worker.js)
  │  ├─ Serves HTML shell (Bootstrap + app.js)
  │  ├─ POST /{n}:/ ─────────► Google Drive API v3 (files.list)
  │  ├─ POST /{n}:search ────► Google Drive API v3 (files.list with q=)
  │  ├─ GET  /download.aspx ─► Google Drive API v3 (files.get?alt=media or files.export for Workspace)
  │  ├─ GET  /{n}:quota     ─► Google Drive API v3 (about.get?fields=storageQuota)
  │  ├─ GET  /findpath?id=  ─► Cross-drive path lookup + redirect
  │  ├─ GET  /?driveid=     ─► Cross-drive raw ID lookup + redirect
  │  ├─ POST /copy          ─► Google Drive API v3 (files/copy)
  │  └─ Auth: AES-CBC session cookie, HMAC-signed download links
  │
  ▼
app.js (frontend — loaded from jsDelivr CDN)
  │  ├─ Renders file list, breadcrumbs, search
  │  └─ Loads media players on demand
  │
  ▼
Google Drive API
```

**Key design decisions:**
- Worker caches drive initialisation in memory (warm starts are fast)
- File IDs are **encrypted** before being sent to the browser; the browser never sees real Google Drive IDs
- Download links are **HMAC-signed + time-limited** (default 7 days)
- Assets (CSS/JS) are served from **jsDelivr** via the npm package so Worker CPU time is not wasted on static files

---

## Prerequisites

| Requirement | Notes |
|---|---|
| Cloudflare account | Free tier works. Workers free plan: 100k requests/day |
| Google Cloud project | Free. Needed to enable the Drive API |
| OAuth 2.0 credentials **or** a Service Account JSON key | See below |
| Node.js 18+ | Only for local development / CLI deployment |
| `wrangler` CLI | `npm i -g wrangler` — only for CLI deployment |

---

## Quick Start

### Method A: OAuth 2.0 (Personal Drive)

#### Step 1 — Google Cloud Setup

1. Go to [Google Cloud Console](https://console.cloud.google.com) → create or select a project
2. **APIs & Services → Enable APIs** → enable **Google Drive API**
3. **APIs & Services → Credentials → Create Credentials → OAuth 2.0 Client ID**
   - Application type: **Desktop app**
   - Copy the `client_id` and `client_secret`
4. Generate a `refresh_token` by visiting the generator at [bdi-generator.hashhackers.com](https://bdi-generator.hashhackers.com) or running the OAuth flow manually (see [Troubleshooting](#getting-a-refresh-token-manually))

#### Step 2 — Configure `src/worker.js`

Open `src/worker.js` and fill in the top section:

```js
const authConfig = {
  "siteName":      "My Drive Index",
  "client_id":     "123456789-abc.apps.googleusercontent.com",
  "client_secret": "GOCSPX-xxxxxxxxxxxxxxxxxxxxxxxx",
  "refresh_token": "1//xxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "roots": [
    { "id": "root", "name": "My Drive", "protect_file_link": false }
  ]
};
```

Also change the encryption keys (see [Encryption Keys](#encryption-keys)):
```js
const crypto_base_key = "YOUR_32_CHAR_HEX_KEY";
const hmac_base_key   = "YOUR_64_CHAR_HEX_KEY";
```

#### Step 3 — Deploy

**Option A — Dashboard (no CLI):**
1. Go to [dash.cloudflare.com](https://dash.cloudflare.com) → **Workers & Pages → Create Application → Create Worker**
2. Click **Edit Code**, paste the full contents of `src/worker.js`, click **Save and Deploy**

**Option B — CLI:**
```bash
npm install
npx wrangler deploy
```

---

### Method B: Service Account (Shared Drive)

Service accounts are ideal for Shared/Team Drives where you don't want to share an OAuth refresh token.

1. **Google Cloud Console → IAM & Admin → Service Accounts → Create Service Account**
2. Download the JSON key file
3. In Google Drive, **share the drive/folder** with the service account email (`xxx@project.iam.gserviceaccount.com`) as a **Viewer**
4. Paste the JSON content into `src/worker.js`:

```js
const serviceaccounts = [
  {
    "type": "service_account",
    "project_id": "your-project",
    "private_key_id": "abc123",
    "private_key": "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n",
    "client_email": "your-sa@your-project.iam.gserviceaccount.com",
    "client_id": "...",
    "token_uri": "https://oauth2.googleapis.com/token"
  }
];
```

5. Set `"service_account": true` in `authConfig`
6. Set the shared drive ID in `roots`:

```js
"roots": [
  { "id": "0AOM2i7Mi3uWIUk9PVA", "name": "Team Drive", "protect_file_link": false }
]
```

> **Multiple service accounts** — add multiple objects to `serviceaccounts[]`. One is picked at random per request to distribute API quota.

---

## Configuration Reference

### authConfig

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `siteName` | string | `"Google Drive Index"` | Page title in the browser tab and navbar |
| `client_id` | string | `""` | Google OAuth 2.0 client ID |
| `client_secret` | string | `""` | Google OAuth 2.0 client secret |
| `refresh_token` | string | `""` | OAuth refresh token for accessing the drive |
| `service_account` | bool | `false` | Set `true` to use Service Account auth instead of OAuth |
| `service_account_json` | object | auto | Don't touch — points to the randomly selected service account |
| `files_list_page_size` | number | `100` | Files fetched per API page (max 1000). Higher = fewer API calls but slower first load |
| `search_result_list_page_size` | number | `100` | Search results per API page |
| `enable_cors_file_down` | bool | `false` | Add `Access-Control-Allow-Origin: *` to file downloads |
| `enable_password_file_verify` | bool | `false` | Enable per-folder `.password` file protection |
| `direct_link_protection` | bool | `false` | Block direct download links used without a Referer from your domain |
| `disable_anonymous_download` | bool | `false` | Require login session for all file downloads |
| `file_link_expiry` | number | `7` | Days until a generated download link expires |
| `search_all_drives` | bool | `true` | When `true`, search spans all user drives; `false` restricts to current drive |
| `enable_login` | bool | `false` | Enable the username/password login system |
| `enable_signup` | bool | `false` | Reserved for future signup support |
| `enable_social_login` | bool | `false` | Show Google sign-in button on login page |
| `google_client_id_for_login` | string | `""` | OAuth client ID for Google login (different from drive access) |
| `google_client_secret_for_login` | string | `""` | OAuth client secret for Google login |
| `redirect_domain` | string | `""` | Your worker URL, e.g. `https://index.example.com` — required for Google OAuth login |
| `login_database` | string | `"Local"` | `"Local"` (in-config users) or `"KV"` (Cloudflare KV store) |
| `login_days` | number | `7` | Session duration in days |
| `enable_ip_lock` | bool | `false` | Bind download links to the requesting user's IP address |
| `single_session` | bool | `false` | Only allow one active session per user (uses KV) |
| `ip_changed_action` | bool | `false` | Log out the user if their IP address changes (uses KV) |
| `cors_domain` | string | `"*"` | `Access-Control-Allow-Origin` for API responses |
| `users_list` | array | `[{username, password}]` | Local user accounts (only used when `login_database: "Local"`) |
| `roots` | array | — | **Required.** List of drives/folders to index (see [Multiple Drives](#multiple-drives)) |

#### roots array

Each entry in `roots` describes one drive or folder:

| Key | Type | Description |
|-----|------|-------------|
| `id` | string | `"root"` for My Drive; a Shared Drive ID (from the URL); or a specific folder ID |
| `name` | string | Display name shown in the navbar dropdown and homepage grid |
| `protect_file_link` | bool | If `true` and login is enabled, requires authentication even for direct file downloads |
| `client_id` | string | *(optional)* Per-drive OAuth client ID — overrides global `client_id` |
| `client_secret` | string | *(optional)* Per-drive OAuth client secret — overrides global `client_secret` |
| `refresh_token` | string | *(optional)* Per-drive refresh token — overrides global `refresh_token` |
| `service_account` | bool | *(optional)* Set `true` to use per-drive service account auth |
| `service_account_json` | object | *(optional)* Service account JSON for this drive (instead of global `serviceaccounts[]`) |

If none of the per-drive credential keys are present, the drive falls back to the global `client_id`/`client_secret`/`refresh_token` or `serviceaccounts[]`. See [Per-Drive Credentials](#per-drive-credentials) for a full example.

---

### uiConfig

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `theme` | string | `"darkly"` | Bootstrap/Bootswatch theme name. Options: `darkly`, `flatly`, `slate`, `cyborg`, `journal`, `lumen`, `materia`, `minty`, `pulse`, `sandstone`, `simplex`, `sketchy`, `solar`, `spacelab`, `superhero`, `united`, `yeti`, `quartz`, `morph`, `vapor`, `zephyr` |
| `version` | string | `CDN_VERSION` | Set automatically by build script — used for npm CDN asset versioning |
| `debug_mode` | bool | `false` | Show a debug panel in the page footer with live API request logs, error traces, and page info. Useful during development; disable for production. |
| `logo_image` | bool | `true` | `true` = use an image URL for the logo; `false` = use plain text |
| `logo_height` | string | `""` | CSS height of the logo image (e.g. `"30px"`) |
| `logo_width` | string | `"100px"` | CSS width of the logo image |
| `favicon` | string | CDN URL | URL to the favicon |
| `logo_link_name` | string | CDN SVG URL | If `logo_image: true`, the image URL; if `false`, the text to show |
| `fixed_header` | bool | `true` | Keep the navbar fixed at the top while scrolling |
| `header_padding` | string | `"80"` | Top padding for page content (pixels). Use `80` with fixed header, `20` otherwise |
| `nav_link_1` | string | `"Home"` | Unused navigation label (reserved) |
| `nav_link_3` | string | `"Current Path"` | Unused navigation label (reserved) |
| `nav_link_4` | string | `"Contact"` | Unused navigation label (reserved) |
| `fixed_footer` | bool | `false` | Fix footer to the bottom of the viewport |
| `hide_footer` | bool | `true` | Completely hide the footer |
| `header_style_class` | string | `"navbar-dark bg-primary"` | Bootstrap classes for the navbar background |
| `footer_style_class` | string | `"bg-primary"` | Bootstrap classes for the footer background |
| `css_a_tag_color` | string | `"white"` | Link colour in the navbar |
| `css_p_tag_color` | string | `"white"` | Paragraph colour in the navbar |
| `folder_text_color` | string | `"white"` | Folder name colour in listings |
| `loading_spinner_class` | string | `"text-light"` | Bootstrap colour class for the loading spinner |
| `search_button_class` | string | `"btn btn-danger"` | Bootstrap classes for the search submit button |
| `path_nav_alert_class` | string | `"alert alert-primary"` | Bootstrap classes for the path alert box |
| `file_view_alert_class` | string | `"alert alert-danger"` | Bootstrap classes for file view alerts |
| `file_count_alert_class` | string | `"alert alert-secondary"` | Bootstrap classes for the item count bar |
| `contact_link` | string | Telegram URL | URL for the contact button in the navbar |
| `copyright_year` | number | auto | Current year — auto-detected, do not change |
| `company_name` | string | `"The Bay Index"` | Name shown in the footer copyright |
| `company_link` | string | Telegram URL | URL for the footer company name |
| `credit` | bool | `true` | Show "Redesigned by..." credit in footer |
| `display_size` | bool | `true` | Show file sizes in the listing |
| `display_time` | bool | `false` | Show file modification timestamps in the listing |
| `display_download` | bool | `true` | Show download icon next to each file in the listing |
| `disable_player` | bool | `false` | Disable all in-browser media players; files open/download directly |
| `disable_video_download` | bool | `false` | Hide download and copy buttons in the video player view |
| `allow_selecting_files` | bool | `true` | Enable checkboxes for bulk file selection / link copying |
| `second_domain_for_dl` | bool | `false` | Route all downloads through a secondary Worker domain (see [Load Balancing](#load-balancing-multiple-download-domains)) |
| `poster` | string | CDN URL | Default video poster/thumbnail image URL |
| `audioposter` | string | CDN URL | Default audio cover art image URL |
| `disable_audio_download` | bool | `false` | Hide the download button in the audio player |
| `jsdelivr_cdn_src` | string | jsDelivr URL | Base CDN URL for assets — change only if self-hosting |
| `render_head_md` | bool | `true` | Render `HEAD.md` as HTML above the file listing |
| `render_readme_md` | bool | `true` | Render `README.md` as HTML below the file listing |
| `unauthorized_owner_link` | string | Telegram URL | Link shown on unauthorised error pages |
| `unauthorized_owner_email` | string | abuse email | Email shown on unauthorised error pages |
| `downloaddomain` | string | auto | Set by `domain_for_dl` — do not change here |
| `show_logout_button` | bool | auto | Auto-set to `true` when `enable_login` is `true` |
| `show_quota` | bool | `false` | Show a storage usage bar below the nav. Fetches quota via `/{n}:quota` on page load. Disabled by default. |

---

### player_config

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `player` | string | `"videojs"` | Video player engine: `"videojs"`, `"plyr"`, `"dplayer"`, `"jwplayer"` |
| `videojs_version` | string | `"8.12.0"` | Video.js CDN version to load |
| `plyr_io_version` | string | `"3.7.8"` | Plyr CDN version to load |
| `jwplayer_version` | string | `"8.16.2"` | JWPlayer version (requires a valid JWPlayer license) |

---

## Multiple Drives

Add one entry per drive/folder to the `roots` array. Each gets its own numbered path (`/0:/`, `/1:/`, etc.) and appears as a tile on the homepage.

```js
"roots": [
  {
    "id": "root",
    "name": "Personal Drive",
    "protect_file_link": false
  },
  {
    "id": "0AOM2i7Mi3uWIUk9PVA",
    "name": "Team Shared Drive",
    "protect_file_link": false
  },
  {
    "id": "1A2B3C4D5E6F7G8H9I0J1K2L3M",
    "name": "Archive (Login Required)",
    "protect_file_link": true
  }
]
```

> **Finding a Drive ID:**
> - **My Drive:** Use `"root"`
> - **Shared Drive:** Open in Google Drive → the URL contains `.../drive/folders/DRIVE_ID`
> - **Specific Folder:** Open the folder → copy the ID from the URL `...folders/FOLDER_ID`
>
> **Note:** Folder IDs (not Shared Drive IDs) will not have search working correctly — Google's API only supports full-drive search, not folder-scoped search without a driveId.

---

## Per-Drive Credentials

Each entry in the `roots` array can have its own OAuth or service account credentials. When per-drive credentials are present, that drive uses them independently — no shared token, no quota collision.

If a drive does not have its own credentials, it falls back to the global `client_id` / `client_secret` / `refresh_token` (or the randomly selected `serviceaccounts[]` entry).

```js
"roots": [
  {
    "id": "root",
    "name": "Personal Drive",
    "protect_file_link": false
    // No per-drive creds → uses global OAuth credentials
  },
  {
    "id": "0ABCDEFGabcdefg",
    "name": "Company Shared Drive",
    "protect_file_link": false,
    // Per-drive OAuth credentials
    "client_id":     "company-client-id.apps.googleusercontent.com",
    "client_secret": "GOCSPX-company-secret",
    "refresh_token": "1//company-refresh-token"
  },
  {
    "id": "0XYZxyzXYZxyz",
    "name": "Archive (Service Account)",
    "protect_file_link": true,
    // Per-drive service account
    "service_account": true,
    "service_account_json": {
      "type": "service_account",
      "project_id": "archive-project",
      "private_key_id": "abc123",
      "private_key": "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n",
      "client_email": "archive-sa@archive-project.iam.gserviceaccount.com",
      "client_id": "...",
      "token_uri": "https://oauth2.googleapis.com/token"
    }
  }
]
```

**How it works internally:**
- Each `googleDrive` instance checks its own `root` config for credentials on every token refresh
- Tokens are cached per-drive instance with a 58-minute TTL (slightly under Google's 60-minute expiry)
- If per-drive creds are missing or incomplete, the global `getAccessToken()` is called as the fallback

---

## Cross-Drive ID Lookup

GDI can automatically find a file or folder by its raw Google Drive ID across all configured drives, without you needing to know which drive it belongs to.

### `GET /?driveid=GOOGLE_DRIVE_ID`

Searches all configured drives for the given ID and redirects to the file's path in the index.

```
https://your-index.workers.dev/?driveid=1PivBPUBk8Nz6kpQIuJFfa8VeiqQJHoxn
https://your-index.workers.dev/?driveid=1PivBPUBk8Nz6kpQIuJFfa8VeiqQJHoxn&view=true
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `driveid` | Yes | A raw (unencrypted) Google Drive file or folder ID |
| `view` | No | Set to `true` to open in viewer mode (`?a=view`) |

**Redirect flow:**
1. Tries `findPathById()` on each configured drive in order
2. If found → `302` redirect to `/{driveIndex}:/path/to/item`
3. If not in any drive hierarchy but credentials can access it → `302` redirect to `/fallback?id=...`
4. If not found anywhere → `404` JSON error

### `GET /findpath?id=GOOGLE_DRIVE_ID`

Same cross-drive search as above but designed for external integrations (used by other apps to resolve a raw Drive ID into a browseable URL).

```
https://your-index.workers.dev/findpath?id=1PivBPUBk8Nz6kpQIuJFfa8VeiqQJHoxn
https://your-index.workers.dev/findpath?id=1PivBPUBk8Nz6kpQIuJFfa8VeiqQJHoxn&view=true
```

Redirect behavior is identical to `/?driveid=` above.

### `GET /{driveIndex}:findpath?id=GOOGLE_DRIVE_ID`

Like `/findpath` but starts with a specific drive, then falls through to all others if not found.

```
https://your-index.workers.dev/1:findpath?id=1PivBPUBk8Nz6kpQIuJFfa8VeiqQJHoxn
```

Useful when you know the file is likely in drive 1 but want automatic fallback to other drives.

---

## Service Account Setup

Service accounts allow you to serve a Shared Drive without an OAuth refresh token. This is ideal for public indexes or team deployments.

### Step-by-step

1. **Google Cloud Console → IAM & Admin → Service Accounts → Create Service Account**
   - Give it a name and click through
2. Click the new service account → **Keys → Add Key → Create new key → JSON**
3. Download the `.json` file
4. **Share the Google Drive/folder** with the service account's email address (shown in the JSON as `client_email`) — grant it **Viewer** access
5. Paste the JSON into `serviceaccounts`:

```js
const serviceaccounts = [
  {
    "type": "service_account",
    "project_id": "my-project-123",
    "private_key_id": "abc123def456",
    "private_key": "-----BEGIN PRIVATE KEY-----\nMIIEvg...\n-----END PRIVATE KEY-----\n",
    "client_email": "gdi-reader@my-project-123.iam.gserviceaccount.com",
    "client_id": "123456789012345678901",
    "auth_uri": "https://accounts.google.com/o/oauth2/auth",
    "token_uri": "https://oauth2.googleapis.com/token"
  }
];
```

6. In `authConfig`, set:
```js
"service_account": true,
```

### Multiple Service Accounts (Load Balancing)

Add multiple service account objects to the array. One is selected randomly on each worker cold-start, distributing API quota across accounts:

```js
const serviceaccounts = [
  { /* service account 1 */ },
  { /* service account 2 */ },
  { /* service account 3 */ }
];
```

---

## Login & Authentication

Set `"enable_login": true` in `authConfig` to protect the entire index. Users must log in to browse files.

### Local Username/Password

The simplest setup — credentials stored directly in `worker.js`:

```js
"login_database": "Local",
"users_list": [
  { "username": "alice", "password": "securePassword1!" },
  { "username": "bob",   "password": "anotherPassword2@" }
]
```

> **Security:** Change these from the defaults before deploying. Consider using the KV database for production so credentials are not visible in source code.

### Google OAuth Login (Social)

Allow users to sign in with their Google account. Only Google accounts listed in `users_list` (by email) or present in KV are granted access.

#### Setup

1. In Google Cloud Console → **Credentials → Create OAuth 2.0 Client ID**
   - Application type: **Web application**
   - Authorised redirect URIs: `https://your-worker.workers.dev/google_callback`
2. Configure `authConfig`:

```js
"enable_social_login": true,
"google_client_id_for_login": "123456789-abc.apps.googleusercontent.com",
"google_client_secret_for_login": "GOCSPX-xxxxxxxxxxxxxxxxx",
"redirect_domain": "https://your-worker.workers.dev",
"login_database": "Local",
"users_list": [
  { "username": "alice@gmail.com", "password": "" }
]
```

When using Google login with the Local database, the `username` field must be the user's **full Google email address**. The `password` field is ignored for OAuth users.

### Cloudflare KV User Database

For dynamic user management without redeploying the worker, use Cloudflare KV.

#### Setup

1. In Cloudflare Dashboard → **Workers & Pages → KV → Create a namespace** (e.g. `GDI_USERS`)
2. In your `wrangler.toml`:

```toml
kv_namespaces = [
  { binding = "ENV", id = "your-kv-namespace-id" }
]
```

3. Set in `authConfig`:
```js
"login_database": "KV"
```

4. Add users via Cloudflare Dashboard → KV → your namespace → **Add entry**:
   - Key: `username` (or email for Google OAuth)
   - Value: `password` (plaintext — KV is encrypted at rest)

5. For single-session and IP-lock features, the same KV namespace is also used to store session tokens and IP addresses.

### Session Security Options

| Option | Config Key | Description |
|--------|-----------|-------------|
| Session duration | `login_days: 7` | How many days before the session cookie expires |
| Single session | `single_session: true` | Logging in on a second device invalidates the first session. Requires KV. |
| IP change logout | `ip_changed_action: true` | If the user's IP changes after login, they are automatically logged out. Requires KV. |
| IP-locked downloads | `enable_ip_lock: true` | Download links are bound to the IP that generated them. |

---

## Per-Folder Password Protection

You can password-protect any subfolder without enabling the full login system.

1. Set `"enable_password_file_verify": true` in `authConfig`
2. Create a file named `.password` inside the Google Drive folder you want to protect
3. Set the file contents to your desired password (plain text, one password per file)

The `.password` file is never shown in the listing and its contents are never exposed to the browser. Users are prompted to enter the password when they first open the folder — it is cached in `localStorage` for convenience.

> **Note:** This feature is currently in preview. It protects the listing but not direct download links if `protect_file_link` is `false`.

---

## Encryption Keys

GDI encrypts session cookies and download links using AES-CBC + HMAC-SHA256. The default keys in the source code are **public and must be changed before deploying**.

**Generate random keys:**

```bash
# 32-byte AES key (64 hex characters)
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"

# 64-byte HMAC key (128 hex characters)  
node -e "console.log(require('crypto').randomBytes(64).toString('hex'))"
```

Then set them in `src/worker.js`:

```js
const crypto_base_key = "a1b2c3d4e5f6...";  // 64 hex chars
const hmac_base_key   = "f6e5d4c3b2a1...";  // 128 hex chars
```

Or generate them directly in the browser console:

```js
// AES key
crypto.getRandomValues(new Uint8Array(32)).reduce((h,b)=>h+b.toString(16).padStart(2,'0'),'')

// HMAC key
crypto.getRandomValues(new Uint8Array(64)).reduce((h,b)=>h+b.toString(16).padStart(2,'0'),'')
```

> **Legacy links:** GDI v2.4.0+ uses a random IV prepended to each ciphertext. A static fallback IV (`legacy_encrypt_iv`) is kept for backward-compatible decryption of links generated before v2.4.0. Do not change `legacy_encrypt_iv`.

---

## Download URL Protection

All download links go through `/download.aspx` with the following protections:

- **Encrypted file ID** — the real Google Drive file ID is never exposed
- **Expiring links** — links expire after `file_link_expiry` days (default: 7)
- **HMAC integrity** — prevents link tampering (changing the file ID or expiry)
- **IP lock** (optional) — the link is only valid from the IP that generated it
- **Login required** (optional) — `disable_anonymous_download: true` blocks all downloads without a session

### How a download link works

```
/download.aspx
  ?file=<AES-encrypted file ID>
  &expiry=<AES-encrypted Unix ms timestamp>
  &mac=<HMAC-SHA256 of "fileId|expiry" or "fileId|expiry|ip">
  [&ip=<AES-encrypted IP>]        ← only when enable_ip_lock is true
  [&inline=true]                   ← serve inline instead of as attachment
```

The worker decrypts the file ID, verifies the MAC, checks the expiry, then streams from Google Drive's API directly to the client.

---

## Region & ASN Blocking

Block access from specific countries or autonomous systems (data centres, VPNs, etc.).

```js
// Block by country code (ISO 3166-1 alpha-2)
const blocked_region = ['CN', 'RU', 'KP'];

// Block by ASN number — see bgplookingglass.com for ASN lists
const blocked_asn = [16509, 14618];  // Example: AWS ASNs
```

Blocked visitors receive a plain "Access Denied" HTML response (status 403).

---

## Load Balancing (Multiple Download Domains)

You can distribute download traffic across multiple Cloudflare Worker deployments.

```js
const domains_for_dl = [
  'https://dl1.yourworker.workers.dev',
  'https://dl2.yourworker.workers.dev',
  'https://dl3.yourworker.workers.dev'
];
```

In `uiConfig`:
```js
"second_domain_for_dl": true
```

Each domain must be a separate Cloudflare Worker deployment of `worker.js` (can be the same code). One is picked randomly per request.

> **Warning:** When `second_domain_for_dl` is `true`, the worker serves `disable_download.html` for direct download requests. Downloads are served exclusively through the secondary domain list.

---

## Themes

GDI uses [Bootswatch](https://bootswatch.com) themes on top of Bootstrap 5. Set the `theme` key in `uiConfig`:

```js
"theme": "darkly"   // dark theme (default)
"theme": "flatly"   // clean light theme
"theme": "cyborg"   // high-contrast dark
"theme": "vapor"    // neon dark
"theme": "quartz"   // glassmorphism light
```

The user can also toggle dark/light mode manually via the moon/sun icon in the navbar. Their preference is saved in `localStorage`.

**Available themes:**
`cerulean` · `cosmo` · `cyborg` · `darkly` · `flatly` · `journal` · `litera` · `lumen` · `lux` · `materia` · `minty` · `morph` · `pulse` · `quartz` · `sandstone` · `simplex` · `sketchy` · `slate` · `solar` · `spacelab` · `superhero` · `united` · `vapor` · `yeti` · `zephyr`

---

## Media Players

### Video

Configure the player engine in `player_config`:

```js
const player_config = {
  "player": "videojs"   // "videojs" | "plyr" | "dplayer" | "jwplayer"
};
```

| Player | HLS support | Keyboard shortcuts | Notes |
|--------|------------|-------------------|-------|
| `videojs` | Yes | Space/F/M/←/→/↑/↓ | Default. Best all-round |
| `plyr` | Yes (with plugin) | Yes | Minimal, beautiful UI |
| `dplayer` | Yes | Yes | Danmu/comment support |
| `jwplayer` | Yes | Yes | Commercial licence required |

**Supported formats:** MP4, WebM, AVI, MKV, MOV, FLV, TS, 3GP, M4V, RMVB, and more.

To disable all players and serve files directly: `"disable_player": true`

### Audio

Uses **APlayer** (auto-loaded). When opening a single audio file, GDI automatically fetches the folder's other audio files and builds a playlist.

**Supported formats:** MP3, FLAC, WAV, OGG, M4A, AAC, WMA, ALAC

To hide the download button on audio: `"disable_audio_download": true`

### PDF

Uses **PDF.js** (auto-loaded). Features:
- Page-by-page navigation with Previous/Next buttons
- Zoom slider (50%–200%)
- Full download button

### Images

Direct `<img>` display with lazy loading. Supported: JPG, JPEG, PNG, GIF, BMP, SVG, TIFF, ICO.

### Code / Text

In-browser code display for files up to **2 MB**. Supported: PHP, CSS, Go, Java, JS, JSON, TXT, SH, MD, HTML, XML, Python, Ruby, C, C++, H, HPP.

---

## Search

Type in the search box to search across your drives.

- **Current drive only:** Set `"search_all_drives": false` in `authConfig`
- **All drives:** Set `"search_all_drives": true` (default)
- Results show in an infinite-scroll list with folder/file icons
- Clicking a folder or file in search results opens a modal with a direct link to the item's path (resolved via the Google Drive API)
- If path resolution fails, a `/fallback?id=...` link is shown which always works

**Search limitations:**
- Folder IDs (as opposed to Shared Drive IDs) do not support drive-scoped search via the Google API — search will fall back to user/all drives
- Special characters `" ' = < > / \ :` are stripped from queries

---

## Deployment

### Via Cloudflare Dashboard (No CLI)

1. Go to [dash.cloudflare.com](https://dash.cloudflare.com)
2. **Workers & Pages → Create Application → Create Worker**
3. Click **Edit Code** (or paste via the online editor)
4. Paste the full contents of `src/worker.js`
5. Click **Save and Deploy**
6. Visit your worker URL (e.g. `https://your-worker.your-subdomain.workers.dev`)

### Via Wrangler CLI

```bash
# Install dependencies
npm install

# Login to Cloudflare
npx wrangler login

# Deploy
npm run deploy
# or
npx wrangler deploy src/worker.js --name my-drive-index --compatibility-date 2024-01-01
```

**Custom domain:**

In Cloudflare Dashboard → **Workers & Pages → your worker → Custom Domains → Add Custom Domain** — point any domain/subdomain in your Cloudflare DNS.

### Environment: `production` vs `development` vs `local`

```js
const environment = 'production';   // Loads assets from jsDelivr CDN
// const environment = 'development'; // Loads from /src/app.min.js (local wrangler dev)
// const environment = 'local';       // Loads from http://127.0.0.1:5500/src/app.js (live reload)
```

---

## Development Setup

### Prerequisites

```bash
node --version   # 18+
npm --version    # 8+
```

### Install and Run

```bash
git clone https://gitlab.com/GoogleDriveIndex/Google-Drive-Index.git
cd Google-Drive-Index
npm install

# Start local dev server (hot-reload for worker, static for assets)
npm run dev
```

The dev server runs on `http://localhost:8787` by default.

### Code Quality

```bash
npm run lint        # Check with ESLint
npm run lint:fix    # Auto-fix lint errors
npm run format      # Format with Prettier
npm run typecheck   # TypeScript type-check (no emit)
```

### Local Asset Development

For faster frontend iteration, set `environment = 'local'` in `worker.js` and open `src/app.js` with a live-reload server (e.g. VS Code Live Server on port 5500).

---

## Build Process (CDN Assets)

GDI's frontend assets (CSS, JS, images) are bundled inside the npm package and served from [jsDelivr](https://www.jsdelivr.com/) via the npm CDN:

```
https://cdn.jsdelivr.net/npm/@googledrive/index@{version}/
```

All static files (`src/app.min.js`, `assets/gdi.min.css`, `assets/homepage.min.js`, `images/`, `sw.js`) live in this repo and are published to npm with each release. No separate CDN repository is needed.

To build and release:

```bash
npm run build    # Patch CDN_VERSION, minify JS/CSS, output built files into repo
git add src/app.min.js assets/gdi.min.css assets/homepage.min.js sw.js
git commit -m "Release v2.5.7"
git tag v2.5.7 && git push && git push --tags
npm publish --access public
```

**What `npm run build` does:**
1. Reads version from `package.json` and patches `CDN_VERSION` in `src/worker.js` and `GDI_VERSION` in `generator/worker.js`
2. Minifies `src/app.js` → `src/app.min.js` using esbuild
3. Minifies `assets/homepage.js` → `assets/homepage.min.js`
4. Minifies `assets/gdi.css` → `assets/gdi.min.css`

After publishing, jsDelivr picks up the new version within minutes — no tag propagation delay.

---

## API Reference

These are the internal POST/GET API endpoints the frontend uses. All paths are relative to your worker URL.

### `POST /{driveIndex}:/path/to/folder/`
Fetch file listing for a directory.

**Request body (JSON):**
```json
{
  "id": "",
  "type": "folder",
  "password": "",
  "page_token": "",
  "page_index": 0
}
```

**Response (JSON):**
```json
{
  "nextPageToken": "token_or_null",
  "curPageIndex": 0,
  "data": {
    "files": [
      {
        "id": "<encrypted>",
        "name": "file.mp4",
        "mimeType": "video/mp4",
        "size": "104857600",
        "modifiedTime": "2024-01-15T10:30:00.000Z",
        "fileExtension": "mp4",
        "driveId": "<encrypted>",
        "link": "/download.aspx?file=...&expiry=...&mac=..."
      }
    ]
  }
}
```

### `POST /{driveIndex}:search`
Full-text search.

**Request body (JSON):**
```json
{ "q": "search terms", "page_token": null, "page_index": 0 }
```

**Response:** Same structure as file listing.

### `POST /{driveIndex}:id2path`
Resolve an encrypted file/folder ID to its path.

**Request body (JSON):**
```json
{ "id": "<encrypted file ID>" }
```

**Response (JSON):**
```json
{ "path": "/0:/folder/subfolder/file.mp4" }
```

### `GET /download.aspx`
Stream or download a file. For Google Workspace files, uses the Drive export API automatically.

| Parameter | Description |
|-----------|-------------|
| `file` | AES-encrypted file ID |
| `expiry` | AES-encrypted Unix ms expiry timestamp |
| `mac` | HMAC-SHA256 integrity token |
| `ip` | AES-encrypted IP (only when `enable_ip_lock: true`) |
| `inline` | `"true"` to serve inline instead of as attachment |
| `fmt` | Export format extension for Google Workspace files: `pdf`, `docx`, `txt` (Docs), `xlsx`, `csv` (Sheets), `pptx` (Slides). Defaults to `pdf` if omitted. |

### `GET /{driveIndex}:quota`
Returns storage quota for the drive's Google account. Requires `show_quota: true` in `uiConfig` to be surfaced in the UI (but the endpoint is always available).

**Response (JSON):**
```json
{
  "user": { "displayName": "...", "emailAddress": "..." },
  "storageQuota": {
    "limit": "16106127360",
    "usage": "4831838208",
    "usageInDrive": "1234567890",
    "usageInDriveTrash": "0"
  }
}
```

### `POST /login`
Authenticate with username/password.

**Request body:** `application/x-www-form-urlencoded` with `username` and `password`.

**Response (JSON):**
```json
{ "ok": true, "redirect": "/" }
// or
{ "ok": false, "message": "Invalid username or password." }
```

### `GET /logout`
Clears the session cookie and redirects to `/login`.

### `GET /google_callback`
OAuth callback handler. Exchanges the Google authorization code for an ID token, validates the email against the user list, and sets a session cookie.

### `GET /findpath`
Cross-drive file/folder lookup by raw Google Drive ID. Searches all configured drives and redirects to the resolved path.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `id` | Yes | Raw (unencrypted) Google Drive file or folder ID |
| `view` | No | `"true"` to redirect to viewer mode |

**Response:** `302` redirect to `/{driveIndex}:/path` (found in drive), `302` to `/fallback?id=...` (accessible via credentials but not in any drive root), or `404` JSON if not found anywhere.

### `GET /{driveIndex}:findpath`
Same as `/findpath` but starts the search from a specific drive index. Falls through to all other drives if not found in the specified one.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `id` | Yes | Raw (unencrypted) Google Drive file or folder ID |
| `view` | No | `"true"` to redirect to viewer mode |

### `GET /?driveid=`
Cross-drive lookup starting from the homepage. See [Cross-Drive ID Lookup](#cross-drive-id-lookup).

### `POST /{driveIndex}:fallback`
Resolve an encrypted file/folder ID from an external source (used by the fallback page to show files/folders that are accessible via credentials but are not rooted in any configured drive).

**Request body (JSON):**
```json
{ "id": "<encrypted ID>", "type": "folder", "page_token": null, "page_index": 0 }
```
- Omit `"type"` (or set to any non-`"folder"` value) to look up a single file
- Include `"type": "folder"` to list a folder's contents

**Response:** File metadata JSON (single file) or listing JSON (folder).

### `GET /{driveIndex}:findpath` → `POST /{driveIndex}:id2path`
Internal pair used by the search result modal to resolve encrypted IDs to paths.

`id2path` takes `{ "id": "<encrypted>" }` and returns `{ "path": "/0:/folder/file.mp4" }` or `{ "path": null }` (404) if not resolvable.

### `POST /copy`
Copy a file within Google Drive.

**Request body:** `application/x-www-form-urlencoded`

| Field | Description |
|-------|-------------|
| `id` | Encrypted file ID (as returned by the listing API) |
| `root_id` | Raw Google Drive folder ID where the copy should be placed |

**Response (JSON):** The created file metadata from the Google Drive API, or an error object.

### `GET /sw.js`
Serves the service worker script for offline support (fetched from CDN). Non-critical — browsing continues normally if this fails.

---

## Troubleshooting / FAQ

### Getting a Refresh Token Manually

If the generator tool is unavailable, get a refresh token manually:

1. In Google Cloud Console, create an OAuth 2.0 Web App credential
2. Set the redirect URI to `https://developers.google.com/oauthplayground`
3. Go to [OAuth Playground](https://developers.google.com/oauthplayground)
4. Click the settings gear → check "Use your own OAuth credentials" → enter your client ID and secret
5. In Step 1, select `https://www.googleapis.com/auth/drive` → Authorize
6. In Step 2, click "Exchange authorization code for tokens"
7. Copy the `refresh_token` value

### "Invalid Request!" on downloads

The download link has either expired, been tampered with, or your IP has changed (if `enable_ip_lock` is enabled). Generate a new link by refreshing the file listing page.

### Files not showing up

- Verify the drive ID is correct and the account/service account has been granted access
- Google Docs, Sheets, and Slides now appear in the listing and can be exported to PDF/DOCX/XLSX/PPTX/TXT/CSV. Google Forms and Sites are still excluded (cannot be exported via the Drive API).
- Check if `.password` file protection is enabled and a password is set on the folder

### Search not working with a Folder ID

If you set a folder ID (not a Shared Drive ID) in `roots`, the Google Drive API does not support drive-scoped search for regular folders. Use `"search_all_drives": true` or set a proper Shared Drive ID.

### Login page keeps looping

- Ensure your encryption keys are set correctly (not the default public keys)
- If using KV, ensure the `ENV` KV binding is created and bound in `wrangler.toml`
- Check that `redirect_domain` does not have a trailing slash

### "User Logged in Someplace Else" error

`single_session: true` is enabled and another device/browser has logged in. Log out everywhere and sign in again on one device only.

### Worker exceeds CPU time limit

- The free tier allows 10ms CPU per request. Most requests are I/O-bound (waiting on Google API) so this is rarely hit
- If it occurs, consider upgrading to Cloudflare Workers Paid plan (50ms CPU limit) or optimising `files_list_page_size`

### Console shows "Report this page when asked..."

This is the global error handler. Copy the full error message and open an issue on the repository with:
1. The error text
2. Your `authConfig` (with credentials removed)
3. The request that caused the error

### CORS errors on downloads

Set `"enable_cors_file_down": true` in `authConfig` to add `Access-Control-Allow-Origin: *` to all download responses.

### Service worker registration fails

This is a non-critical error. The `/sw.js` endpoint fetches a service worker from the CDN for offline support. If the CDN is unreachable, the error is caught silently and browsing continues normally.

---

## Planned Features

The following features are under consideration for future releases:

- [ ] **Rate limiting** on the login endpoint to prevent brute-force attacks
- [ ] **Bulk download as ZIP** — select multiple files and download a ZIP archive
- [ ] **File upload** — upload files to Google Drive from the web UI (requires write permission)
- [ ] **Thumbnail/grid view** — image gallery mode for photo folders
- [ ] **Subtitle support** — auto-detect `.srt`/`.vtt` files for video player subtitles
- [ ] **Admin panel** — manage KV users, view access logs
- [ ] **Analytics** — optional lightweight visit/download counters
- [ ] **Custom folder sorting** — pin folders to top, custom order via metadata files
- [ ] **Webhook on download** — notify a URL when a file is downloaded
- [ ] **Password reset flow** — for KV-based users, email-based password reset
- [ ] **Two-factor authentication** — TOTP/HOTP for the login system
- [ ] **Embed mode** — `?embed=1` already supported; iframe-friendly minimal UI
- [ ] **MongoDB user database** — `login_database: "MongoDB"` placeholder already in code

---

## Changelog

### v2.5.9 (Current)

**Bug fixes:**
- Fixed: File and folder names truncated with ellipsis on mobile — names now wrap fully in both index and search results on screens ≤768px.

---

### v2.5.8

**Bug fixes:**
- Fixed: File size not shown on mobile in search results — added `data-size` attribute to the search result row-name element so the CSS pseudo-element rule can render it below the filename at ≤560px viewport width.

---

### v2.5.7

**Bug fixes:**
- Fixed: Mobile search bar was hidden at ≤420px viewport width — removed `display:none` on `.gdi-nav-search`, replaced with narrowed `max-width` and hidden separator instead.
- Fixed: Breadcrumb showed raw URL segment (e.g. `2:`) instead of the configured drive name — now maps through `window.drive_names[]` in both `generateBreadcrumb()` and `list()`.
- Fixed: File viewer crash ("Cannot set properties of null") when `requestListPath()` was called from a file viewer page that has no `#update` / `#list` elements — switched to jQuery `$('#id').html()` which silently no-ops on missing elements.
- Fixed: "More options" dropdown in the file viewer was clipped behind the viewer card — removed `overflow: hidden` from `.gdi-viewer-card` and `.gdi-btn-split`, added border-radius to first/last children instead.

**UI:**
- Sticky footer — debug bar and footer now pin to the bottom of the viewport on short pages and naturally follow content on long pages (`body { display:flex; flex-direction:column }` + `#content { flex:1 }`).

---

### v2.5.6

**Bug fixes:**
- Fixed: Search result click for files in unconfigured shared drives (`rootIdx = -2`) navigated to `/fallback?id=...` but failed with 400 — `getQueryVariable` returned the URL-encoded ID (`%2B` instead of `+`), causing `decryptString` to fail. Fixed with `decodeURIComponent`.

**CDN / release:**
- CDN migrated from separate GitHub CDN repo to npm package (`@googledrive/index`) — assets now served via `cdn.jsdelivr.net/npm/@googledrive/index@{version}/`. No separate CDN repository needed.
- `CDN_VERSION` replaces `CDN_SHA` — single version string drives all CDN URLs; auto-patched by `npm run build` from `package.json`.
- Added `files` field to `package.json` so only deployable files are included in the npm package.
- `npm run build` now patches version strings, minifies CSS/JS, and prepares the repo for `npm publish`.

---

### v2.5.5

**New features:**
- New: **Google Workspace export** — Google Docs, Sheets, and Slides now appear in file listings and can be exported via `/download.aspx?fmt=<ext>`. Supported formats: PDF, DOCX, TXT (Docs); PDF, XLSX, CSV (Sheets); PDF, PPTX (Slides). Google Forms and Sites remain excluded.
- New: **Storage quota display** — `show_quota: true` in `uiConfig` shows a usage bar below the nav (green/orange/red). Backed by a new `GET /{n}:quota` endpoint (`about.get`). Disabled by default.
- New: **Auto-discover Shared Drives** in the generator — paste a temporary access token, click "Fetch Drives", and check the drives you want to add. Supports "Select All" and closes the panel automatically after adding.
- New: `GET /{n}:quota` — returns `storageQuota` and `user` from the Drive `about.get` API for the drive's account
- New: `GET /findpath?id=` — cross-drive file/folder lookup; searches all configured drives in order, falls back to `/fallback` if accessible via credentials but not in any drive root, returns 404 if not found anywhere
- New: `GET /?driveid=DRIVE_ID` — same cross-drive lookup from the homepage; supports `&view=true`
- New: `GET /{n}:findpath?id=` — per-drive findpath that falls through to all other drives automatically
- New: Per-drive credentials — each `roots[]` entry can have its own `client_id`/`client_secret`/`refresh_token` or `service_account`/`service_account_json`; falls back to global credentials if not set
- New: `POST /copy` — copy a Google Drive file to a specified folder via the worker API

**Bug fixes:**
- Fixed: `GET /findpath` previously hard-redirected to `/0:findpath`, meaning it only ever searched drive 0
- Fixed: `findId2Path` (used by `/{n}:findpath`) only tried one drive and never fell back to others
- Fixed: `/?driveid=` redirected to `/fallback` even for IDs that don't exist anywhere (now returns 404)
- Fixed: OAuth error redirect pointed to `/?error=Invalid Token` instead of `/login?error=Invalid+Token`
- Fixed: `handleSearch` responses were missing `Content-Type: application/json` header
- Fixed: `handleId2Path` returned `{"path":"/undefined:undefined"}` when path was not resolvable
- Fixed: `_list_gdrive_files` returned `null` when parent ID was undefined, causing `TypeError` in callers
- Fixed: Google API error responses (no `files` array) caused crashes in listing code
- Fixed: Fallback URL had unencoded base64 characters (`+`, `=`, `/`) in the `id` query param
- Fixed: Fallback file response was missing `Content-Type` header
- Fixed: `/3:/{nonexistent}` (and similar) API returned 500 instead of 404 when file not found
- Fixed: Breadcrumb incorrectly truncated folder names containing `?` (e.g. "My Folder? Yes" became "My Folder")
- Fixed: `findItemById` conditionally omitted `&supportsAllDrives=true` on user drives, causing lookup failures for shared items
- Fixed: `redirectToIndexPage()` used HTTP 307 (method-preserving); redirected POST `/login` re-sent the form body to `/0:/`, causing a 500 error when login is disabled — changed to 302
- Fixed: `handleId2Path` returned 500 for requests with invalid base64 in the encrypted ID (now returns 400 "Invalid encrypted ID")
- Fixed: `findParentFilesRecursion` compared parent IDs against the string `"root"` instead of the real Google Drive root folder ID (`target_top_id`), causing `POST /{n}:id2path` to always return `{ "path": null }` for files in personal drives
- Fixed: `fetchAccessToken` did not check `response.ok` — non-2xx token responses were silently swallowed; now throws with the HTTP status
- Fixed: `get_single_file_api`, `searchFilesinDrive`, `findItemById`, `_findDirId` all lacked `response.ok` checks — API errors could cause crashes or silent empty results; all now return safe fallback values on non-2xx
- Fixed: `get_single_file` called `download(file.id)` without checking if `file` or `file.id` was null first; now returns 404 if the file is not found
- Fixed: `console.log` of raw file ID and download path left in production code path; removed

### v2.4.1
- Fixed: `sleep()` in download retry loop was not `await`-ed (no actual delay between retries)
- Fixed: POST API requests bypassed session authentication when `enable_login: true`
- Fixed: `kv_key` was `undefined` in Google OAuth callback for local-database users (broken session creation → login loop)
- Fixed: `params.get("q")` crash when navigating to `/search` with no `q` query parameter
- Fixed: `path.slice(3)` stripped wrong number of characters for drive indexes > 9 (drives 10+)
- Fixed: `details.parents[0] = null` threw if `parents` was undefined or empty
- Fixed: `Access-Control-Allow-Credentials` header set to boolean `true` instead of string `"true"`
- Fixed: Logout redirected to `/?error=...` instead of `/login` when login is enabled
- Fixed: Session cookie not cleared with `Max-Age=0` on logout; path was missing
- Fixed: `var user_found` scoping — replaced all with `let user_found = false` to prevent undefined reference
- Fixed: Frontend `sleep()` was a CPU-blocking busy-wait loop (froze browser UI during retry)
- Fixed: `performRequest()` in `requestListPath` never decremented the retry counter (potential infinite retry loop)
- Fixed: `requestSearch()` retry had the same infinite-retry bug
- Fixed: `data-bytes` attribute in fallback list and search results was `NaN` after `formatFileSize()` conversion (broke column sorting by size)
- Fixed: `id2path` fetch sent wrong `Content-Type: application/x-www-form-urlencoded` header for a JSON body
- Fixed: `MutationObserver` on `documentElement` added a new click listener to select-all checkbox on every DOM mutation (memory leak + multiple listener bug)
- Fixed: `fallback = true` used as function argument (assignment expression, not a value) — replaced with literal `true`
- Fixed: `formatFileSize()` returned `''` for 0-byte files; now returns `"0 bytes"`
- Improved: Session validation now returns proper JSON 401 responses for POST API requests (instead of HTML login page)

### v2.4.0
- New: Random IV per encryption operation (AES-CBC) for session cookies and download links
- New: Legacy static IV kept as fallback for pre-v2.4.0 links
- New: Redesigned login page with password toggle and Google OAuth button
- New: `gdi.css` v2.5.0 design system with CSS custom properties for theming
- New: Homepage grid with search bar
- New: Toast notifications for clipboard copy
- New: PDF.js viewer with zoom/navigation
- New: APlayer audio player with auto-playlist detection
- Improved: Retry logic with exponential back-off across all API calls

### v2.3.x
- Multiple drives with homepage grid
- Bootstrap 5 migration
- Dark/light theme toggle
- Bulk file selection

---

## Credits

- **Original concept:** [maple3142/GDIndex](https://github.com/maple3142/GDIndex) and [yanzai/goindex](https://github.com/yanzai/goindex)
- **Author / Maintainer:** [Parveen Bhadoo](https://parveenbhadoo.com) — [@PBhadoo](https://gitlab.com/PBhadoo)
- **UI Redesign (v2.5.0):** [TheFirstSpeedster](https://www.npmjs.com/package/@googledrive/index)
- **UI Framework:** [Bootstrap 5](https://getbootstrap.com) + [Bootswatch](https://bootswatch.com) + [Bootstrap Icons](https://icons.getbootstrap.com)
- **API:** [Google Drive API v3](https://developers.google.com/drive/api/v3/reference)
- **Video:** [Video.js](https://videojs.com), [Plyr](https://plyr.io), [DPlayer](https://dplayer.diygod.dev)
- **Audio:** [APlayer](https://aplayer.js.org)
- **PDF:** [PDF.js](https://mozilla.github.io/pdf.js/) by Mozilla
- **Markdown:** [Marked.js](https://marked.js.org)
- **CDN:** [jsDelivr](https://www.jsdelivr.com)
- **Hosting:** [Cloudflare Workers](https://workers.cloudflare.com)

---

## Sponsors

<a href="https://www.browserstack.com"><img src="https://i.imgur.com/UMYceGo.png" alt="BrowserStack" width="240"></a>
&nbsp;&nbsp;
<a href="https://tuta.com"><img src="https://gitlab.com/GoogleDriveIndex/Google-Drive-Index/-/raw/master/images/tuta-logo.png" alt="Tuta Mail" width="200"></a>
&nbsp;&nbsp;
<a href="https://1password.com"><img src="https://gitlab.com/GoogleDriveIndex/Google-Drive-Index/-/raw/master/images/1password.png" alt="1Password" width="200"></a>

Support the project:

[![Buy Me a Coffee](https://cdn.buymeacoffee.com/buttons/v2/default-white.png)](https://www.buymeacoffee.com/bhadoo)

---

## License

[MIT License](LICENSE) — Copyright (c) Parveen Bhadoo
