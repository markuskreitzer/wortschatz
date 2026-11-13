/* Wortschatz renderer. Platform-agnostic: no macOS/Linux APIs.
 *
 * Two ways to drive it:
 *  1. Host-driven (macOS .saver, future Linux runner): the native shell links the Rust engine
 *     (engine/), picks the next card and calls  window.Wortschatz.show(cardJson).
 *     The host sets  window.WORTSCHATZ_HOST = { driven: true, preview: bool, hold: seconds }
 *     before this script runs, and calls Wortschatz.pause()/resume() on stop/start.
 *  2. Standalone (open index.html in any browser): this file shuffles data/lexicon.js itself,
 *     mirroring engine/src/lib.rs (same shuffle-bag rules, same card formatting).
 */
(function () {
  "use strict";
  const host = window.WORTSCHATZ_HOST || {};
  const params = new URLSearchParams(location.search);
  const HOLD = Math.max(5, Number(params.get("hold") || host.hold || 32)) * 1000;
  const FADE = Number(params.get("fade") || host.fade || 2600);
  const root = document.documentElement;
  root.style.setProperty("--fade", FADE + "ms");
  if (host.preview || params.has("preview")) document.body.classList.add("preview");

  const cardEl = document.getElementById("card");
  const bar = document.getElementById("progress-bar");
  let current = null, paused = false, timer = null, shownAt = 0, remaining = HOLD, swapTimer = null;

  // ---- card formatting (mirror of engine::card_for) ----
  const POS = { noun: ["Substantiv", "noun"], verb: ["Verb", "verb"], adj: ["Adjektiv", "adjective"], adv: ["Adverb", "adverb"] };
  function cardFor(e, serial) {
    const f = (k) => (e.forms && e.forms[k] && String(e.forms[k]).trim()) || null;
    let label = null, line = null;
    if (e.pos === "noun" && f("plural")) { label = "Plural"; line = "die " + f("plural"); }
    if (e.pos === "verb") {
      const parts = [];
      if (f("present3")) parts.push("er " + f("present3"));
      if (f("past")) parts.push(f("past"));
      if (f("participle")) {
        const aux = (f("auxiliary") || "").split("/").map((a) => ({ haben: "hat", sein: "ist" })[a.trim()] || "").filter(Boolean).join("/");
        parts.push(aux ? aux + " " + f("participle") : f("participle"));
      }
      if (parts.length) { label = "Konjugation"; line = parts.join(" · "); }
    }
    if (e.pos === "adj" && f("comparative") && f("superlative")) { label = "Steigerung"; line = f("comparative") + " · " + f("superlative"); }
    const [pde, pen] = POS[e.pos] || [e.pos, e.pos];
    return { serial, id: e.id, word: e.word, article: e.article, gender: e.gender, pos: e.pos, pos_de: pde, pos_en: pen,
      ipa: e.ipa, syllables: e.syllables, english: e.english, definition_en: e.definition_en, definition_de: e.definition_de,
      forms_label: label, forms_line: line, source: e.source || {} };
  }

  // ---- shuffle bag (mirror of engine::Deck) ----
  function makeDeck(n) {
    const RECENT = 8; let order = [], pos = 0, recent = [], serial = 0;
    const rnd = (k) => Math.floor(Math.random() * k);
    function reshuffle() {
      order = Array.from({ length: n }, (_, i) => i);
      for (let i = n - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
      const guard = Math.min(RECENT, Math.floor((n - 1) / 2));
      for (let s = 0; s < guard; s++) if (recent.includes(order[s])) {
        for (let k = guard; k < n; k++) if (!recent.includes(order[k])) { [order[s], order[k]] = [order[k], order[s]]; break; }
      }
      pos = 0;
    }
    reshuffle();
    return { next() { if (pos >= order.length) reshuffle(); const i = order[pos++]; serial++; recent.push(i); if (recent.length > RECENT) recent.shift(); return [i, serial]; } };
  }

  // ---- rendering ----
  function wordSize(card) {
    const len = card.word.length + (card.article ? 2 : 0);
    return len <= 7 ? 11.5 : len <= 10 ? 10 : len <= 13 ? 8.4 : len <= 16 ? 7 : 6;
  }
  const DRIFT = [[0, 0], [-2.5, -1.5], [2.5, 1.2], [-1.5, 2], [2, -2]];

  function fill(card) {
    cardEl.className = "card" + (card.gender ? " g-" + card.gender : "");
    cardEl.dataset.pos = card.pos;
    const set = (k, v) => cardEl.querySelectorAll(`[data-f="${k}"]`).forEach((n) => { n.textContent = v || ""; n.hidden = !v; });
    set("pos", card.gender ? `${card.pos_de} · ${({ masculine: "maskulin", feminine: "feminin", neuter: "neutrum" })[card.gender]}` : (card.pos_de.toLowerCase() === card.pos_en.toLowerCase() ? card.pos_de : `${card.pos_de} · ${card.pos_en}`));
    set("article", card.article); set("word", card.word); set("ipa", card.ipa);
    set("syllables", card.syllables && card.syllables !== card.word ? card.syllables : null);
    set("english", card.english); set("definition_en", card.definition_en); set("definition_de", card.definition_de);
    set("forms_label", card.forms_line ? card.forms_label : null); set("forms_line", card.forms_line);
    cardEl.style.setProperty("--word-size", wordSize(card));
    const d = DRIFT[(card.serial || 0) % DRIFT.length];
    cardEl.style.setProperty("--dx", `calc(var(--u) * ${d[0]})`);
    cardEl.style.setProperty("--dy", `calc(var(--u) * ${d[1]})`);
    cardEl.hidden = false;
  }

  function startProgress(ms) {
    bar.style.transition = "none"; bar.style.transform = `scaleX(${1 - ms / HOLD})`;
    void bar.offsetWidth;
    bar.style.transition = `transform ${ms}ms linear`; bar.style.transform = "scaleX(1)";
  }
  function freezeProgress() {
    const m = getComputedStyle(bar).transform; bar.style.transition = "none"; bar.style.transform = m === "none" ? "scaleX(0)" : m;
  }

  // WebKit freezes CSS transitions while the page is "hidden" (an off-screen or occluded host
  // window, which can include the System Settings thumbnail). In that state, show cards without
  // transitions so they are never stuck invisible; fades resume once the page is visible.
  const syncVisibility = () => document.body.classList.toggle("instant", document.visibilityState === "hidden");
  document.addEventListener("visibilitychange", syncVisibility);
  syncVisibility();

  function show(card) {
    if (typeof card === "string") card = JSON.parse(card);
    clearTimeout(swapTimer);
    const enter = () => {
      fill(card); current = card;
      // Force a style flush, then add the class on a timer. Not requestAnimationFrame: rAF is
      // throttled when the host window is off-screen or occluded (System Settings thumbnail),
      // which would leave the card invisible.
      syncVisibility();
      void cardEl.offsetWidth;
      setTimeout(() => cardEl.classList.add("in"), 40);
      shownAt = performance.now(); remaining = HOLD;
      if (!paused) startProgress(HOLD);
      document.title = "Wortschatz — " + (card.article ? card.article + " " : "") + card.word;
    };
    if (current) { cardEl.classList.remove("in"); cardEl.classList.add("out"); swapTimer = setTimeout(enter, FADE * 0.85); }
    else enter();
  }

  // ---- standalone driver ----
  const lex = window.WORTSCHATZ_LEXICON;
  let deck = null;
  function nextStandalone() {
    if (!lex || !lex.entries || !lex.entries.length) return;
    deck = deck || makeDeck(lex.entries.length);
    const pin = params.get("id");
    if (pin && !current) { const e = lex.entries.find((x) => x.id === pin || x.word === pin); if (e) return show(cardFor(e, 1)); }
    const [i, serial] = deck.next();
    show(cardFor(lex.entries[i], serial));
  }
  function schedule(ms) {
    clearTimeout(timer);
    if (host.driven || params.has("still")) return;
    timer = setTimeout(() => { nextStandalone(); schedule(HOLD + FADE); }, ms);
  }

  const api = {
    show,
    next: () => { if (!host.driven) { nextStandalone(); schedule(HOLD + FADE); } },
    pause() { if (paused) return; paused = true; clearTimeout(timer); remaining = Math.max(0, remaining - (performance.now() - shownAt)); freezeProgress(); document.body.classList.add("paused"); },
    resume() { if (!paused) return; paused = false; shownAt = performance.now(); startProgress(remaining); schedule(remaining); document.body.classList.remove("paused"); },
    count: () => (lex && lex.entries ? lex.entries.length : 0),
    holdMs: HOLD,
  };
  window.Wortschatz = api;

  document.fonts && document.fonts.ready.then(() => document.body.classList.add("fonts-ready"));
  if (!host.driven) {
    nextStandalone(); schedule(HOLD + FADE);
    window.addEventListener("keydown", (ev) => { if (ev.key === "ArrowRight" || ev.key === " ") { ev.preventDefault(); api.next(); } });
    document.addEventListener("visibilitychange", () => (document.hidden ? api.pause() : api.resume()));
  }
})();
