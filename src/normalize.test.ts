// Every test's input is a literal example from docs/friction.md — the
// file that justifies each rule also proves it. When a new entry lands
// there, its example lands here first, red, then the rule makes it green.

import { describe, expect, it } from "vitest";
import { normalize } from "./normalize";

describe("normalize — one test per friction entry", () => {
  it("#1 currency reads forward", () => {
    expect(normalize("raised $60 million in funding")).toBe(
      "raised 60 million dollars in funding",
    );
    expect(normalize("costs $60")).toBe("costs 60 dollars");
  });

  it("#2 decimals keep their point", () => {
    expect(normalize("up 3.5% this year")).toBe("up 3 point 5% this year");
  });

  it("#3 version numbers keep their dots", () => {
    expect(normalize("hearit v0.1.2 shipped")).toBe(
      "hearit v0 point 1 point 2 shipped",
    );
  });

  it("#4 digit ranges say 'to'", () => {
    expect(normalize("On July 30–31, 2026")).toBe("On July 30 to 31, 2026");
  });

  it("#4 prose dashes become a breath, not silence", () => {
    expect(normalize("the key — finally — works")).toBe(
      "the key, finally, works",
    );
    expect(normalize("one thought—another")).toBe("one thought, another");
  });

  it("#5 dotted initialisms stop ending sentences", () => {
    expect(normalize("U.S. President Donald Trump")).toBe(
      "U S President Donald Trump",
    );
    expect(normalize("pauses, e.g. this one")).toBe(
      "pauses, for example this one",
    );
  });

  it("#6 hash before a number is 'number'", () => {
    expect(normalize("PR #1000 landed")).toBe("PR number 1000 landed");
  });
});

describe("normalize — composition and restraint", () => {
  it("rules compose in order: dashed currency range", () => {
    expect(normalize("a $5–10 million round")).toBe(
      "a 5 to 10 million dollars round",
    );
  });

  it("hyphens are not dashes: compound words survive", () => {
    expect(normalize("a well-known fix")).toBe("a well-known fix");
  });

  it("thousands separators pass through", () => {
    expect(normalize("about 60,000 people")).toBe("about 60,000 people");
  });

  it("percent already worked; we touch nothing around it", () => {
    expect(normalize("50% done")).toBe("50% done");
  });

  it("is idempotent: speaking twice must not drift", () => {
    const once = normalize("U.S. raised $5–10 million for v0.1.2, PR #1000 — up 3.5%");
    expect(normalize(once)).toBe(once);
  });
});
