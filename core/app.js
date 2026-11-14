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
  // Timing comes from core/timing.js (shared with the macOS saver). ?hold= (seconds) and ?fade=
  // (ms) still override it for quick previews; ?fade scales every transition proportionally.
  const T = Object.assign({ fade_in_ms: 2600, drift_in_ms: 4160, progress_ms: 32000, cycle_ms: 34600,
    exit_lead_ms: 2210, fade_out_ms: 2080 }, window.WORTSCHATZ_TIMING || {});
  const FADE = Number(params.get("fade") || host.fade || T.fade_in_ms);
  const kT = FADE / T.fade_in_ms;
  const HOLD = params.get("hold") || host.hold ? Math.max(5, Number(params.get("hold") || host.hold)) * 1000 : T.progress_ms;
  const CYCLE = HOLD + (T.cycle_ms - T.progress_ms) * kT;     // enter to next enter
  const EXIT_LEAD = T.exit_lead_ms * kT;                      // exit starts this long before the next enter
  const EXIT_AT = CYCLE - EXIT_LEAD;
  const root = document.documentElement;
  root.style.setProperty("--fade-in", FADE + "ms");
  root.style.setProperty("--drift-in", T.drift_in_ms * kT + "ms");
  root.style.setProperty("--fade-out", T.fade_out_ms * kT + "ms");
  if (host.preview || params.has("preview")) document.body.classList.add("preview");

  const cardEl = document.getElementById("card");
  const bar = document.getElementById("progress-bar");
  let current = null, paused = false, timer = null, shownAt = 0, remaining = HOLD, swapTimer = null;

  // ---- card formatting (mirror of engine::card_for) ----
  const POS = { noun: ["Substantiv", "noun"], verb: ["Verb", "verb"], adj: ["Adjektiv", "adjective"], adv: ["Adverb", "adverb"] };
  function regularPresent3(w) {
    const stem = /(eln|ern)$/.test(w) ? w.slice(0, -1) : w.endsWith("en") ? w.slice(0, -2) : w.endsWith("n") ? w.slice(0, -1) : w;
    const cs = Array.from(stem), n = cs.length, last = cs[n - 1], p = cs[n - 2];
    let e = last === "t" || last === "d";
    if ((last === "m" || last === "n") && n >= 2) {
      const vowel = "aeiouäöüy".includes(p);
      e = (!vowel && !"lrmnh".includes(p)) || (p === "h" && n >= 3 && cs[n - 3] === "c");
    }
    return stem + (e ? "et" : "t");
  }
  function principalParts(e) {
    const get = (k) => (e.forms && e.forms[k] && String(e.forms[k]).trim()) || null;
    const past = get("past"), participle = get("participle");
    if (!past || !participle) return null;
    const particle = e.separable || null;
    const finite = (form) => {
      const core = form.split(/\s+/).filter((t) => t && t !== "sich");
      if (particle && core.length > 1 && core[core.length - 1] === particle) core.pop();
      let out = core.join(" ");
      if (e.reflexive) out += " sich";
      if (particle) out += " " + particle;
      return out;
    };
    const parts = [];
    if (get("present3")) {
      const base = particle && e.word.startsWith(particle) ? e.word.slice(particle.length) : e.word;
      const shown = finite(get("present3"));
      if (shown.split(" ")[0] !== regularPresent3(base)) parts.push(shown);
    }
    parts.push(finite(past));
    const aux = (get("auxiliary") || "").split("/").map((a) => ({ sein: "ist", haben: "hat" })[a.trim()]).filter(Boolean).join(" / ");
    const refl = e.reflexive ? "sich " : "";
    parts.push(aux ? `${aux} ${refl}${participle}` : refl + participle);
    return parts.join(" · ");
  }
  function cardFor(e, serial) {
    const f = (k) => (e.forms && e.forms[k] && String(e.forms[k]).trim()) || null;
    const plural = e.pos === "noun" && e.number !== "plural_only" ? f("plural") : null;
    const pluralNote = e.pos !== "noun" ? null : e.number === "plural_only" ? "nur Plural" : e.number === "singular_only" && !plural ? "kein Plural" : null;
    const pp = e.pos === "verb" ? principalParts(e) : null;
    let label = null, line = null;
    if (e.pos === "noun" && plural) { label = "Plural"; line = "die " + plural; }
    if (e.pos === "verb" && pp) { label = "Stammformen"; line = pp; }
    if (e.pos === "adj" && f("comparative") && f("superlative")) { label = "Steigerung"; line = f("comparative") + " · " + f("superlative"); }
    const [pde, pen] = POS[e.pos] || [e.pos, e.pos];
    return { serial, id: e.id, word: e.word, article: e.article, gender: e.gender, pos: e.pos, pos_de: pde, pos_en: pen,
      ipa: e.ipa, syllables: e.syllables, english: e.english, definition_en: e.definition_en, definition_de: e.definition_de,
      forms_label: label, forms_line: line, number: e.number || null,
      plural_article: plural ? "die" : null, plural, plural_note: pluralNote,
      reflexive: e.pos === "verb" && !!e.reflexive, separable_prefix: e.pos === "verb" ? e.separable || null : null,
      principal_parts: pp, example_de: e.example ? e.example.de : null, example_en: e.example ? e.example.en : null,
      source: e.source || {} };
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
    const lead = card.article ? card.article : card.reflexive ? "sich" : "";
    const len = card.word.length + (lead ? 2 : 0) + (card.separable_prefix ? 1 : 0);
    return len <= 7 ? 11.5 : len <= 10 ? 10 : len <= 13 ? 8.4 : len <= 16 ? 7 : 6;
  }
  const DRIFT = [[0, 0], [-2.5, -1.5], [2.5, 1.2], [-1.5, 2], [2, -2]];
  const NBSP = "\u00a0";

  // The line after the headword: "· die Häuser", "· kein Plural", "· fährt · fuhr · ist gefahren".
  // Built from text nodes only (no innerHTML), parts joined by breakable " · ".
  function inflectionParts(card) {
    const span = (cls, text) => { const n = document.createElement("span"); if (cls) n.className = cls; n.textContent = text; return n; };
    if (card.pos === "noun") {
      if (card.plural) return [[span("pl-art", card.plural_article || "die"), document.createTextNode(NBSP + card.plural)]];
      if (card.plural_note && card.number !== "plural_only") return [[span("note", card.plural_note)]];
      return [];
    }
    const line = card.pos === "verb" ? card.principal_parts : card.pos === "adj" ? card.forms_line : null;
    return line ? line.split(" · ").map((p) => [document.createTextNode(p.replace(/ /g, NBSP))]) : [];
  }
  function fillInflection(card) {
    const el = cardEl.querySelector('[data-f="inflection"]');
    el.replaceChildren(); el.classList.remove("wrapped");
    const parts = inflectionParts(card);
    el.hidden = !parts.length;
    parts.forEach((nodes, i) => {
      const dot = document.createElement("span");
      dot.className = "dot" + (i === 0 ? " lead" : "");
      dot.textContent = i === 0 ? "·" + NBSP : " · ";
      el.append(dot, ...nodes);
    });
  }
  // After layout: if the inflection dropped below the word, hide its leading dot.
  function markWrap() {
    const el = cardEl.querySelector('[data-f="inflection"]'), w = cardEl.querySelector(".headword .word");
    if (el.hidden) return;
    el.classList.toggle("wrapped", el.offsetTop > w.offsetTop + w.offsetHeight * 0.5);
  }
  // Keep tall cards (long definitions + example on a short screen) inside the stage by
  // shrinking the card's unit, as the native renderer does.
  function baseUnit() {
    const pv = document.body.classList.contains("preview");
    return pv ? Math.min(innerWidth * 0.0115, innerHeight * 0.019) : Math.min(innerWidth * 0.01, innerHeight * 0.0165);
  }
  function fit() {
    cardEl.style.removeProperty("--u");
    markWrap();
    const u = baseUnit(), avail = innerHeight - 12 * u;
    for (let i = 0; i < 3 && cardEl.offsetHeight > avail; i++) {
      const k = avail / cardEl.offsetHeight;
      cardEl.style.setProperty("--u", (parseFloat(cardEl.style.getPropertyValue("--u")) || u) * k + "px");
      markWrap();
    }
  }

  function fill(card) {
    cardEl.className = "card" + (card.gender ? " g-" + card.gender : "") + (card.number ? " n-" + card.number : "");
    cardEl.dataset.pos = card.pos;
    const set = (k, v) => cardEl.querySelectorAll(`[data-f="${k}"]`).forEach((n) => { n.textContent = v || ""; n.hidden = !v; });
    const GENDER = { masculine: "maskulin", feminine: "feminin", neuter: "neutrum" };
    set("pos", card.gender ? `${card.pos_de} · ${GENDER[card.gender]}` : card.number === "plural_only" ? `${card.pos_de} · nur Plural`
      : (card.pos_de.toLowerCase() === card.pos_en.toLowerCase() ? card.pos_de : `${card.pos_de} · ${card.pos_en}`));
    set("article", card.article || (card.reflexive ? "sich" : null));
    const pre = card.separable_prefix && card.word.startsWith(card.separable_prefix) ? card.separable_prefix : null;
    set("prefix", pre); set("stem", pre ? card.word.slice(pre.length) : card.word);
    cardEl.querySelector(".headword .sep").hidden = !pre;
    fillInflection(card);
    set("ipa", card.ipa);
    set("syllables", card.syllables && card.syllables !== card.word ? card.syllables : null);
    set("english", card.english); set("definition_en", card.definition_en); set("definition_de", card.definition_de);
    set("example_de", card.example_de); set("example_en", card.example_de ? card.example_en : null);
    cardEl.querySelector(".example").hidden = !card.example_de;
    cardEl.style.setProperty("--word-size", wordSize(card));
    const d = DRIFT[(card.serial || 0) % DRIFT.length];
    cardEl.style.setProperty("--dx", `calc(var(--u) * ${d[0]})`);
    cardEl.style.setProperty("--dy", `calc(var(--u) * ${d[1]})`);
    cardEl.hidden = false;
    fit();
  }
  window.addEventListener("resize", () => { if (current) fit(); });
  document.fonts && document.fonts.ready.then(() => { if (current) fit(); });

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
    if (current) { cardEl.classList.remove("in"); cardEl.classList.add("out"); swapTimer = setTimeout(enter, EXIT_LEAD); }
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
    timer = setTimeout(() => { nextStandalone(); schedule(CYCLE); }, ms);
  }

  const api = {
    show,
    cardFor,
    next: () => { if (!host.driven) { nextStandalone(); schedule(CYCLE); } },
    pause() { if (paused) return; paused = true; clearTimeout(timer); remaining = Math.max(0, remaining - (performance.now() - shownAt)); freezeProgress(); document.body.classList.add("paused"); },
    resume() { if (!paused) return; paused = false; shownAt = performance.now(); startProgress(remaining); schedule(remaining + EXIT_AT - HOLD); document.body.classList.remove("paused"); },
    count: () => (lex && lex.entries ? lex.entries.length : 0),
    holdMs: HOLD,
    timing: { fadeIn: FADE, cycle: CYCLE, exitAt: EXIT_AT, exitLead: EXIT_LEAD },
  };
  window.Wortschatz = api;

  document.fonts && document.fonts.ready.then(() => document.body.classList.add("fonts-ready"));
  if (!host.driven) {
    nextStandalone(); schedule(EXIT_AT);
    window.addEventListener("keydown", (ev) => { if (ev.key === "ArrowRight" || ev.key === " ") { ev.preventDefault(); api.next(); } });
    document.addEventListener("visibilitychange", () => (document.hidden ? api.pause() : api.resume()));
  }
})();
