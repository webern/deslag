# Day 1 checklist — device arrival

Work top to bottom; log everything (including surprises and failures) in `lab-notebook/001-*.md` as you go. Photograph before touching.

## 1. Unboxing & hardware actuals (RQ4, RQ5)

- [ ] Photograph box, contents, device from all sides
- [ ] Record: form factor (pendant/watch/clip?), port type, charging mechanism (case? cable?), buttons, any LED/indicator lights
- [ ] Note model number and, once paired, firmware/app version
- [ ] Check for a physical recording-indicator light and whether it can be disabled (RQ5)
- [ ] Note claimed battery/water-resistance from *in-box* documentation (vs. the refuted review claims)
- [ ] Start battery log: charge to full, note time

## 2. Account & app setup

- [ ] Install app, create account — **record which email used** (matters for MCP OAuth later; Apple Private Relay users must use the relay email)
- [ ] Screenshot every permission the app requests
- [ ] Walk every settings screen; screenshot; note any consent/notification features (RQ5)
- [ ] **Does the phone app record from the phone's microphone** (vs. only syncing from the wearable)? If yes: does it use a connected external mic (USB-C/Bluetooth lav) as input? (Enables the mic-quality vs. cloud-pipeline separation experiment — see `data/accuracy/scripts/README.md`)

## 3. Developer surface bring-up

- [ ] Confirm webhook receiver is live and reachable *before* pairing (payload #1 is data)
- [ ] App → Settings → Developer Settings → paste webhook URL (include the secret token as a query parameter — Fieldy has no HMAC signing)
- [ ] Generate API key (`sk-fieldy-...`); store in local secrets, **never in this repo**
- [ ] Connect official MCP server (`https://api.fieldy.ai/mcp`) to Claude via browser OAuth

## 4. First-light tests

- [ ] Speak a scripted 30-second test passage (ground truth saved in `data/accuracy/scripts/`); wait for sync
- [ ] Confirm: transcription appears in app → webhook POST arrives → API `/conversations` returns it
- [ ] Record end-to-end timing for the first utterance (RQ6, n=1)
- [ ] Ask Claude (via MCP) about the test recording — note tool names exposed and response quality
- [ ] Diff webhook payload vs. API response for the same conversation

## 5. Wrap

- [ ] Notebook entry 001 written, payload samples sanitized into `data/`
- [ ] Commit and push
- [ ] Battery level noted at end of day
