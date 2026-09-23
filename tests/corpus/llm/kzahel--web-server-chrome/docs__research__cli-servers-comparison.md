# CLI Static File Server Landscape

Research on what the competition does. These are the tools people actually use
today when they need to serve files from a directory.

---

## The Big Players

### miniserve (Rust) - 7.4k stars
The closest Rust competitor. Single binary, feature-rich, good UX.

**Standout features:**
- QR code for mobile access (`--qrcode`)
- 4 color themes with dark mode and theme selector
- README.md rendering inline in directory listings
- Random route for URL obscurity (`--random-route`)
- Archive downloads: tar, tar.gz, zip of any folder
- File upload, delete, mkdir from web UI
- SHA256/SHA512 password hashing for auth
- Read-only WebDAV (PROPFIND)
- Route prefix (`--route-prefix`)
- wget command shown in footer (`--show-wget-footer`)
- Duplicate file policy: error/overwrite/rename
- Env var config (`MINISERVE_PORT=3000` etc.)
- Shell completions and man page generation

**All flags:**
`-p/--port`, `-i/--interfaces`, `-v/--verbose`, `--log-color`, `--index`,
`--spa`, `--pretty-urls`, `--route-prefix`, `--random-route`, `-a/--auth`,
`--auth-file`, `-u/--upload-files`, `-U/--mkdir`, `-R/--rm-files`,
`--web-upload-files-concurrency`, `-o/--on-duplicate-files`, `-P/--no-symlinks`,
`-H/--hidden`, `-l/--show-symlink-info`, `-m/--media-type`, `-M/--raw-media-type`,
`-S/--default-sorting-method`, `-O/--default-sorting-order`, `-D/--dirs-first`,
`-t/--title`, `-I/--disable-indexing`, `-F/--hide-version-footer`,
`-W/--show-wget-footer`, `-c/--color-scheme`, `-d/--color-scheme-dark`,
`--hide-theme-selector`, `-r/--enable-tar`, `-g/--enable-tar-gz`, `-z/--enable-zip`,
`-C/--compress-response`, `--header`, `--tls-cert`, `--tls-key`,
`--enable-webdav`, `-q/--qrcode`, `--readme`, `--print-completions`, `--print-manpage`

### http-server (Node) - 14.2k stars
The classic. Most stars, most known. Has proxy support.

**Standout features:**
- **Full reverse proxy** (`--proxy`, `--proxy-all`, `--proxy-config`) - the killer feature
- Pre-compressed file serving (.gz/.br)
- Auto-open browser (`-o`)
- robots.txt generation (`--robots`)
- Connection timeout control
- Custom MIME type files
- Private Network Access CORS header (Chrome LAN feature)
- Cache-Control with `-c` seconds (or `-c-1` to disable)

**All flags:**
`-p/--port`, `-a` (address), `--base-dir`, `-d` (dir listing), `-i` (autoindex),
`-g/--gzip`, `-b/--brotli`, `-e/--ext`, `-s/--silent`, `--coop`, `--cors`,
`--private-network-access`, `-H/--header`, `-o` (open browser), `-c` (cache),
`-t` (timeout), `-T/--title`, `-U/--utc`, `--log-ip`, `-P/--proxy`,
`--proxy-options`, `--proxy-config`, `--proxy-all`, `--user`, `--password`,
`-S/--tls`, `-C/--cert`, `-K/--key`, `-r/--robots`, `--no-dotfiles`,
`--hide-permissions`, `--mimetypes`, `--no-panic`

### serve (Vercel/Node) - 9.8k stars, 942k dependents
Most widely used. Config-file driven.

**Standout features:**
- Powerful rewrite/redirect engine via `serve.json`
- Clean URLs (strip .html)
- Per-path custom headers via config
- Clipboard copy of URL on startup
- Auto port-switching when port is taken
- UNIX domain socket and Windows named pipe support
- Compression on by default

**All flags:**
`-l/--listen`, `-s/--single` (SPA), `-d/--debug`, `-c/--config`,
`-n/--no-clipboard`, `-u/--no-compression`, `--no-etag`, `-S/--symlinks`,
`-C/--cors`, `--no-port-switching`, `--ssl-cert`, `--ssl-key`, `--ssl-pass`,
`-L/--no-request-logging`

**serve.json config:** `public`, `cleanUrls`, `rewrites`, `redirects`, `headers`,
`directoryListing`, `unlisted`, `trailingSlash`, `renderSingle`, `symlinks`, `etag`

### dufs (Rust) - 9.7k stars
More of a file management system than a simple server. Very impressive.

**Standout features:**
- **Full WebDAV** (MKCOL, MOVE, DELETE, PUT, PROPFIND)
- **File search** via web UI and API
- **Resumable uploads AND downloads**
- **Per-path, per-user access control** (`user:pass@/path:rw`)
- Digest auth (not just Basic)
- SHA256 file hashing via API
- File move/rename via HTTP MOVE
- JSON and simple-text listing output (`?json`, `?simple`)
- Health check endpoint (`/__dufs__/health`)
- Custom UI theming via assets directory
- YAML config file
- Env var config (`DUFS_PORT=5000` etc.)
- ZIP archive downloads of any folder
- Granular permission flags (upload/delete/search/archive/hash separately)

### simple-http-server (Rust) - 3.4k stars
Nginx-style directory listing. Some unique features.

**Standout features:**
- Per-extension compression control (`-c=js,d.ts`)
- CSRF token protection for uploads
- Worker thread count config
- Upload size limit
- HTTP 301 redirect mode (redirect everything)
- COEP/COOP headers for cross-origin isolation
- PKCS#12 certificate format

### sfz (Rust) - 396 stars, ARCHIVED
Minimalist. Notable for one unique feature.

**Standout features:**
- **Respects .gitignore by default** - only server that does this
- Cross-Origin Isolation in one flag (`--coi`)
- Defaults to 127.0.0.1 (secure default)
- Brotli/gzip/deflate on by default

### anywhere (Node) - small but notable
Simple server with proxy.

**Standout features:**
- `--proxy http://localhost:7000/api` - proxy unmatched requests
- HTML5 history mode (`-f /index.html`)
- Auto-opens browser
- Proxy config via config file (http-proxy-middleware)

---

## Feature Frequency Analysis

How many of these 7 servers implement each feature:

| Feature | Count | Who |
|---|---|---|
| Custom port | 7/7 | Everyone |
| Directory listing | 7/7 | Everyone |
| CORS | 6/7 | All except sfz (sfz has it too actually) |
| TLS/HTTPS | 6/7 | All except sfz |
| SPA mode | 6/7 | All except sfz |
| Basic auth | 5/7 | miniserve, http-server, simple-http-server, dufs, anywhere (no) |
| File upload | 4/7 | miniserve, simple-http-server, dufs, anywhere (no) |
| Custom headers | 3/7 | miniserve, http-server, serve |
| Compression | 5/7 | miniserve, serve, simple-http-server, dufs (zip), sfz |
| Pre-compressed (.gz/.br) | 1/7 | http-server |
| Clean URLs | 2/7 | miniserve, serve |
| Proxy | 2/7 | http-server, anywhere |
| QR code | 1/7 | miniserve |
| File delete | 2/7 | miniserve, dufs |
| Archive download | 3/7 | miniserve, dufs, (serve no) |
| Config file | 3/7 | serve, dufs, anywhere |
| WebDAV | 2/7 | miniserve (read-only), dufs (full) |
| Browser auto-open | 2/7 | http-server, anywhere |
| robots.txt | 1/7 | http-server |
| .gitignore respect | 1/7 | sfz |
| README rendering | 1/7 | miniserve |
| File search | 1/7 | dufs |
| Resumable transfers | 1/7 | dufs (both directions) |
| Health endpoint | 1/7 | dufs |

---

## Insights for ok200

### Must-haves (everyone does it)
- Custom port, host binding
- Directory listing with good default UI
- CORS toggle
- SPA mode
- TLS/HTTPS
- Request logging

### High-value differentiators (few do it well)
1. **Proxy** - only http-server and anywhere have this. Huge for frontend dev
   workflows (proxy `/api/*` to a backend). This is the feature you noticed.
2. **QR code** - only miniserve. Extremely useful, trivial to implement.
3. **Pre-compressed serving** - only http-server. Smart and useful for prod-like
   testing.
4. **.gitignore respect** - only sfz (archived). Great default behavior.
5. **Clean URLs** - only miniserve and serve. People expect this now.
6. **Custom headers** - only 3/7. Essential for testing CSP, security headers.

### Features to skip (over-engineering for our use case)
- Full WebDAV (dufs territory, not ours)
- File search (dufs territory)
- YAML config files (CLI flags are fine for now)
- Theme systems (one good theme is enough)
- Archive downloads (nice-to-have, not core)
- README rendering (niche)

### The proxy pattern
The proxy feature is genuinely useful and underserved:
```
ok200 serve . --port 3000 --proxy /api=http://localhost:8080
```
This lets you serve a frontend from disk and proxy API calls to a dev backend.
Eliminates CORS issues during development. http-server does `--proxy` but it's
all-or-nothing (proxy everything that doesn't match a file). A path-based proxy
like the above is more useful.

### Proposed ok200 Phase 0 flags (updated)
```
ok200 [path]                    # serve directory (default: .)
  -p, --port <PORT>             # default: 8080
  -h, --host <HOST>             # default: 127.0.0.1
  --cors                        # Access-Control-Allow-Origin: *
  --spa                         # rewrite 404s to /index.html
  --proxy <PATH=URL>            # proxy path prefix to URL
  --no-index                    # disable auto index.html
  --no-listing                  # disable directory listing
  -q, --quiet                   # suppress request logging
  --open                        # open browser on startup
  -v, --version
  --help
```

Phase 1 additions:
```
  --tls                         # auto self-signed cert
  --tls-cert <PATH>             # custom cert
  --tls-key <PATH>              # custom key
  --auth <USER:PASS>            # basic auth
  --header <NAME:VALUE>         # custom header (repeatable)
  --qr                          # show QR code
  --clean-urls                  # strip .html extensions
  --no-dotfiles                 # hide dot files
  --gitignore                   # respect .gitignore
```
