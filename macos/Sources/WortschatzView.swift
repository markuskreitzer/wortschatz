// Wortschatz — macOS ScreenSaver.framework shell.
//
// Thin host: a ScreenSaverView with a WKWebView that renders the shared core (core/index.html,
// bundled under Contents/Resources/core). Card selection comes from the Rust engine
// (engine/, linked as a static library through the C ABI in wortschatz.h). Pacing is tied to
// ScreenSaverView's animation callbacks so stopAnimation really stops everything.
import AppKit
import ScreenSaver
import WebKit

@objc(WortschatzView)
final class WortschatzView: ScreenSaverView, WKNavigationDelegate {
    private var webView: WKWebView?
    private var engine: OpaquePointer?
    private var pageReady = false
    private var lastAdvance: Date = .distantPast
    private var running = false
    private let hold: TimeInterval = 32      // seconds a card stays up
    private let fadeMs = 2600                // crossfade (handled in CSS)

    override init?(frame: NSRect, isPreview: Bool) {
        super.init(frame: frame, isPreview: isPreview)
        commonInit()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        commonInit()
    }

    private var bundle: Bundle { Bundle(for: WortschatzView.self) }

    private func commonInit() {
        animationTimeInterval = 0.5
        wantsLayer = true
        layer?.backgroundColor = NSColor(calibratedRed: 0.051, green: 0.047, blue: 0.043, alpha: 1).cgColor

        // Rust engine: lexicon from the bundle, deck state persisted in this process's
        // (sandboxed) Application Support so each activation continues the round.
        if let lex = bundle.url(forResource: "lexicon", withExtension: "json", subdirectory: "core/data") {
            let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
                .appendingPathComponent("Wortschatz", isDirectory: true)
            let state = support?.appendingPathComponent(isPreview ? "deck-preview.json" : "deck.json").path
            engine = lex.path.withCString { lp in
                if let state { return state.withCString { sp in wz_engine_new(lp, sp) } }
                return wz_engine_new(lp, nil)
            }
        }

        let config = WKWebViewConfiguration()
        config.suppressesIncrementalRendering = true
        config.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        config.mediaTypesRequiringUserActionForPlayback = .all
        let hostScript = """
        window.WORTSCHATZ_HOST = { platform: "macos", driven: \(engine != nil), preview: \(isPreview), hold: \(Int(hold)), fade: \(fadeMs) };
        """
        config.userContentController.addUserScript(
            WKUserScript(source: hostScript, injectionTime: .atDocumentStart, forMainFrameOnly: true))

        let wv = WKWebView(frame: bounds, configuration: config)
        wv.autoresizingMask = [.width, .height]
        wv.navigationDelegate = self
        wv.setValue(false, forKey: "drawsBackground")   // no white flash before first paint
        wv.allowsMagnification = false
        addSubview(wv)
        webView = wv

        if let index = bundle.url(forResource: "index", withExtension: "html", subdirectory: "core") {
            wv.loadFileURL(index, allowingReadAccessTo: index.deletingLastPathComponent())
        }

        // Sonoma+ keeps legacyScreenSaver alive after the saver is dismissed and does not always
        // call stopAnimation; this notification is the reliable "we are done" signal.
        DistributedNotificationCenter.default().addObserver(
            self, selector: #selector(screensaverWillStop(_:)),
            name: NSNotification.Name("com.apple.screensaver.willstop"), object: nil)
    }

    deinit {
        DistributedNotificationCenter.default().removeObserver(self)
        if let engine { wz_engine_free(engine) }
    }

    // MARK: ScreenSaverView lifecycle

    override func startAnimation() {
        super.startAnimation()
        running = true
        js("window.Wortschatz && Wortschatz.resume()")
        if pageReady && lastAdvance == .distantPast { advance() }
    }

    override func stopAnimation() {
        super.stopAnimation()
        running = false
        js("window.Wortschatz && Wortschatz.pause()")
    }

    @objc private func screensaverWillStop(_ note: Notification) {
        stopAnimation()
        if !isPreview {
            // Let the process wind down cleanly instead of rendering in the background.
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { exit(0) }
        }
    }

    override func animateOneFrame() {
        guard running, pageReady else { return }
        if Date().timeIntervalSince(lastAdvance) >= hold + Double(fadeMs) / 1000 { advance() }
    }

    override func draw(_ rect: NSRect) {
        NSColor(calibratedRed: 0.051, green: 0.047, blue: 0.043, alpha: 1).setFill()
        rect.fill()
    }

    override var hasConfigureSheet: Bool { false }
    override var configureSheet: NSWindow? { nil }

    // MARK: Engine -> WebView

    private func advance() {
        lastAdvance = Date()
        guard let engine, let cstr = wz_engine_next_card_json(engine) else { return }
        let json = String(cString: cstr)
        wz_string_free(cstr)
        js("window.Wortschatz && Wortschatz.show(\(json))")
    }

    private func js(_ source: String) {
        webView?.evaluateJavaScript(source, completionHandler: nil)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        pageReady = true
        if engine != nil { advance() }
        if !running && !isPreview { js("window.Wortschatz && Wortschatz.pause()") }
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        pageReady = false
        lastAdvance = .distantPast
        webView.reload()
    }
}
