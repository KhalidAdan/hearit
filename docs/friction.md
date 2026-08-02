# The friction list

Earned by listening, not imagined (north star, the v2 gate). Every entry
is something real ears actually hit during real work. Fixes get built
when a cluster forms, not per-entry.

Format: date noticed · what happened · suspected stage · fix shape.

---

## 1. Currency reads backwards

- **2026-08-01** — `$60 million` spoken as "Dollar 60 million".
  Correct: "sixty million dollars". Reported as one of a "decent amount
  of errors like this" — money amounts are common in exactly the
  articles worth listening to.
- **Stage:** text normalization inside the sidecar (kokoros
  `normalize.rs` expands symbols/numbers before phonemization; it reads
  `$` in place instead of reordering it after the amount).
- **Fix shape:** a small deterministic normalize pass in hearit between
  grab and synthesize — regex class: `$X` → "X dollars", `$X million` →
  "X million dollars", same for %, °, £, €, ranges (`$5–10M`). Ours, not
  upstream's, so it survives a sidecar swap. NOT a dictionary entry —
  the dictionary maps words, this maps patterns. (Alternatively/also: a
  PR to kokoros normalize.rs, but hearit shouldn't depend on it.)
- **Status:** logged, waiting for the cluster ("decent amount of errors
  like this" — collect the other shapes before designing the pass).

## 2. Decimals lose their point

- **2026-08-01** — `3.5%` spoken as "three five percent". Correct:
  "three point five percent". The decimal point is being dropped or
  treated as a separator, so the digits read as a list. Likely
  independent of the `%` (test: bare `3.5` in prose) — if so, this hits
  every decimal in every article, which makes it the highest-frequency
  entry so far.
- **Stage:** same as #1 — sidecar text normalization (number expansion).
- **Fix shape:** same normalize pass as #1 — decimals rule:
  `\d+\.\d+` → "N point D-digits" ("3.5" → "three point five", "3.14" →
  "three point one four") before the sidecar sees it.
- **Status:** logged, clustering with #1.

## 3. Version numbers lose their dots

- **2026-08-01** — `v0.1.2` spoken as "V. zero. one. two." Correct:
  "vee zero point one point two". Same root as #2 seen from the other
  side: dots between digits vanish instead of becoming "point". Version
  strings can't hide behind a decimal-number rule (0.1.2 is not a
  float), so the pass needs the general form, not the special case.
- **Stage:** same as #1/#2 — sidecar number normalization.
- **Fix shape:** one rule covers #2 AND #3: any `.` flanked by digits
  becomes " point " ("3.5" → "3 point 5", "0.1.2" → "0 point 1 point
  2") before number expansion. Uniform, explainable, no float-parsing.
- **Status:** logged. The cluster now has a unifying rule candidate.

## 4. Dashes are ignored

- **2026-08-01** — `On July 30–31, 2026` spoken as "On July 30. 31,
  2026." The en dash vanishes, so the range reads as two disconnected
  numbers. Em dashes in prose presumably vanish the same way, which
  silently glues clauses together that the author deliberately held
  apart.
- **Stage:** sidecar normalization drops – and — instead of voicing or
  pausing them.
- **Fix shape:** two rules in the pass: (a) dash flanked by digits
  becomes " to " ("30–31" → "30 to 31", "2019–2023" → "2019 to 2023",
  composes with #1 for "$5–10M"); (b) prose em/en dash becomes a comma —
  not spoken, but breathed, which is what a dash is for.
- **Status:** logged. Shapes so far: currency reorder (#1), digit-dot
  (#2/#3), digit-dash ranges + prose dashes (#4).

## 5. Dotted initialisms read as sentence ends

- **2026-08-01** — `U.S. President Donald Trump` spoken as "U. (pause)
  S. President…". The periods inside the initialism are treated as
  sentence boundaries.
- **Stage:** BOTH sides, and one of them is ours. hearit's own sentence
  splitter (main.ts) cuts on every `.`, so "U.S." mid-sentence can
  fracture one sentence into multiple synth requests — an inter-request
  pause plus sentence-final intonation on "U." That pause is likely
  ours, not espeak's. The <24-char merge hides some cases, not all.
- **Fix shape:** rewrite dotted initialisms BEFORE the splitter ever
  sees them: `([A-Z]\.){2,}` → the bare letters spaced ("U.S." → "U S",
  "e.g."/"Ph.D." handled by a small known-abbreviation list). One fix
  location (the normalize pass, which therefore must run before
  splitting, in TS or invoked ahead of it) cures the splitter fracture
  and the espeak pause at once. Ordering fact #2 for the pass: it runs
  pre-split, not per-sentence.
- **Status:** logged. First entry implicating hearit's own code.

## 6. Hash before a number is "Hash"

- **2026-08-01** — `PR #1000` spoken as "PR Hash 1000". Correct: "PR
  number one thousand". `#` flanking digits means "number" in English;
  the sidecar names the symbol instead of translating it.
- **Stage:** sidecar symbol normalization — same family as #1's `$`:
  symbols whose English reading depends on their position relative to
  the number.
- **Fix shape:** joins the symbol rules in the pass: `#N` → "number N".
  Rule family so far: `$` (reorder + "dollars"), `#` ("number"),
  presumably `%` is fine (it said "percent" in #2), watch for `°`, `£`,
  `€`, `&`.
- **Status:** logged, clustering with #1.
