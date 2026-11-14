// Deterministic frame capture of the real core/ renderer (headless Chrome).
// After load, page timers and CSS transitions are driven by a manual clock, so every frame is
// exactly 1/FPS apart regardless of how long screenshots take.
//   npm i puppeteer-core   (any Chrome/Chromium; set CHROME=/path/to/chrome)
//   node tools/capture-animation.js <outdir> <w> <h> <fps> [dpr] [png|jpg]
// See tools/make-readme-media.sh for the frames -> GIF/MP4 step.
const puppeteer = require('puppeteer-core');
const fs = require('fs');
const path = require('path');
const [out, W, H, FPS, DPR, FMT] = [process.argv[2], +process.argv[3], +process.argv[4], +process.argv[5], +(process.argv[6] || 1), process.argv[7] || 'png'];
const SEED = 155338; // deck order after the pinned first card: einladen, gut, Kleidung
const URL = 'file://' + path.resolve(__dirname, '../core/index.html') + '?id=Haus%23noun&still&hold=7';
(async () => {
  fs.mkdirSync(out, { recursive: true });
  const b = await puppeteer.launch({ executablePath: process.env.CHROME || '/usr/bin/google-chrome', headless: 'new',
    args: ['--no-sandbox', '--allow-file-access-from-files', '--hide-scrollbars', '--force-color-profile=srgb'] });
  const p = await b.newPage();
  await p.setViewport({ width: W, height: H, deviceScaleFactor: DPR });
  await p.evaluateOnNewDocument((seed) => {
    let a = seed; Math.random = function () { a |= 0; a = a + 0x6D2B79F5 | 0; let t = Math.imul(a ^ a >>> 15, 1 | a); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296; };
  }, SEED);
  await p.goto(URL, { waitUntil: 'load' });
  await p.evaluate(() => document.fonts.ready);
  await new Promise(r => setTimeout(r, 5500));
  await p.evaluate(() => {
    const vt = window.__vt = { T: 0, timers: [], id: 1, seen: new Map() };
    window.setTimeout = (fn, ms) => { const id = vt.id++; vt.timers.push({ id, at: vt.T + (ms || 0), fn }); return id; };
    window.clearTimeout = (id) => { vt.timers = vt.timers.filter(t => t.id !== id); };
    performance.now = () => vt.T;
    vt.sync = () => { for (const a of document.getAnimations()) { if (!vt.seen.has(a)) vt.seen.set(a, vt.T); a.pause(); a.currentTime = Math.max(0, vt.T - vt.seen.get(a)); } };
    vt.tick = (dt) => {
      const end = vt.T + dt;
      for (;;) { vt.timers.sort((x, y) => x.at - y.at); const t = vt.timers[0]; if (!t || t.at > end) break;
        vt.timers.shift(); vt.T = t.at; vt.sync(); t.fn(); vt.sync(); }
      vt.T = end; vt.sync();
    };
    // Replay the first card's entrance exactly as show() does: no .in, style flush, .in after 40 ms.
    const el = document.getElementById('card'); el.style.transition = 'none'; el.classList.remove('in');
    void el.offsetWidth; el.style.transition = ''; void el.offsetWidth; setTimeout(() => el.classList.add('in'), 40);
    Wortschatz.pause(); Wortschatz.resume();   // restart the progress hairline with the manual clock
  });
  const dt = 1000 / FPS; let f = 0;
  const shot = async () => { await p.screenshot({ path: `${out}/f${String(f++).padStart(4, '0')}.${FMT}`, type: FMT === 'jpg' ? 'jpeg' : 'png', quality: FMT === 'jpg' ? 95 : undefined }); };
  const run = async (ms) => { for (let e = 0; e < ms - 1e-6; e += dt) { await p.evaluate((d) => __vt.tick(d), dt); await shot(); } };
  const probe = () => p.evaluate(() => document.title + ' op=' + (+getComputedStyle(document.getElementById('card')).opacity).toFixed(2));
  await shot();
  await run(1000); console.log('1s', await probe());
  await run(5000); console.log('6s', await probe());
  for (let k = 0; k < 2; k++) {
    await p.evaluate(() => Wortschatz.next());
    await run(1000); console.log('out', await probe());
    await run(6000); console.log('in', await probe());
  }
  await p.evaluate(() => Wortschatz.next());
  await run(2160); console.log('end', await probe());
  console.log('frames', f);
  await b.close();
})().catch(e => { console.error(e); process.exit(1); });
