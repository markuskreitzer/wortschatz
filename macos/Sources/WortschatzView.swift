// Wortschatz: macOS ScreenSaver.framework shell.
//
// The Rust engine (engine/, a static library behind the C ABI in wortschatz.h) chooses and
// formats each card. CardRenderer draws it natively with AppKit and CoreText, using the bundled
// fonts and mirroring core/style.css. Motion runs off ScreenSaverView's animateOneFrame on a
// clock that only advances while animating, so stopAnimation really freezes it.
//
// An earlier version hosted core/index.html in a WKWebView. Inside legacyScreenSaver, WebKit
// treats that page as hidden: the WebContent process is throttled to "background" within half a
// second and suspended about three seconds later, so the first card appeared and then the
// screen went blank. Drawing natively avoids that whole process model.
import AppKit
import ScreenSaver
import os

private let log = Logger(subsystem: "local.markus.wortschatz", category: "saver")

/// Card timing, read from the bundled core/timing.js: the same file the browser renderer uses,
/// so both follow one spec. The defaults below are that file's values, used only if it is missing
/// or unreadable.
struct Timing: Decodable {
    var fade_in_ms = 2600.0, drift_in_ms = 4160.0, progress_ms = 32000.0
    var cycle_ms = 34600.0, exit_lead_ms = 2210.0, fade_out_ms = 2080.0

    static func load(_ url: URL?) -> Timing {
        guard let url, let js = try? String(contentsOf: url, encoding: .utf8),
              // Same marker as Timing::MARKER in engine/src/lib.rs, searched from the end so the
              // file's doc comment (which mentions WORTSCHATZ_TIMING) never matches.
              let a = js.range(of: "WORTSCHATZ_TIMING = {", options: .backwards), let open = js[js.index(before: a.upperBound)...].firstIndex(of: "{"),
              let close = js[open...].firstIndex(of: "}"),
              let t = try? JSONDecoder().decode(Timing.self, from: Data(js[open...close].utf8))
        else { log.error("core/timing.js missing or unreadable; using built-in timing"); return Timing() }
        return t
    }
}

@objc(WortschatzView)
final class WortschatzView: ScreenSaverView {
    // Timing (seconds) from core/timing.js, shared with the browser renderer.
    private static let timing = Timing.load(Bundle(for: WortschatzView.self)
        .url(forResource: "timing", withExtension: "js", subdirectory: "core"))
    private let hold = WortschatzView.timing.progress_ms / 1000  // the progress hairline fills over this
    private let fade = WortschatzView.timing.fade_in_ms / 1000  // opacity in
    private let driftIn = WortschatzView.timing.drift_in_ms / 1000  // drift into place
    private let fadeOut = WortschatzView.timing.fade_out_ms / 1000  // opacity out (and drift down)
    private let cycle = WortschatzView.timing.cycle_ms / 1000  // enter-to-enter
    private let enterAfterOut = WortschatzView.timing.exit_lead_ms / 1000  // out starts this long before the next enter
    private let opacityEase = CubicBezier.opacity, driftEase = CubicBezier.drift

    private var engine: OpaquePointer?
    private lazy var renderer = CardRenderer(fonts: Fonts(directory:
        Bundle(for: WortschatzView.self).resourceURL?.appendingPathComponent("core/fonts", isDirectory: true)))

    private struct Shown {
        let card: Card
        var layout: CardLayout
        var layoutSize: CGSize
        var image: CGImage? = nil           // the card rendered once, then faded/drifted as a bitmap
        var imageScale: CGFloat = 0         // backing scale `image` was rendered at
        var imagePad: CGFloat = 0
        var cardW: CGFloat = 0
        var unit: CGFloat = 0               // u for this card (smaller when it had to be fitted)
        let enteredAt: Double
        let from: CGPoint      // start offset (in u) of the entrance drift
        let to: CGPoint        // resting offset (in u)
    }
    private var current: Shown?
    private var clock: Double = 0                 // advances only while animating
    private var lastTick: CFTimeInterval?
    private var running = false
    /// Seconds of animation clock elapsed (diagnostics for tools/snapshot.swift).
    @objc var wzClock: Double { clock }

    override init?(frame: NSRect, isPreview: Bool) {
        super.init(frame: frame, isPreview: isPreview)
        commonInit()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        commonInit()
    }

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { true }

    private func commonInit() {
        animationTimeInterval = 1.0 / 30.0
        let bundle = Bundle(for: WortschatzView.self)
        if let lex = bundle.url(forResource: "lexicon", withExtension: "json", subdirectory: "core/data") {
            // Deck state lives in this (sandboxed) process's Application Support, so each
            // activation continues the round. The preview keeps its own deck.
            let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
                .appendingPathComponent("Wortschatz", isDirectory: true)
            let state = support?.appendingPathComponent(isPreview ? "deck-preview.json" : "deck.json").path
            engine = lex.path.withCString { lp in
                if let state { return state.withCString { sp in wz_engine_new(lp, sp) } }
                return wz_engine_new(lp, nil)
            }
        }
        advance()
        // macOS 14+ keeps legacyScreenSaver alive after dismissal and does not always call
        // stopAnimation; this notification is the reliable "we are done" signal.
        DistributedNotificationCenter.default().addObserver(
            self, selector: #selector(screensaverWillStop(_:)),
            name: NSNotification.Name("com.apple.screensaver.willstop"), object: nil)
    }

    deinit {
        DistributedNotificationCenter.default().removeObserver(self)
        if let engine { wz_engine_free(engine) }
    }

    // MARK: Lifecycle

    override func startAnimation() {
        super.startAnimation()
        log.notice("startAnimation preview=\(self.isPreview, privacy: .public) size=\(Int(self.bounds.width), privacy: .public)x\(Int(self.bounds.height), privacy: .public)")
        running = true
        lastTick = nil
        needsDisplay = true
    }

    override func stopAnimation() {
        super.stopAnimation()
        log.notice("stopAnimation preview=\(self.isPreview, privacy: .public)")
        running = false
        lastTick = nil
    }

    @objc private func screensaverWillStop(_ note: Notification) {
        stopAnimation()
        if !isPreview {
            // Let the host process wind down instead of drawing on in the background.
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { exit(0) }
        }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        current?.image = nil            // re-render the card bitmap at the new backing scale
        needsDisplay = true
    }

    override var hasConfigureSheet: Bool { false }
    override var configureSheet: NSWindow? { nil }

    // MARK: Timeline

    private func nextCard() -> Card? {
        guard let engine, let cstr = wz_engine_next_card_json(engine) else { return nil }
        defer { wz_string_free(cstr) }
        return try? JSONDecoder().decode(Card.self, from: Data(String(cString: cstr).utf8))
    }

    private func advance() {
        guard let card = nextCard() else { log.error("engine returned no card"); return }
        log.notice("card #\(card.serial, privacy: .public) \(card.id, privacy: .public) preview=\(self.isPreview, privacy: .public)")
        let d = CardRenderer.drift[card.serial % CardRenderer.drift.count]
        let to = CGPoint(x: d.0, y: d.1)
        // Like the CSS transition: the card drifts in from where the previous one rested
        // (plus 0.8u lower). The first card starts from its own resting place.
        let prev = current?.to ?? to
        current = Shown(card: card, layout: CardLayout(), layoutSize: .zero, enteredAt: clock,
                        from: CGPoint(x: prev.x, y: prev.y + 0.8), to: to)
    }

    override func animateOneFrame() {
        guard running else { return }
        let now = CACurrentMediaTime()
        if let last = lastTick { clock += min(0.25, max(0, now - last)) }   // no jumps after a stall
        lastTick = now
        if let c = current, clock - c.enteredAt >= cycle { advance() }
        scheduleRedraw()
    }

    private var lastProgressStep = -1
    /// Full redraws only while the card moves; during the hold just the progress hairline,
    /// a few times per second.
    private func scheduleRedraw() {
        guard let c = current else { return }
        let t = clock - c.enteredAt
        if t < max(fade, driftIn) + 0.1 || t >= cycle - enterAfterOut - 0.05 { needsDisplay = true; return }
        guard !isPreview else { return }
        let step = Int(t * 4)
        if step != lastProgressStep { lastProgressStep = step; setNeedsDisplay(colophonRect()) }
    }

    private func colophonRect() -> NSRect {
        let u = CardRenderer.unit(for: bounds.size, preview: isPreview)
        return NSRect(x: 0, y: bounds.height - 6 * u, width: bounds.width, height: 6 * u)
    }

    private func progress(_ t: Double) -> CGFloat { CGFloat(min(1, max(0, t / hold))) }

    // MARK: Drawing

    override func draw(_ rect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        let size = bounds.size
        ctx.saveGState()
        ctx.clip(to: rect)
        renderer.drawBackground(in: ctx, size: size)
        let u = CardRenderer.unit(for: size, preview: isPreview)
        defer { ctx.restoreGState() }
        guard var c = current else { return }
        let t = clock - c.enteredAt

        if !isPreview, rect.intersects(colophonRect()) {
            renderer.drawColophon(in: ctx, size: size, u: u, progress: progress(t))
        }

        // The bitmap is rendered at the backing scale, so a move between Retina and non-Retina
        // displays (or a scale change) re-renders it; see viewDidChangeBackingProperties.
        let scale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2
        if c.layoutSize != size || c.imageScale != scale || c.image == nil {
            // Same as core/app.js fit(): a card taller than the stage gets a smaller unit.
            var uc = u
            var cw = min(82 * uc, size.width - 16 * u)
            c.layout = renderer.layout(c.card, width: cw, u: uc)
            let avail = size.height - 12 * u
            for _ in 0..<3 where c.layout.height > avail {
                uc *= avail / c.layout.height
                cw = min(82 * uc, size.width - 16 * u)
                c.layout = renderer.layout(c.card, width: cw, u: uc)
            }
            c.cardW = cw
            c.unit = uc
            c.layoutSize = size
            c.imagePad = ceil(3 * u)
            c.imageScale = scale
            c.image = renderCard(c.layout, width: c.cardW, pad: c.imagePad, scale: scale)
            current = c
        }

        // Opacity and drift, as the CSS transitions: in over `fade` and `driftIn`, out over
        // `fadeOut`, starting `enterAfterOut` before the next card enters (core/timing.js).
        var opacity = opacityEase(t / fade)
        let k = CGFloat(driftEase(t / driftIn))
        var off = CGPoint(x: c.from.x + (c.to.x - c.from.x) * k, y: c.from.y + (c.to.y - c.from.y) * k)
        let outStart = cycle - enterAfterOut
        if t > outStart {
            let o = (t - outStart) / fadeOut
            opacity *= 1 - opacityEase(o)
            off.y += 0.8 * CGFloat(driftEase(o))
        }
        guard opacity > 0.001, let image = c.image else { return }

        let cardW = c.cardW, cu = c.unit
        let x = (size.width - cardW) / 2 + off.x * cu
        let y = max(6 * u, (size.height - c.layout.height) / 2) + off.y * cu
        let dst = CGRect(x: x - c.imagePad, y: y - c.imagePad,
                         width: cardW + 2 * c.imagePad, height: c.layout.height + 2 * c.imagePad)
        guard rect.intersects(dst) else { return }
        ctx.setAlpha(CGFloat(opacity))
        ctx.interpolationQuality = .high
        // The view is flipped; CGContext.draw(image) assumes y-up, so flip locally.
        ctx.translateBy(x: dst.minX, y: dst.maxY)
        ctx.scaleBy(x: 1, y: -1)
        ctx.draw(image, in: CGRect(origin: .zero, size: dst.size))
    }

    /// Render the card once into a bitmap at the display scale (transparent background).
    private func renderCard(_ layout: CardLayout, width: CGFloat, pad: CGFloat, scale: CGFloat) -> CGImage? {
        let w = width + 2 * pad, h = layout.height + 2 * pad
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let bm = CGContext(data: nil, width: Int(ceil(w * scale)), height: Int(ceil(h * scale)), bitsPerComponent: 8,
                                 bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        bm.scaleBy(x: scale, y: scale)
        bm.translateBy(x: 0, y: h)
        bm.scaleBy(x: 1, y: -1)                 // flipped, like the view
        bm.translateBy(x: pad, y: pad)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: bm, flipped: true)
        renderer.draw(layout, in: bm)
        NSGraphicsContext.restoreGraphicsState()
        return bm.makeImage()
    }
}
