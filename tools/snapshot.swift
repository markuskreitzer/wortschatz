// Dev tool: load the built Wortschatz.saver exactly like the screen saver host does
// (NSBundle principal class -> ScreenSaverView(frame:isPreview:)), run it for a few seconds and
// write a PNG of what it renders. Usage: snapshot <path.saver> <out.png> [preview 0|1] [w] [h] [finish]
import AppKit
import ScreenSaver
import WebKit

let args = CommandLine.arguments
let saverPath = args[1], outPath = args[2]
let preview = args.count > 3 ? args[3] == "1" : false
let w = args.count > 4 ? Double(args[4])! : 1512, h = args.count > 5 ? Double(args[5])! : 982

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
guard let bundle = Bundle(path: saverPath), bundle.load(), let cls = bundle.principalClass as? ScreenSaverView.Type else {
    print("FAIL: could not load principal class"); exit(1)
}
let frame = NSRect(x: 0, y: 0, width: w, height: h)
let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
window.setFrameOrigin(NSPoint(x: -20000, y: -20000))   // off-screen
guard let view = cls.init(frame: frame, isPreview: preview) else { print("FAIL: init returned nil"); exit(1) }
window.contentView = view
window.orderFrontRegardless()
view.startAnimation()
print("loaded \(cls) preview=\(preview) animating=\(view.isAnimating)")


let finish = args.count > 6 && args[6] == "finish"
// Off-screen windows are visibilityState=hidden to WebKit, which freezes CSS transitions; with
// "finish" the snapshot disables transitions (dev only; the real saver window is on screen).
func web() -> WKWebView {
    guard let w = view.subviews.compactMap({ $0 as? WKWebView }).first else { print("FAIL: no WKWebView"); exit(1) }
    return w
}
DispatchQueue.main.asyncAfter(deadline: .now() + 5) {
    if finish {
        web().evaluateJavaScript("(()=>{const s=document.createElement('style');s.textContent='*{transition:none!important}';document.head.appendChild(s);return 1})()") { _, _ in }
    }
}
DispatchQueue.main.asyncAfter(deadline: .now() + 6.5) {
    let probe = "document.title + ' | ' + document.getElementById('card').className + ' | fonts:' + document.fonts.status + ' | vis:' + document.visibilityState + ' | opacity:' + getComputedStyle(document.getElementById('card')).opacity"
    web().evaluateJavaScript(probe) { r, e in
        print("page:", r ?? "nil", e.map { "\($0)" } ?? "")
        web().takeSnapshot(with: nil) { img, err in
            guard let img, let tiff = img.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
                  let png = rep.representation(using: .png, properties: [:]) else { print("FAIL: snapshot \(String(describing: err))"); exit(1) }
            try! png.write(to: URL(fileURLWithPath: outPath))
            view.stopAnimation()
            print("OK wrote \(outPath) animating-after-stop=\(view.isAnimating)")
            exit(0)
        }
    }
}
app.run()
