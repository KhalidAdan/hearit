# hearit

**The key that speaks.** Select something — anything, anywhere — press
F10, and the words are read aloud. Local synthesis, no cloud, no account,
no window: just a key and a small pill with an off switch.

sayit's sibling, mirrored: sayit is the key that listens (you talk, it
types); hearit is the key that speaks (you point, it reads).

- [docs/north-star.md](docs/north-star.md) — why it exists and what it refuses to be
- [docs/architecture.md](docs/architecture.md) — how it sits in the machine
- [docs/sidecar.md](docs/sidecar.md) — the Kokoro voice engine and how to fetch it

## Status

v1 scaffold. The pipeline (hotkey → grab → synthesize → play), the
dot-matrix pill, and the dead-air instrumentation are written; the
Kokoro sidecar binary and model must be fetched per docs/sidecar.md
before the key makes sound.

## Dev

```
npm install
npm run tauri dev
```

Select text anywhere, press F10. Press again on nothing to stop; the
pill's ✕ also stops. Dead air per take is logged to the console and to
`dead-air-log.csv` in the app config dir.

Icons are currently sayit's, borrowed as placeholders.
