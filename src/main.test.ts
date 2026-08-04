// The dead-air ledger race, replayed on purpose. The bug these tests pin
// down: playback_started is stamped INSIDE the first speak_sentence call,
// so the event used to reach JS before the promise resolved — and the
// ledger logged the zero placeholders instead of the real stage numbers.
// The fix defers the row until both clocks are in; these tests drive the
// real coordinator (main.ts) with both orderings and prove the row.
//
// Only the two Tauri seams are mocked — invoke and listen. Everything
// between the key press and the log_gap row is the code that ships.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

type Handler = (e: { payload: unknown }) => void;

// A promise with the resolve handle held outside — lets a test keep
// speak_sentence in flight while playback_started lands, which is the
// exact window the bug lived in.
const deferred = <T>() => {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
};

const GRAB = {
  text: "The ledger deserves real numbers.",
  clipboardSaveMs: 5,
  copyMs: 40,
  waitMs: 15,
  totalMs: 60,
};

const SPOKEN = {
  engineWaitMs: 120,
  attempts: 1,
  httpMs: 300,
  decodeMs: 50,
  audioMs: 4200,
  queued: true,
};

// Boots a fresh coordinator (fresh Refs) against the mocked seams.
// speakSentence is injectable so each test controls when sentence 0
// resolves relative to the playback_started event.
const boot = async (speakSentence: () => Promise<unknown>) => {
  vi.resetModules();
  const { invoke } = (await import("@tauri-apps/api/core")) as unknown as {
    invoke: Mock;
  };
  const { listen } = (await import("@tauri-apps/api/event")) as unknown as {
    listen: Mock;
  };

  const handlers = new Map<string, Handler>();
  listen.mockImplementation((name: string, h: Handler) => {
    handlers.set(name, h);
    return Promise.resolve(() => {});
  });

  const logGap = vi.fn(() => Promise.resolve());
  invoke.mockImplementation((name: string, args?: Record<string, unknown>) => {
    switch (name) {
      case "get_idle_minutes":
        return Promise.resolve(0); // no sleep timer in tests
      case "is_ready":
        return Promise.resolve(false);
      case "grab_selection":
        return Promise.resolve(GRAB);
      case "speak_begin":
        return Promise.resolve(1);
      case "speak_sentence":
        return speakSentence();
      case "log_gap":
        return logGap(args);
      default:
        return Promise.resolve(undefined);
    }
  });

  await import("./main");
  const fire = (name: string, payload: unknown) =>
    handlers.get(name)!({ payload });
  return { fire, logGap, invoke };
};

// Let the Effect runtime drain everything currently runnable.
const settle = () => new Promise((r) => setTimeout(r, 20));

const PRESS_MS = 1_000;
const STARTED_MS = 1_980; // dead air = 980ms by definition

// The row the fix promises: real stage numbers, dead_air_ms untouched.
const EXPECTED_ROW = {
  row: {
    deadAirMs: STARTED_MS - PRESS_MS,
    chars: GRAB.text.length,
    sentences: 1,
    grabMs: GRAB.totalMs,
    engineWaitMs: SPOKEN.engineWaitMs,
    firstSynthMs: SPOKEN.httpMs + SPOKEN.decodeMs,
    firstAudioMs: SPOKEN.audioMs,
  },
};

describe("dead-air ledger — the event/promise race", () => {
  beforeEach(() => {
    vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("playback_started beats the Spoken result — the row still gets real numbers", async () => {
    const sentence = deferred<typeof SPOKEN>();
    const { fire, logGap } = await boot(() => sentence.promise);

    fire("speak_pressed", PRESS_MS);
    // The pipeline is now awaiting speak_sentence. Land the stamp first —
    // the ordering that used to write zeros.
    await settle();
    fire("playback_started", STARTED_MS);
    await settle();

    // Half the clocks in → no row yet. This deferral IS the fix.
    expect(logGap).not.toHaveBeenCalled();

    sentence.resolve(SPOKEN);
    await vi.waitFor(() => expect(logGap).toHaveBeenCalledTimes(1));
    expect(logGap).toHaveBeenCalledWith(EXPECTED_ROW);
  });

  it("Spoken resolves first — the stamp arrives late and flushes the same row", async () => {
    const { fire, logGap } = await boot(() => Promise.resolve(SPOKEN));

    fire("speak_pressed", PRESS_MS);
    await settle();
    // Sentence 0 fully resolved; no stamp yet → still no row.
    expect(logGap).not.toHaveBeenCalled();

    fire("playback_started", STARTED_MS);
    await vi.waitFor(() => expect(logGap).toHaveBeenCalledTimes(1));
    expect(logGap).toHaveBeenCalledWith(EXPECTED_ROW);
  });

  it("an underrun restart fires playback_started again — one row, first stamp wins", async () => {
    const { fire, logGap } = await boot(() => Promise.resolve(SPOKEN));

    fire("speak_pressed", PRESS_MS);
    await settle();
    fire("playback_started", STARTED_MS);
    await vi.waitFor(() => expect(logGap).toHaveBeenCalledTimes(1));

    // The sink drained and refilled mid-take; the stamp must not move.
    fire("playback_started", STARTED_MS + 5_000);
    await settle();
    expect(logGap).toHaveBeenCalledTimes(1);
    expect(logGap).toHaveBeenCalledWith(EXPECTED_ROW);
  });
});
