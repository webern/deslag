# Probing and Analyzing with sigrok-cli

How to capture and decode transport/chip traffic on real hardware using
`sigrok-cli` and this repo's protocol decoders. For the actual per-transport
channel/decoder settings, see `sigrok/CONFIGS.md` (your bench's wiring — copy
it from `sigrok/CONFIGS.example`, which holds the repo-wide decoder/channel
facts).

## Hardware

- Logic analyzer: Saleae Logic (original, 8-channel), driver `fx2lafw`.
- Requires `sigrok-firmware-fx2lafw` installed on the host.
- Decoders in this repo (`sigrok/<name>/`) are linked into the system-wide
  sigrok-cli decoder search path — no `SIGROKDECODE_DIR` override needed.

## Basic workflow

Capturing and analyzing are always two separate steps — a capture always
lands in a `.sr` file first, and decoding always happens against that file,
never live off the device. This keeps the raw signal reproducible and
replayable against different decoders/annotations without re-touching the
hardware.

1. Identify the transport/chip being probed and look up its known
   configuration in `sigrok/CONFIGS.md` (your wiring) and
   `sigrok/CONFIGS.example` (decoder id, decoder channels, samplerate).
2. Attach LA probe clips per the wiring documented there.
3. Capture (see below).
4. Analyze the resulting `.sr` file (see below).

### Capturing

Record raw samples to a `.sr` file. `--channels <ch0>=<name0>,...` selects
and renames the physical channels (D0, D1, …) to meaningful signal names at
capture time — that naming is baked into the `.sr` file's metadata, so the
analyze step doesn't need to repeat it. Pick the capture strategy that fits
the scenario:

- **Continuous/repeating signal** (or you know exactly how long the DUT
  runs for): time-based capture.
  ```
  sigrok-cli -d fx2lafw --channels <ch0>=<name0>,<ch1>=<name1>,... \
    --config samplerate=<RATE> --time <MS> -o capture.sr
  ```
- **One-shot or unknown-timing transaction** (e.g. a single test run whose
  exact duration you don't want to guess, or you don't yet know whether
  there's any traffic at all): trigger-based capture, positioned on the
  meaningful edge (e.g. a protocol start condition) instead of idle bus time,
  with `--time` bounding the overall session so it can't hang forever if the
  trigger never fires.
  ```
  sigrok-cli -d fx2lafw --channels <ch0>=<name0>,<ch1>=<name1>,... \
    --config samplerate=<RATE>:captureratio=1 \
    --triggers <name>=<edge> -o capture.sr --time <MS>
  ```
  If no matching capture appears within `<MS>`, treat that as "no traffic,"
  not a capture bug. `<MS>` only needs to be "long enough to plausibly
  happen," not an exact match to the DUT's runtime. If `--time` alone proves
  insufficient to bound a run in practice, fall back to wrapping the command
  in an external `timeout <CEILING_S>s ...` instead.

### Analyzing

Decode a previously captured `.sr` file — channel names come from the
capture, so only the decoder binding is needed:
```
sigrok-cli -i capture.sr \
  -P <decoder>:<pdchan0>=<name0>:<pdchan1>=<name1>,... \
  -A <decoder>
```
- `-P decoder:pdchan=devchan` binds decoder channels to the names given at
  capture time via `--channels`.
- `-A decoder` prints only that decoder's annotations.

To decode a chip on top of its transport, stack a chip decoder onto the
transport decoder — chip decoders in this repo consume the transport
decoder's output rather than raw LA channels, so no extra channel bindings
are needed for `<chip>`:
```
sigrok-cli -i capture.sr -P <decoder>:<pdchan0>=<name0>,...,<chip> -A <decoder>,<chip>
```
- `-A <decoder>,<chip>` shows both transport framing and chip-level
  annotations — useful while still trusting/debugging the framing.
- `-A <chip>` alone shows just the chip's decoded output, once the framing
  underneath is trusted.

Re-run the analyze step as often as needed against the same `capture.sr` —
different decoder, different annotations, no new capture required.

## Useful commands

- List connected devices: `sigrok-cli --scan`
- List available decoders: `sigrok-cli -L`
- Show a decoder's channels/options: `sigrok-cli -P <decoder> --show`
