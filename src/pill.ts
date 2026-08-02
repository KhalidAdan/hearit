// The pill: hearit's one piece of visible UI, allowed by the north star
// because a thing that makes sound owes you its off switch. The dot-matrix
// grid is content, not decoration — Rust runs an FFT over the samples
// actually leaving the speaker and emits `viz_heights` (32 quantized band
// heights) ~30 times a second; this file only flips data attributes.
//
// Rendering strategy from the spectrum-grid-dom prototype: each column
// carries data-h (its quantized height) and static generated CSS decides
// which cells light up. Per frame we write at most 32 attributes, and only
// for columns whose height actually changed — the naive version touches
// all 192 cells and re-rasterizes a gradient+shadow layer for each.

import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";

// Must match BANDS/ROWS in src-tauri/src/speak.rs — the grid and the FFT
// are two halves of one instrument.
const ROWS = 6;
const BANDS = 32;

const viz = document.getElementById("viz")!;
const bandEls: HTMLElement[] = [];
const heights: number[] = new Array(BANDS).fill(0);

function buildDOM() {
  for (let b = 0; b < BANDS; b++) {
    const band = document.createElement("div");
    band.className = "band";
    band.dataset.h = "0";
    for (let r = 0; r < ROWS; r++) {
      const cell = document.createElement("div");
      cell.className = "cell";
      band.appendChild(cell);
    }
    viz.appendChild(band);
    bandEls.push(band);
  }
}

function buildHeightRules() {
  const out: string[] = [];
  for (let h = 1; h <= ROWS; h++) {
    if (h > 1) {
      out.push(
        `.spectrum .band[data-h="${h}"] .cell:nth-child(-n+${h - 1})` +
          `{background:var(--body-bg);box-shadow:var(--body-fx)}`,
      );
    }
    out.push(
      `.spectrum .band[data-h="${h}"] .cell:nth-child(${h})` +
        `{background:var(--cap-bg);box-shadow:var(--cap-fx)}`,
    );
  }
  document.getElementById("genstyle")!.textContent = out.join("\n");
}

void listen<number[]>("viz_heights", (e) => {
  for (let b = 0; b < BANDS; b++) {
    const h = e.payload[b] ?? 0;
    if (h !== heights[b]) {
      heights[b] = h;
      bandEls[b].dataset.h = String(h);
    }
  }
});

document.getElementById("stop")!.addEventListener("click", () => {
  // Belt and suspenders: silence the sink NOW (straight to Rust), then
  // tell the coordinator so its sentence loop stops feeding the queue.
  void invoke("speak_stop");
  void emit("pill_stop");
});

buildDOM();
buildHeightRules();
