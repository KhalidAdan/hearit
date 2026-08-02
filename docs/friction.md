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
