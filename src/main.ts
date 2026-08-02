// The coordinator, in Effect — sayit's coordinator run backwards. Rust
// touches the OS, the sidecar thinks; this file makes decisions. The whole
// grammar of the key lives in onPressed: nothing selected → stop; the same
// selection while speaking → stop; a new selection → the new one wins,
// instantly, no queue, no modes.
//
// Reading guide (same as sayit's):
// - An Effect is a *description* — nothing runs until Effect.runPromise.
// - Effect.gen + yield* reads like async/await with typed errors.
// - Ref is a mutable cell shared across concurrently-running Effects.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Data, Duration, Effect, Ref } from "effect";

type State = "idle" | "speaking";

// Mirrors `Grab` in grab.rs (serde renames to camelCase).
type Grab = {
  text: string;
  clipboardSaveMs: number;
  copyMs: number;
  waitMs: number;
  totalMs: number;
};

// Mirrors `Spoken` in lib.rs. queued=false means the take was superseded
// while this sentence was synthesizing — the audio was dropped, not played.
type Spoken = {
  engineWaitMs: number;
  attempts: number;
  httpMs: number;
  decodeMs: number;
  audioMs: number;
  queued: boolean;
};

// One take's dead-air ledger, assembled here because this is where all
// three clocks meet: the OS key stamp, the Rust stage timings, and the
// playback_started stamp from the audio thread.
type TakeLog = {
  pressMs: number;
  grabMs: number;
  chars: number;
  sentences: number;
  engineWaitMs: number;
  firstSynthMs: number;
  firstAudioMs: number;
  logged: boolean;
};

class CmdError extends Data.TaggedError("CmdError")<{
  readonly cmd: string;
  readonly cause: unknown;
}> {}

const cmd = <T = void>(name: string, args?: Record<string, unknown>) =>
  Effect.tryPromise({
    try: () => invoke<T>(name, args),
    catch: (cause) => new CmdError({ cmd: name, cause }),
  });

// Fire-and-forget: the pill's visibility must never fail the pipeline.
const pill = (visible: boolean) =>
  cmd(visible ? "pill_show" : "pill_hide").pipe(Effect.ignore);

const stateRef = Ref.unsafeMake<State>("idle");
const currentTextRef = Ref.unsafeMake<string | null>(null);
// Client-side generation counter: a newer press invalidates any sentence
// loop still running for an older one. Rust keeps its own counter (bumped
// by speak_begin/speak_stop) so audio synthesized for a dead take can
// never reach the speaker; this one just stops us wasting synth calls.
const genRef = Ref.unsafeMake(0);
// True while a sentence loop is still feeding the sink — playback_done
// during that window is an underrun (synth slower than speech), not the
// end of the take.
const feedingRef = Ref.unsafeMake(false);
const takeRef = Ref.unsafeMake<TakeLog | null>(null);

// ---- the sentence splitter -------------------------------------------

// The streaming unit is the sentence: Kokoro's latency scales with input
// length, and dead air is measured against the FIRST sentence only.
// Regex, not NLP — an abbreviation edge case costs a slightly odd pause,
// never a wrong word.
const MAX_SENTENCE = 400;
const splitSentences = (text: string): string[] => {
  const flat = text.replace(/\s+/g, " ").trim();
  const rough = flat.match(/[^.!?…]+[.!?…]+["')\]]*\s*|[^.!?…]+$/g) ?? [flat];
  const out: string[] = [];
  for (const r of rough) {
    let s = r.trim();
    // A wall of text with no punctuation still has to stream: carve at
    // the last space before the cap rather than handing Kokoro a slab.
    while (s.length > MAX_SENTENCE) {
      const cut = s.lastIndexOf(" ", MAX_SENTENCE);
      const at = cut > 40 ? cut : MAX_SENTENCE;
      out.push(s.slice(0, at).trim());
      s = s.slice(at).trim();
    }
    if (s) out.push(s);
  }
  // Tiny fragments ("No.", "Dr.") ride with the previous sentence instead
  // of costing a whole synth round-trip each.
  const merged: string[] = [];
  for (const s of out) {
    if (merged.length > 0 && s.length < 24) merged[merged.length - 1] += " " + s;
    else merged.push(s);
  }
  return merged.length > 0 ? merged : [flat];
};

// ---- stopping ---------------------------------------------------------

// Idempotent, and every path to silence runs through it: the key with
// nothing selected, the key on the same text, the pill's X, a failed take.
const stopSpeaking = Effect.gen(function* () {
  yield* Ref.update(genRef, (g) => g + 1); // cancels any sentence loop
  yield* cmd("speak_stop").pipe(Effect.ignore); // silences the sink NOW
  yield* Ref.set(feedingRef, false);
  yield* Ref.set(currentTextRef, null);
  yield* Ref.set(stateRef, "idle");
  yield* pill(false);
});

// ---- the pipeline -----------------------------------------------------

const onPressed = (stampMs: number) =>
  Effect.gen(function* () {
    const t0 = stampMs > 0 ? stampMs : Date.now();
    const grab = yield* cmd<Grab>("grab_selection");
    const text = grab.text.trim();
    const speaking = (yield* Ref.get(stateRef)) === "speaking";
    const current = yield* Ref.get(currentTextRef);

    // The whole grammar. It never gets more complicated than this.
    if (text.length === 0 || (speaking && text === current)) {
      return yield* stopSpeaking;
    }

    // New selection wins: speak_begin stops the old voice inside Rust and
    // hands back a take token — audio from any older take can never reach
    // the speaker once this returns.
    const gen = yield* Ref.updateAndGet(genRef, (g) => g + 1);
    const take = yield* cmd<number>("speak_begin");
    yield* Ref.set(currentTextRef, text);
    yield* Ref.set(stateRef, "speaking");
    yield* Ref.set(feedingRef, true);

    const sentences = splitSentences(text);
    yield* Ref.set(takeRef, {
      pressMs: t0,
      grabMs: grab.totalMs,
      chars: text.length,
      sentences: sentences.length,
      engineWaitMs: 0,
      firstSynthMs: 0,
      firstAudioMs: 0,
      logged: false,
    });
    yield* pill(true);
    yield* Effect.sync(() =>
      console.log(
        `[hearit] speaking ${text.length} chars as ${sentences.length} sentence(s) — grab ${grab.totalMs}ms (save ${grab.clipboardSaveMs} · copy ${grab.copyMs} · wait ${grab.waitMs})`,
      ),
    );

    for (let i = 0; i < sentences.length; i++) {
      if ((yield* Ref.get(genRef)) !== gen) return; // a newer press won
      // Per-sentence seatbelt: synth_waiting is patient for 30s on the
      // Rust side; 45s here means a wedged stage returns the key, always.
      const spoken = yield* cmd<Spoken>("speak_sentence", {
        text: sentences[i],
        take,
      }).pipe(Effect.timeout(Duration.seconds(45)));
      if (!spoken.queued) return; // Rust says this take is dead
      if (i === 0) {
        yield* Ref.update(
          takeRef,
          (t) =>
            t && {
              ...t,
              engineWaitMs: spoken.engineWaitMs,
              firstSynthMs: spoken.httpMs + spoken.decodeMs,
              firstAudioMs: spoken.audioMs,
            },
        );
      }
    }
    yield* Ref.set(feedingRef, false); // queue is fully fed; the sink drains
  }).pipe(
    // CmdError = a stage failed; TimeoutException = a stage never answered.
    // Both end the same way: silence, back to idle, key usable.
    Effect.catchAll((e) =>
      Effect.gen(function* () {
        yield* Effect.sync(() => console.error("[hearit] take failed:", e));
        yield* stopSpeaking;
      }),
    ),
  );

// ---- events from Rust -------------------------------------------------

// Payload is a wall-clock stamp from hotkey.rs, taken at the OS event.
void listen<number>("speak_pressed", (e) => {
  if (e.payload > 0) console.log(`[dead-air] press dispatch: ${Date.now() - e.payload}ms`);
  void Effect.runPromise(onPressed(e.payload));
});

// The audio thread stamped the moment the sink went from empty to fed.
// First time per take = the dead-air number the north star is judged by.
void listen<number>("playback_started", (e) =>
  void Effect.runPromise(
    Effect.gen(function* () {
      const take = yield* Ref.get(takeRef);
      if (!take || take.logged) return;
      yield* Ref.update(takeRef, (t) => t && { ...t, logged: true });
      const deadAirMs = Math.max(0, e.payload - take.pressMs);
      const plumbing = Math.max(
        0,
        deadAirMs - take.grabMs - take.engineWaitMs - take.firstSynthMs,
      );
      yield* Effect.sync(() =>
        console.log(
          [
            `[dead-air] ━━ ${deadAirMs}ms from key to voice · ${take.chars} chars in ${take.sentences} sentence(s)`,
            `[dead-air]   grab ${take.grabMs}ms · engine wait ${take.engineWaitMs}ms · first synth ${take.firstSynthMs}ms · plumbing ${plumbing}ms`,
            `[dead-air]   (first sentence carries ${(take.firstAudioMs / 1000).toFixed(1)}s of audio)`,
          ].join("\n"),
        ),
      );
      yield* cmd("log_gap", {
        row: {
          deadAirMs,
          chars: take.chars,
          sentences: take.sentences,
          grabMs: take.grabMs,
          engineWaitMs: take.engineWaitMs,
          firstSynthMs: take.firstSynthMs,
          firstAudioMs: take.firstAudioMs,
        },
      }).pipe(Effect.ignore);
    }),
  ),
);

// The sink has been empty for the grace window. If we were still feeding
// it, that's an underrun (synth slower than speech) — stay up; the next
// enqueue restarts playback. Otherwise the take is over.
void listen("playback_done", () =>
  void Effect.runPromise(
    Effect.gen(function* () {
      if ((yield* Ref.get(stateRef)) !== "speaking") return;
      if (yield* Ref.get(feedingRef)) {
        return yield* Effect.sync(() =>
          console.warn("[hearit] underrun: the voice outran the synth"),
        );
      }
      yield* Ref.set(currentTextRef, null);
      yield* Ref.set(stateRef, "idle");
      yield* pill(false);
    }),
  ),
);

// The pill's X. speak_stop already ran (the pill invokes it directly, no
// round-trip through here) — this cleans up the coordinator's state.
void listen("pill_stop", () => void Effect.runPromise(stopSpeaking));

void listen("sidecar_ready", () =>
  console.log("[hearit] engine warm — the key is live"),
);
void listen("pipeline_error", (e) =>
  console.error("[hearit] pipeline error:", e.payload),
);

// Race-proofing, same as sayit: warmup may finish before this page's
// listeners exist, so we PULL readiness once at startup too.
void Effect.runPromise(
  Effect.gen(function* () {
    if (yield* cmd<boolean>("is_ready")) {
      yield* Effect.sync(() =>
        console.log("[hearit] engine warm — the key is live"),
      );
    }
  }).pipe(Effect.ignore),
);
