# hearit

**The key that speaks.**

Your screen can show you anything. It just assumes your eyes will do all the work.

---

## The Idea

hearit is not an app. It has no window, no place you go, no document format it prefers.
It is one new key on your keyboard: select something — anything, anywhere — press the
key, and the words are spoken to you. You keep scrolling, or you lean back. Your eyes
are off the hook.

It is sayit's sibling, and the mirror is exact: sayit is the key that listens — you
talk, it types. hearit is the key that speaks — you point, it reads. Together the
keyboard finally works in both directions. And because it's a key, it inherits the
same brutal standard, taken just as literally:

- A key works on any selectable text in any program, without the program knowing.
- A key responds the instant you press it. There is no buffering spinner on the letter T.
- A key has no account, no settings screen, no onboarding.
- A key that sent your reading to a server would be a wiretap on your attention. A key
  that reads aloud is still a key.

Reading is the last thing screens make you do manually. Podcasts freed your eyes from
articles someone else chose to record; hearit frees them from everything else — the
docs, the diff, the long reply, the terms nobody reads. The bet: the moment hearing a
selection is as reliable as pressing T, "read" and "listen" stop being separate
decisions. You'll just notice which one your body picked.

## What the Key Owes You

**Dead air is the enemy.** Dead air: you press the key, and no voice has started yet.
Every version of hearit is judged by that silence, measured in milliseconds, not
adjectives. The first sentence starts speaking while the rest is still being
synthesized — streaming is not a v3 luxury here, it is the v1 architecture, because a
ten-minute article must start in under a second or the key is a loading bar.

**Everywhere, or it doesn't count.** If you can select it, it can be spoken. The
browser, the terminal, the PDF, the code comment, the chat message from someone who
types in walls. There is no context menu to install into every app — Windows doesn't
have one to give — so the key takes the selection the way every app already
surrenders it: a borrowed copy, clipboard restored before you notice it was gone.

**Silence on demand.** A voice you can't stop instantly isn't a reader, it's an alarm.
While hearit speaks, one small pill floats on screen — a dot-matrix waveform pulsing
with the voice, and an X. That pill is the only face this tool gets, and it exists for
one reason: you should never have to hunt for the off switch of something that's
talking. The key itself is the other off switch — press it with nothing new selected
and the voice stops; press it with a new selection and the new one wins, instantly, no
queue, no modes. The key reads what you *just* chose. That grammar never gets more
complicated.

**Speed is yours.** Speed is to listening what font size is to reading — not a
preference, a prerequisite. Nobody listens at someone else's pace for long. One voice
to start, chosen carefully; a small shelf of voices someday. But speed comes first.

**Your reading stays home.** What you select is what you're thinking about, and that
never leaves this machine. Synthesis is local, on my own silicon — no network calls,
no API keys, no bill that scales with how much I'd rather listen. Not a privacy
toggle; the definition of a key.

**Nothing I can't explain.** hearit is also how I keep learning this field — the same
clause as sayit, unchanged. The model may stay a black box; the plumbing may not. Any
line of glue I can't teach back gets rewritten until I can.

## Under the Keycap

```
hotkey ── press ──▶ grab ── text ──▶ synthesize ── audio ──▶ play
  (global-shortcut)   (borrowed copy)   (TTS sidecar, streaming)   (audio out + pill)
```

Four stages, one direction — sayit's pipeline run backwards. Where sayit ends by
injecting text at your cursor, hearit begins by lifting text from your selection; the
clipboard trick is the same trick, mirrored. Each stage replaceable without the others
noticing: swap the voice model, keep the plumbing; redraw the pill, keep the pipeline.

The house is the same house: Tauri. Rust owns the OS-facing stages — hotkey, clipboard,
audio device. TypeScript orchestrates and draws the pill. The voice model runs as a
sidecar — a Kokoro-class local TTS we talk to, never link against — behind the same
boundary sayit keeps whisper.cpp behind. Two projects, one architecture, each teaching
the other.

## The Road

### v1 — A key that speaks

The smallest thing that reads one honestly-selected paragraph out loud, starting in
under a second. Hold nothing, configure nothing: select, press, listen. One good
voice at one honest speed. Streaming synthesis from the first commit, because dead
air is the enemy from the first commit. And the pill — waveform and an X — because
unlike sayit, v1 here makes *sound*, and a thing that makes sound owes you its off
switch before it owes you anything else.

### v2 — A key you forget is software

Startup residence and a tray icon, so it can be quit and trusted. Speed under your
thumb — adjusted live, remembered forever. The model warm at boot so the first press
of the day speaks as fast as the hundredth. Dead air measured stage by stage and
pinned to the GPU if one exists. v2 begins only after v1 has read real pages during
real work; the friction list must be earned by listening, not imagined.

### v3 — A key that reads the way you listen

The listening comforts that only surface after hours of use: skip back a sentence
when your attention lapsed, a small shelf of curated voices instead of one, the pill
polished into something you'd miss. v3 begins only when v2's dead air is a number on
the table, not a mood.

### Beyond the road (noticed, not promised)

Same three-part test as sayit: the need showed up while listening, dead air doesn't
widen, and I can explain the mechanism after building it.

- A pronunciation dictionary — sayit's dictionary, reflected: it learns how *you*
  spell; this learns how words *sound*
- Follow-along highlighting, if the OS ever makes "where is this text on screen" honest
- An LLM pass between grab and synthesize — the wall of text in, the gist out loud
- Voice cloning — any voice from six seconds of audio, because the models can
- An installer for strangers, if hearit ever earns ears other than mine

## What hearit Refuses to Be

No cloud voices, no account, no telemetry, no subscription — a key is bought once with
effort and owned forever. No browser extension; the key already works in the browser,
and everywhere else too. No file reader, no read-later inbox, no ebook mode — hearit
reads selections, not libraries; wanting a document heard costs exactly one Ctrl+A.
No queue. The key speaks what you just chose, then it's done.

## How I'll Know It's Done

Keys don't have success metrics; they have reflexes. hearit is done when select-and-
press is as unconscious as reaching for backspace — when you realize you're three
paragraphs into an article and your eyes left the screen two paragraphs ago. When
long reads default to ears, and eyes are for skimming, and you never once decided
that on purpose.

Its sibling types what you say. This one says what you'd rather not read. The
keyboard, finally, in both directions.
