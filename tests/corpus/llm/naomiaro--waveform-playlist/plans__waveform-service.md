# Waveform Data as a Service

Paid cloud service that generates pre-computed waveform peak data from audio files, removing heavy client-side decoding from the browser.

## Problem

Decoding audio and computing peaks client-side is:
- Slow on mobile devices and low-end hardware
- Blocking — ties up the main thread or requires Web Workers
- Wasteful — every user decodes the same file independently
- Limited to formats the browser supports via Web Audio API

## Product

A Cloudflare Workers API that accepts audio files, computes peak data server-side, and returns a URL to cached results. The open-source library consumes this data natively.

## Architecture

```
Audio File → Cloudflare Worker → Peak Generation (WASM) → R2 Storage → CDN URL
                                                                          ↓
                                                         waveform-playlist client
```

### Components

**API Worker** — Accepts uploads or URLs, validates input, dispatches processing, returns peak data URLs.

**Processing Worker** — Decodes audio and computes peaks. Uses a WebAssembly module (FFmpeg or a lightweight decoder) to support formats beyond what browsers handle (FLAC, OGG, etc.).

**R2 Storage** — Stores computed peak data as JSON. Content-addressed by hash of audio file + resolution parameters. No egress fees.

**CDN** — Peak data served from Cloudflare's edge. Cached indefinitely since peak data for a given file + config never changes.

## API Design

### Generate peaks

```
POST /v1/peaks
Content-Type: multipart/form-data

file: <audio file>           # or url: <audio file URL>
samples_per_pixel: 512       # resolution (default: 512)
channels: 2                  # mono/stereo output (default: all)
format: "json"               # json | binary (default: json)
```

Response:
```json
{
  "id": "abc123",
  "status": "ready",
  "url": "https://peaks.waveformplaylist.com/abc123.json",
  "duration": 245.3,
  "sample_rate": 44100,
  "channels": 2,
  "samples_per_pixel": 512
}
```

For large files, return `status: "processing"` with a polling URL.

### Retrieve peaks

```
GET /v1/peaks/:id
GET /v1/peaks/:id/data        # raw peak data
```

### Client integration

```typescript
// Current — client-side decoding
<WaveformPlaylistProvider tracks={[{ src: '/audio/track.mp3' }]}>

// With service — pre-computed peaks
<WaveformPlaylistProvider tracks={[{
  src: '/audio/track.mp3',
  peaksUrl: 'https://peaks.waveformplaylist.com/abc123.json'
}]}>
```

When `peaksUrl` is provided, the library skips decoding and fetches peak data directly. Audio file is still loaded separately for playback.

## Pricing Model

**Free tier** — 100 files/month, max 10 min duration, JSON only. Enough to evaluate.

**Pro** — $9/month. 1,000 files/month, up to 60 min, binary format, webhook notifications.

**Team** — $29/month. 10,000 files/month, no duration limit, priority processing, SLA.

**Enterprise** — Custom. Dedicated processing, custom formats, on-prem option.

Revenue scales with usage. R2 has no egress so margins stay healthy as CDN traffic grows.

## Implementation Phases

### Phase 1 — Proof of concept
- Cloudflare Worker with basic upload endpoint
- FFmpeg WASM for audio decoding
- Peak computation matching the library's existing algorithm
- R2 storage with content-addressed keys
- JSON output format compatible with `webaudio-peaks`

### Phase 2 — Client integration
- Add `peaksUrl` support to `@waveform-playlist/browser`
- Fallback: if peaks URL fails, decode client-side as before
- CLI tool: `npx waveform-peaks upload track.mp3` → returns URL

### Phase 3 — Production readiness
- Auth (API keys via Cloudflare KV)
- Rate limiting per tier
- Usage dashboard
- Stripe integration for billing
- Binary peak format for smaller payloads

### Phase 4 — Advanced features
- Batch processing endpoint
- Webhook notifications for async jobs
- Multiple resolutions in one request (for zoom levels)
- URL-based ingestion (fetch audio from S3/GCS/URL instead of uploading)
- Waveform image rendering (PNG/SVG) as an add-on

## Peak Format

Uses BBC's [audiowaveform](https://github.com/bbc/audiowaveform) format — the existing standard for pre-computed waveform data. Supports both JSON and binary (.dat) output.

Benefits:
- Well-documented spec with wide adoption
- Binary format is compact and fast to parse
- Existing ecosystem of tools can generate/consume the data
- `waveform-playlist` already supports loading audiowaveform data

The processing worker runs audiowaveform (compiled to WASM) rather than implementing peak computation from scratch.

## Micropayment Rail (x402)

The subscription model works for human users but creates friction for AI agents — the primary growth vector for developer infrastructure APIs. An agent building an audio app wants to POST a file, get peaks back, and pay per call without signing up.

**x402** is an HTTP-native payment protocol where the server returns `402 Payment Required` with a price and payment address. The client pays (crypto micropayment) and retries — no API keys, no accounts, no Stripe minimums.

### Why this matters for waveform-service

- **Per-request pricing** — charge $0.001–$0.01 per file processed. Agents pay exactly for what they use.
- **No minimum transaction overhead** — Stripe's fee floor (~$0.30) makes sub-dollar transactions uneconomical. x402 removes that constraint.
- **Zero onboarding** — no API key provisioning, no OAuth, no billing portal. The payment *is* the authentication.
- **Agent-native** — AI coding assistants and automation pipelines can consume the API without human signup flows.

### Practical approach

Build the API with standard auth/billing first (Phase 3's Stripe integration). Add x402 as an alternative payment rail when the ecosystem matures. The API design doesn't change — only the payment layer.

```
# Standard path (human users)
POST /v1/peaks  +  Authorization: Bearer <api-key>

# x402 path (agents)
POST /v1/peaks  →  402 Payment Required  →  pay  →  retry  →  200 OK
```

### Where it gets more interesting

Peak generation alone may not justify per-request payment — agents could run audiowaveform locally. Higher-value operations that are harder to self-host make stronger candidates:

- Multi-format transcoding (requires FFmpeg + codec libraries)
- Loudness normalization (EBU R128 / ITU-R BS.1770)
- Stem separation (GPU-accelerated ML models)
- Batch waveform rendering at multiple zoom levels

These have real compute cost and expertise barriers that justify paying per request.

## Open Questions

- **Max file size**: Workers have memory limits (~128MB). Large files may need Durable Objects or a queued pipeline.
- **Audio storage**: Should the service optionally store audio files too, or only peaks? Storing audio enables re-processing at different resolutions but increases costs and liability.
- **Branding**: Separate product name, or `waveform-playlist Cloud`?
