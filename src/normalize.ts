// The normalize pass: deterministic text-to-speakable-text rewriting,
// run on the WHOLE grabbed text BEFORE the sentence splitter (friction
// #5 made that placement non-negotiable — half its bug was our splitter
// fracturing on dots this pass removes).
//
// Every rule was earned by listening: born-from cites the entry in
// docs/friction.md, and that entry's literal example is a test case in
// normalize.test.ts. Order is load-bearing (friction #4 + #1: the dash
// rule must turn "$5–10 million" into "$5 to 10 million" before the
// currency rule can wrap it). The array IS the documentation.
//
// What this refuses to be: it never touches plain words — that's the
// future dictionary's turf (word-level, user-editable). This is the
// pattern layer: symbols, digits, punctuation-as-speech.

type Rule = {
  name: string;
  bornFrom: string; // docs/friction.md entry
  apply: (text: string) => string;
};

const RULES: Rule[] = [
  {
    // "U.S." → "U S": dotted initialisms read as sentence ends, both by
    // espeak and by our own splitter. Uppercase single letters only —
    // narrow on purpose.
    name: "initialisms",
    bornFrom: "#5",
    apply: (t) =>
      t.replace(/\b(?:[A-Z]\.){2,}/g, (m) => m.replace(/\./g, " ").trim()),
  },
  {
    // The handful of dotted abbreviations that fracture the splitter but
    // aren't single capital letters. A list, not a pattern — each entry
    // is a word we know how English speaks. Grows only when heard.
    name: "abbreviations",
    bornFrom: "#5",
    apply: (t) =>
      t
        .replace(/\be\.g\./gi, "for example")
        .replace(/\bi\.e\./gi, "that is")
        .replace(/\bPh\.D\./g, "P H D")
        .replace(/\bvs\./gi, "versus"),
  },
  {
    // "July 30–31" → "July 30 to 31". True en/em dashes between digits
    // are ranges. ASCII hyphens are deliberately untouched: "well-known"
    // must never become "well to known", and 2019-01-01 is ambiguous.
    name: "digit ranges",
    bornFrom: "#4",
    apply: (t) => t.replace(/(\d)\s*[–—]\s*(?=\d)/g, "$1 to "),
  },
  {
    // "$60 million" → "60 million dollars". The $ moves to the end and
    // becomes a word, wrapping any scale word and any range the
    // previous rule produced ("$5 to 10 million" → "5 to 10 million
    // dollars"). "$3.99" becomes "3.99 dollars" here and "3 point 99
    // dollars" two rules later — understandable, if not bank-teller.
    name: "currency",
    bornFrom: "#1",
    apply: (t) =>
      t.replace(
        /\$(\d[\d,]*(?:\.\d+)?(?:\s+to\s+\d[\d,]*(?:\.\d+)?)?)(\s+(?:thousand|million|billion|trillion))?\b/gi,
        "$1$2 dollars",
      ),
  },
  {
    // "PR #1000" → "PR number 1000". A hash flanking digits means
    // "number" in English; the sidecar names the symbol instead.
    name: "hash numbers",
    bornFrom: "#6",
    apply: (t) => t.replace(/#(\d)/g, "number $1"),
  },
  {
    // "3.5" → "3 point 5", "v0.1.2" → "v0 point 1 point 2". One rule
    // for decimals AND version strings: any dot flanked by digits
    // becomes "point". No float parsing, no special cases. Thousands
    // separators (60,000) are commas and pass through untouched.
    name: "digit dots",
    bornFrom: "#2 #3",
    apply: (t) => t.replace(/(\d)\.(?=\d)/g, "$1 point "),
  },
  {
    // Remaining en/em dashes are prose dashes — the author holding two
    // clauses apart. A dash is a breath, so it becomes a comma instead
    // of vanishing and welding the clauses together.
    name: "prose dashes",
    bornFrom: "#4",
    apply: (t) => t.replace(/\s*[–—]+\s*/g, ", "),
  },
  {
    // Rewrites above can leave doubled spaces; the splitter flattens
    // whitespace anyway, but tests read better against clean output.
    name: "tidy whitespace",
    bornFrom: "hygiene",
    apply: (t) => t.replace(/ {2,}/g, " "),
  },
];

/** Grabbed text in, speakable text out. Pure, ordered, microseconds. */
export const normalize = (text: string): string =>
  RULES.reduce((t, rule) => rule.apply(t), text);
