# Modal TTS Setup

Use this optional path when you do not want to run `nanovllm_gradio_local.py` on your own machine. The Modal app exposes the same REST-style TTS API shape consumed by the game.

## Configure the Modal App

Open `nanovllm_app_modal.py` and review these constants near the top of the file:

```python
MODAL_APP_NAME = "voxcpm2-nanovllm-service"
MODAL_VOICES_VOLUME_NAME = "voxcpm-voices"
MODAL_SECRET_NAME = "voxcpm-secrets"
```

Change them if you want a different Modal app name, volume name, or secret name.

## Install and Authenticate Modal

Install the Modal CLI in the environment you use for deployment:

```bash
pip install modal
```

Authenticate your machine with Modal:

```bash
modal setup
```

## Create the Secret

The app reads `TTS_API_KEY` from a Modal secret. Create it with the same value that your game will send in `TTS_API_KEY`:

```bash
modal secret create voxcpm-secrets TTS_API_KEY=your_tts_api_key
```

If you changed `MODAL_SECRET_NAME`, use that same name instead of `voxcpm-secrets`.

## Create the Voice Volume

Create the Modal volume used to store reference voices:

```bash
modal volume create voxcpm-voices
```

If you changed `MODAL_VOICES_VOLUME_NAME`, use that same name instead of `voxcpm-voices`.

## Upload the Reference Voice

Upload the local reference voice file to the root of the Modal volume:

```bash
modal volume put voxcpm-voices ./voices/voz_1.wav voz_1.wav
```

The app expects `voice_id="voz_1.wav"`, which maps to `/voices/voz_1.wav` inside the Modal container.

You can inspect the volume contents with:

```bash
modal volume ls voxcpm-voices
```

## Deploy

Deploy the Modal app:

```bash
modal deploy nanovllm_app_modal.py
```

After deployment, Modal prints the public endpoint for the `generate_api` function. Use that URL as the TTS endpoint in your game configuration.

## Configure the Game

For direct `.env` usage with `DOD_USE_LOCAL_API=True`:

```env
DOD_DISABLE_TTS=False
DOD_USE_LOCAL_API=True
TTS_API_URL=https://your-modal-generate-api-url
TTS_API_MODE=rest
TTS_API_KEY=your_tts_api_key
```

For mapper-based usage, add the endpoint to your `inference_map.json`:

```json
{
  "tts": {
    "primary": {
      "name": "modal-tts",
      "url": "https://your-modal-generate-api-url",
      "mode": "rest"
    }
  }
}
```

Keep `TTS_API_KEY` in the game environment when the Modal secret is configured, because the game sends it as a bearer token to the Modal endpoint.
