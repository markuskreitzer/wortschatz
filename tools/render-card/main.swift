// Render one card with the native macOS renderer (macos/Sources/CardRenderer.swift) to a PNG,
// laid out exactly as WortschatzView draws a card at rest (same unit, fit, centring, colophon).
// Used for the README stills and to check specific words, since the saver itself shuffles.
//
//   B=~/Library/Caches/wortschatz-build
//   swiftc -O macos/Sources/CardRenderer.swift tools/render-card/main.swift -framework AppKit -o $B/render-card
//   cargo run --release --manifest-path engine/Cargo.toml --bin wortschatz -- card core/data/lexicon.json einladen > /tmp/card.json
//   $B/render-card core/fonts /tmp/card.json out.png 1512 982 [preview 0|1] [scale]
import AppKit

let a = CommandLine.arguments
guard a.count >= 6 else { print("usage: render-card <fontsDir> <card.json> <out.png> <w> <h> [preview 0|1] [scale]"); exit(2) }
let fonts = Fonts(directory: URL(fileURLWithPath: a[1], isDirectory: true))
let card = try JSONDecoder().decode(Card.self, from: Data(contentsOf: URL(fileURLWithPath: a[2])))
let size = CGSize(width: Double(a[4])!, height: Double(a[5])!)
let preview = a.count > 6 && a[6] == "1"
let scale = a.count > 7 ? CGFloat(Double(a[7])!) : 2
let r = CardRenderer(fonts: fonts)

let u = CardRenderer.unit(for: size, preview: preview)
var uc = u
var cw = min(82 * uc, size.width - 16 * u)
var L = r.layout(card, width: cw, u: uc)
for _ in 0..<3 where L.height > size.height - 12 * u {
    uc *= (size.height - 12 * u) / L.height
    cw = min(82 * uc, size.width - 16 * u)
    L = r.layout(card, width: cw, u: uc)
}
guard let space = CGColorSpace(name: CGColorSpace.sRGB),
      let ctx = CGContext(data: nil, width: Int(size.width * scale), height: Int(size.height * scale), bitsPerComponent: 8,
                          bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { exit(1) }
ctx.scaleBy(x: scale, y: scale)
ctx.translateBy(x: 0, y: size.height)
ctx.scaleBy(x: 1, y: -1)                       // flipped, like the view
NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
r.drawBackground(in: ctx, size: size)
if !preview { r.drawColophon(in: ctx, size: size, u: u, progress: 0.35) }
let off = CardRenderer.drift[card.serial % CardRenderer.drift.count]
ctx.saveGState()
ctx.translateBy(x: (size.width - cw) / 2 + off.0 * uc, y: max(6 * u, (size.height - L.height) / 2) + off.1 * uc)
r.draw(L, in: ctx)
ctx.restoreGState()
let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
try rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[3]))
print("\(card.id) u=\(String(format: "%.2f", u)) fitted=\(String(format: "%.2f", uc)) height=\(Int(L.height))")
