#!/usr/bin/env node
// Checks that core/app.js formats every card exactly like the Rust engine.
//
// core/app.js keeps a JavaScript copy of engine::card_for so the page also works standalone in a
// browser. This script loads app.js against a minimal fake DOM, formats every lexicon entry with
// its cardFor(), and compares the result field by field with `wortschatz cards`.
//
//   node tools/check-parity.mjs            (from the repo root; builds the CLI with cargo)
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const root = fileURLToPath(new URL("..", import.meta.url));
const lexPath = root + "core/data/lexicon.json";
const lexicon = JSON.parse(readFileSync(lexPath, "utf8"));

// Just enough DOM for app.js to load in host-driven mode (no cards are drawn).
const el = () => new Proxy(function () {}, {
  get: (t, k) => (k === Symbol.toPrimitive ? () => "" : k === "classList" || k === "style" ? el() : el()),
  apply: () => el(),
});
const document = { documentElement: el(), body: el(), getElementById: el(), addEventListener() {}, visibilityState: "visible" };
const window = { WORTSCHATZ_HOST: { driven: true }, addEventListener() {} };
const ctx = { window, document, location: { search: "" }, URLSearchParams, getComputedStyle: el(), performance, setTimeout, clearTimeout };
vm.createContext(ctx);
vm.runInContext(readFileSync(root + "core/app.js", "utf8"), ctx);
const { cardFor } = window.Wortschatz;

const out = execFileSync("cargo", ["run", "--quiet", "--release", "-p", "wortschatz-core", "--bin", "wortschatz", "--", "cards", lexPath],
  { cwd: root, encoding: "utf8", maxBuffer: 64 << 20, stdio: ["ignore", "pipe", "inherit"] });
const rust = out.trim().split("\n").map((l) => JSON.parse(l));

// JSON round trip so `undefined` and missing keys compare like Rust's `null`.
const norm = (c) => Object.fromEntries(Object.entries(JSON.parse(JSON.stringify(c))).map(([k, v]) => [k, v ?? null]).sort());
let bad = 0;
if (rust.length !== lexicon.entries.length) {
  console.error(`engine printed ${rust.length} cards for ${lexicon.entries.length} entries`);
  process.exit(1);
}
lexicon.entries.forEach((e, i) => {
  const r = norm(rust[i]), j = norm(cardFor(e, 1));
  for (const k of new Set([...Object.keys(r), ...Object.keys(j)])) {
    if (JSON.stringify(r[k] ?? null) !== JSON.stringify(j[k] ?? null)) {
      if (bad++ < 20) console.error(`${e.id}: ${k}\n  rust: ${JSON.stringify(r[k])}\n  js:   ${JSON.stringify(j[k])}`);
    }
  }
});
console.log(`${lexicon.entries.length} cards compared, ${bad} differences`);
process.exit(bad ? 1 : 0);
