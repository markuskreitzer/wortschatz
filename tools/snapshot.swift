// Dev tool: load a built Wortschatz.saver exactly like the screen saver host does (NSBundle
// principal class -> ScreenSaverView(frame:isPreview:)), let it animate in a real (off-screen)
// window on its own timer, and write PNGs at the requested times.
//   snapshot <path.saver> <out-prefix> <preview 0|1> <w> <h> <t1> [t2 ...]
// writes <out-prefix>-<t>s.png for every t (seconds after startAnimation).
import AppKit
import ScreenSaver

let a = CommandLine.arguments
guard a.count >= 7 else { print("usage: snapshot <saver> <prefix> <preview> <w> <h> <t...>"); exit(2) }
let (saverPath, prefix, preview) = (a[1], a[2], a[3] == "1")
let (w, h) = (Double(a[4])!, Double(a[5])!)
let times = a[6...].compactMap(Double.init).sorted()

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
guard let bundle = Bundle(path: saverPath), bundle.load(), let cls = bundle.principalClass as? ScreenSaverView.Type else {
    print("FAIL: could not load principal class"); exit(1)
}
let frame = NSRect(x: 0, y: 0, width: w, height: h)
let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
window.setFrameOrigin(NSPoint(x: -30000, y: -30000))
guard let view = cls.init(frame: frame, isPreview: preview) else { print("FAIL: init returned nil"); exit(1) }
window.contentView = view
window.orderFrontRegardless()
view.startAnimation()
print("loaded \(cls) preview=\(preview) animating=\(view.isAnimating) interval=\(view.animationTimeInterval)")

let start = Date()
for t in times {
    DispatchQueue.main.asyncAfter(deadline: .now() + t) {
        guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { print("FAIL: rep"); exit(1) }
        view.cacheDisplay(in: view.bounds, to: rep)
        let path = "\(prefix)-\(String(format: "%g", t))s.png"
        try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
        let clock = (view.value(forKey: "wzClock") as? Double) ?? -1
        print(String(format: "t=%.1fs clock=%.2fs wrote %@", Date().timeIntervalSince(start), clock, path))
        if t == times.last {
            view.stopAnimation()
            print("animating-after-stop=\(view.isAnimating)")
            exit(0)
        }
    }
}
app.run()
