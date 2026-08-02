# hearit

**The key that speaks.** Select something — anything, anywhere — press
F10, and the words are read aloud. Local synthesis, no cloud, no account,
no window: just a key and a small pill with an off switch.

sayit's sibling, mirrored: sayit is the key that listens (you talk, it
types); hearit is the key that speaks (you point, it reads).

- [docs/north-star.md](docs/north-star.md) — why it exists and what it refuses to be
- [docs/architecture.md](docs/architecture.md) — how it sits in the machine
- [docs/sidecar.md](docs/sidecar.md) — the Kokoro voice engine and how to fetch it

## Install

Two downloads from [Releases](https://github.com/KhalidAdan/hearit/releases),
one time each:

1. **The app** — run `hearit_x.y.z_x64-setup.exe`. Updates after that are
   automatic and tiny.
2. **The voice** — download
   [hearit-sidecar-cuda-win64.zip](https://github.com/KhalidAdan/hearit/releases/download/sidecar-v1/hearit-sidecar-cuda-win64.zip)
   (~1.7GB, once) and extract it so `sidecar\` sits next to `hearit.exe`.
   Needs an NVIDIA GPU; no GPU, rename the bundled `koko-cpu.exe` over
   `koko.exe` (works, slower). The app and the voice live in separate
   releases on purpose: the app updates weekly and silently, the voice is
   bought once — see docs/sidecar.md.

Then: select text anywhere, press **F10**. Press again on nothing to
stop; the pill's ✕ also stops; a new selection wins instantly.

## Dev

```
npm install
npm run tauri dev
```

Dead air per take is logged to the console and to `dead-air-log.csv` in
the app config dir — that file is the project's conscience.
