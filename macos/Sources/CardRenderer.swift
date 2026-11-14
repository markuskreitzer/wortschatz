// Wortschatz: native card renderer for macOS (AppKit + CoreText).
//
// It mirrors core/style.css: the same unit system, fonts, colours, spacing and motion, so the
// macOS saver looks like the browser preview. It does not use a WebView, because inside
// legacyScreenSaver WebKit treats the page as hidden and suspends its WebContent process a few
// seconds after start.
import AppKit
import CoreText

struct Card: Decodable {
    let serial: Int
    let id: String
    let word: String
    let article: String?
    let gender: String?
    let pos: String
    let pos_de: String
    let pos_en: String
    let ipa: String?
    let syllables: String?
    let english: String?
    let definition_en: String?
    let definition_de: String?
    let forms_label: String?
    let forms_line: String?
    // Grammar (engine::Card): plural / number for nouns, principal parts for verbs.
    let number: String?
    let plural_article: String?
    let plural: String?
    let plural_note: String?
    let reflexive: Bool?
    let separable_prefix: String?
    let principal_parts: String?
    let example_de: String?
    let example_en: String?
}

// MARK: - Palette (core/style.css :root)

enum Palette {
    static func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> NSColor {
        NSColor(srgbRed: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255,
                blue: CGFloat(hex & 0xff) / 255, alpha: a)
    }
    static let bg = rgb(0x0d0c0b)
    static let ink = rgb(0xece6dc)
    static let ink2 = rgb(0xb9b1a4)
    static let ink3 = rgb(0x7d766c)
    static let hair = rgb(0xece6dc, 0.14)
    static let plural = rgb(0xd4a26a)       // plural "die": ochre, never read as feminine
    static func accent(_ gender: String?, number: String? = nil) -> NSColor {
        if gender == nil && number == "plural_only" { return plural }
        switch gender {
        case "masculine": return rgb(0x8fa9c9)
        case "feminine": return rgb(0xcf9a8e)
        case "neuter": return rgb(0x9fb894)
        default: return rgb(0xc8b48a)
        }
    }
}

// MARK: - Fonts (bundled WOFF2, loaded per process; nothing is installed system-wide)

final class Fonts {
    enum Face: String { case display = "CormorantGaramond", displayItalic = "CormorantGaramond-Italic",
                        text = "SourceSerif4", textItalic = "SourceSerif4-Italic", sans = "Inter", ipa = "GentiumPlus-Regular" }
    private var descriptors: [Face: CTFontDescriptor] = [:]
    private var cache: [String: NSFont] = [:]

    init(directory: URL?) {
        for face in [Face.display, .displayItalic, .text, .textItalic, .sans, .ipa] {
            guard let url = directory?.appendingPathComponent(face.rawValue + ".woff2"),
                  let data = try? Data(contentsOf: url),
                  let list = CTFontManagerCreateFontDescriptorsFromData(data as CFData) as? [CTFontDescriptor],
                  let first = list.first else { continue }
            descriptors[face] = first
        }
    }

    /// A font at `size` points with explicit variation axes (CSS font-weight and font-variation-settings).
    func font(_ face: Face, _ size: CGFloat, weight: CGFloat = 400, opsz: CGFloat? = nil) -> NSFont {
        let key = "\(face.rawValue)/\(size)/\(weight)/\(opsz ?? -1)"
        if let f = cache[key] { return f }
        var result: NSFont
        if let d = descriptors[face] {
            let base = CTFontCreateWithFontDescriptor(d, size, nil)
            var variation: [NSNumber: NSNumber] = [:]
            let axes = (CTFontCopyVariationAxes(base) as? [[String: Any]]) ?? []
            func has(_ tag: UInt32) -> Bool { axes.contains { ($0[kCTFontVariationAxisIdentifierKey as String] as? NSNumber)?.uint32Value == tag } }
            let wght: UInt32 = 0x7767_6874, opszTag: UInt32 = 0x6f70_737a
            if has(wght) { variation[NSNumber(value: wght)] = NSNumber(value: Double(weight)) }
            if has(opszTag) {
                // CSS font-optical-sizing: auto uses the font size in px, clamped by the font.
                variation[NSNumber(value: opszTag)] = NSNumber(value: Double(opsz ?? size))
            }
            // Apply the variation to the descriptor, not to a font copy: when a requested value
            // equals the axis default, CTFontCreateCopyWithAttributes drops it and the named
            // instance's coordinates (e.g. Inter Thin) win.
            let attrs = [kCTFontVariationAttribute as String: variation] as CFDictionary
            result = CTFontCreateWithFontDescriptor(CTFontDescriptorCreateCopyWithAttributes(d, attrs), size, nil) as NSFont
        } else {
            let fallback: NSFont?
            switch face {
            case .display, .text, .ipa: fallback = NSFont(name: "Iowan Old Style", size: size)
            case .displayItalic, .textItalic: fallback = NSFont(name: "IowanOldStyle-Italic", size: size)
            case .sans: fallback = NSFont.systemFont(ofSize: size)
            }
            result = fallback ?? NSFont.systemFont(ofSize: size)
        }
        cache[key] = result
        return result
    }
}

// MARK: - Easing (CSS cubic-bezier)

struct CubicBezier {
    let x1, y1, x2, y2: Double
    func callAsFunction(_ t: Double) -> Double {
        if t <= 0 { return 0 }
        if t >= 1 { return 1 }
        func bx(_ s: Double) -> Double { 3 * (1 - s) * (1 - s) * s * x1 + 3 * (1 - s) * s * s * x2 + s * s * s }
        func by(_ s: Double) -> Double { 3 * (1 - s) * (1 - s) * s * y1 + 3 * (1 - s) * s * s * y2 + s * s * s }
        var lo = 0.0, hi = 1.0, s = t
        for _ in 0..<40 { s = (lo + hi) / 2; if bx(s) < t { lo = s } else { hi = s } }
        return by(s)
    }
    static let opacity = CubicBezier(x1: 0.4, y1: 0, x2: 0.2, y2: 1)
    static let drift = CubicBezier(x1: 0.2, y1: 0.7, x2: 0.2, y2: 1)
}

// MARK: - Layout

private extension NSAttributedString.Key {
    static let ctLanguage = NSAttributedString.Key(kCTLanguageAttributeName as String)
}

/// A piece of text placed at a baseline (single line) or in a rect (wrapping paragraph).
struct TextItem {
    let string: NSAttributedString
    let origin: CGPoint      // single line: baseline-left; paragraph: top-left of the rect
    let width: CGFloat?      // nil = single line
    func draw() {
        if let width {
            string.draw(with: CGRect(x: origin.x, y: origin.y, width: width, height: 10_000),
                        options: [.usesLineFragmentOrigin, .usesFontLeading])
        } else {
            let font = string.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
            // NSAttributedString.draw(at:) takes the top-left of the line box in a flipped view.
            string.draw(at: CGPoint(x: origin.x, y: origin.y - (font.map { $0.ascender } ?? 0)))
        }
    }
}

struct CardLayout {
    var texts: [TextItem] = []
    var lines: [(CGRect, NSColor)] = []                 // solid hairlines
    var gradientRule: (CGRect, NSColor)? = nil          // meta rule: accent -> hair -> transparent
    var englishDash: (CGRect, NSColor)? = nil
    var height: CGFloat = 0
}

final class CardRenderer {
    let fonts: Fonts
    init(fonts: Fonts) { self.fonts = fonts }

    static func unit(for size: CGSize, preview: Bool) -> CGFloat {
        preview ? min(size.width * 0.0115, size.height * 0.019) : min(size.width * 0.01, size.height * 0.0165)
    }

    static let drift: [(CGFloat, CGFloat)] = [(0, 0), (-2.5, -1.5), (2.5, 1.2), (-1.5, 2), (2, -2)]

    private func attr(_ s: String, _ font: NSFont, _ color: NSColor, trackingEm: CGFloat = 0,
                      lineHeight: CGFloat? = nil, lang: String? = nil, indent: CGFloat = 0, hyphenate: Float = 0) -> NSAttributedString {
        var a: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color, .kern: trackingEm * font.pointSize]
        let p = NSMutableParagraphStyle()
        p.lineBreakMode = .byWordWrapping
        p.firstLineHeadIndent = indent
        p.hyphenationFactor = hyphenate
        if let lineHeight {
            p.minimumLineHeight = lineHeight; p.maximumLineHeight = lineHeight
            // CSS centres the glyphs in the line box (half-leading above and below).
            let natural = font.ascender - font.descender
            a[.baselineOffset] = (lineHeight - natural) / 2
        }
        a[.paragraphStyle] = p
        if let lang { a[.ctLanguage] = lang }
        return NSAttributedString(string: s, attributes: a)
    }

    private func width(_ s: NSAttributedString) -> CGFloat { ceil(s.size().width) }
    private func lineBox(_ f: NSFont) -> CGFloat { f.ascender - f.descender + f.leading }

    private static func lead(_ c: Card) -> String? {
        if let a = c.article, !a.isEmpty { return a }
        return c.reflexive == true ? "sich" : nil
    }

    private static func prefix(_ c: Card) -> String? {
        guard let p = c.separable_prefix, !p.isEmpty, c.word.hasPrefix(p), p.count < c.word.count else { return nil }
        return p
    }

    private static func wordSize(_ c: Card) -> CGFloat {
        let len = c.word.count + (lead(c) != nil ? 2 : 0) + (prefix(c) != nil ? 1 : 0)
        return len <= 7 ? 11.5 : len <= 10 ? 10 : len <= 13 ? 8.4 : len <= 16 ? 7 : 6
    }

    private func nonEmpty(_ s: String?) -> String? { (s?.isEmpty ?? true) ? nil : s }

    /// Lay out a card at origin (0,0) with width `w` (flipped coordinates, y grows downwards).
    func layout(_ c: Card, width w: CGFloat, u: CGFloat) -> CardLayout {
        var L = CardLayout()
        let accent = Palette.accent(c.gender, number: c.number)
        var y: CGFloat = 0

        // Meta row: part of speech, then a gradient rule to the right edge.
        let genderDe = ["masculine": "maskulin", "feminine": "feminin", "neuter": "neutrum"]
        let posText: String
        if let g = c.gender, let gd = genderDe[g] { posText = "\(c.pos_de) · \(gd)" }
        else if c.number == "plural_only" { posText = "\(c.pos_de) · nur Plural" }
        else if c.pos_de.lowercased() == c.pos_en.lowercased() { posText = c.pos_de }
        else { posText = "\(c.pos_de) · \(c.pos_en)" }
        let posFont = fonts.font(.sans, 1.05 * u)
        let pos = attr(posText.uppercased(), posFont, accent, trackingEm: 0.18)
        let rowH = lineBox(posFont)
        let rowMid = y + rowH / 2
        L.texts.append(TextItem(string: pos, origin: CGPoint(x: 0, y: rowMid - (posFont.ascender + posFont.descender) / 2), width: nil))
        let ruleStart = width(pos) + 1.6 * u
        if w > ruleStart { L.gradientRule = (CGRect(x: ruleStart, y: rowMid - 0.5, width: w - ruleStart, height: 1), accent) }
        y += rowH + 2.2 * u

        // Headword: article (or "sich"; italic, accent, 0.42em, raised 0.08em) + word, with the
        // separable particle marked "an·rufen"; then the inflection line at article size.
        var size = CardRenderer.wordSize(c) * u
        let gap = { (s: CGFloat) in 0.28 * s }
        let pre = CardRenderer.prefix(c)
        func headParts(_ s: CGFloat) -> (NSAttributedString?, NSAttributedString) {
            let wf = fonts.font(.display, s)
            let word = NSMutableAttributedString()
            if let pre {
                // CSS .sep { margin: 0 0.04em }: extra space after the particle and after the dot.
                word.append(attr(String(pre.dropLast()), wf, Palette.ink, trackingEm: -0.012))
                word.append(attr(String(pre.suffix(1)), wf, Palette.ink, trackingEm: -0.012 + 0.04))
                word.append(attr("·", wf, accent, trackingEm: -0.012 + 0.04))
                word.append(attr(String(c.word.dropFirst(pre.count)), wf, Palette.ink, trackingEm: -0.012))
            } else {
                word.append(attr(c.word, wf, Palette.ink, trackingEm: -0.012))
            }
            let art = CardRenderer.lead(c).map { attr($0, fonts.font(.displayItalic, 0.42 * s), accent, trackingEm: -0.012) }
            return (art, word)
        }
        var (art, word) = headParts(size)
        while (art.map { width($0) + gap(size) } ?? 0) + width(word) > w && size > 2 * u {
            size *= 0.94; (art, word) = headParts(size)
        }
        let wf = word.attribute(.font, at: 0, effectiveRange: nil) as! NSFont
        let lh = 0.92 * size
        let baseline = y + (lh - (wf.ascender - wf.descender)) / 2 + wf.ascender
        var x: CGFloat = 0
        if let art {
            L.texts.append(TextItem(string: art, origin: CGPoint(x: 0, y: baseline - 0.08 * 0.42 * size), width: nil))
            x = width(art) + gap(size)
        }
        L.texts.append(TextItem(string: word, origin: CGPoint(x: x, y: baseline), width: nil))
        x += width(word)
        y += lh
        if let parts = inflection(c), !parts.isEmpty {
            let fs = min(4.4 * u, max(3.2 * u, 0.42 * size))
            let roman = fonts.font(.display, fs), italic = fonts.font(.displayItalic, fs)
            func build(lead: Bool, lineHeight: CGFloat?) -> NSAttributedString {
                let m = NSMutableAttributedString()
                let a = { (t: String, f: NSFont, col: NSColor) in m.append(self.attr(t, f, col, lineHeight: lineHeight)) }
                for (i, part) in parts.enumerated() {
                    if i == 0 { if lead { a("·\u{00a0}", roman, Palette.ink3) } } else { a(" · ", roman, Palette.ink3) }
                    for (t, kind) in part {
                        switch kind {
                        case .pluralArticle: a(t, italic, Palette.plural)
                        case .note: a(t, italic, Palette.ink3)
                        case .text: a(t, roman, Palette.ink2)
                        }
                    }
                }
                return m
            }
            let inline = build(lead: true, lineHeight: nil)
            if x + gap(size) + width(inline) <= w {
                L.texts.append(TextItem(string: inline, origin: CGPoint(x: x + gap(size), y: baseline), width: nil))
            } else {
                // Drops below the word (CSS flex-wrap, row-gap 1.3u), without the leading dot.
                y += 1.3 * u
                let s = build(lead: false, lineHeight: 1.15 * fs)
                let h = ceil(s.boundingRect(with: CGSize(width: w, height: 10_000), options: [.usesLineFragmentOrigin, .usesFontLeading]).height)
                L.texts.append(TextItem(string: s, origin: CGPoint(x: 0, y: y), width: w))
                y += h
            }
        }

        // Pronunciation: IPA + syllables, baseline-aligned.
        y += 2.2 * u
        let ipaFont = fonts.font(.ipa, 2.1 * u)
        let sylFont = fonts.font(.sans, 1.1 * u)
        let ipa = nonEmpty(c.ipa).map { attr($0, ipaFont, Palette.ink2, trackingEm: 0.02) }
        let syl = nonEmpty(c.syllables).flatMap { $0 == c.word ? nil : attr($0, sylFont, Palette.ink3, trackingEm: 0.12) }
        if ipa != nil || syl != nil {
            let fonts = [ipa.map { _ in ipaFont }, syl.map { _ in sylFont }].compactMap { $0 }
            let asc = fonts.map { $0.ascender + max(0, $0.leading) / 2 }.max()!
            let desc = fonts.map { -$0.descender + max(0, $0.leading) / 2 }.max()!
            var px: CGFloat = 0
            if let ipa { L.texts.append(TextItem(string: ipa, origin: CGPoint(x: 0, y: y + asc), width: nil)); px = width(ipa) + 2.2 * u }
            if let syl { L.texts.append(TextItem(string: syl, origin: CGPoint(x: px, y: y + asc), width: nil)) }
            y += asc + desc
        }

        // English equivalent with the accent dash.
        y += 3.6 * u
        if let en = nonEmpty(c.english) {
            let ef = fonts.font(.displayItalic, 4.2 * u)
            let elh = 1.05 * ef.pointSize
            let dashW = 2.8 * u, indent = dashW + 1.4 * u
            let s = attr(en, ef, Palette.ink, lineHeight: elh, indent: indent)
            let h = ceil(s.boundingRect(with: CGSize(width: w, height: 10_000), options: [.usesLineFragmentOrigin, .usesFontLeading]).height)
            L.texts.append(TextItem(string: s, origin: CGPoint(x: 0, y: y), width: w))
            // vertical-align: middle = baseline - x-height/2, then translateY(-0.1em).
            let firstBaseline = y + (elh - (ef.ascender - ef.descender)) / 2 + ef.ascender
            let dashY = firstBaseline - ef.xHeight / 2 - 0.1 * ef.pointSize
            L.englishDash = (CGRect(x: 0, y: dashY - 0.5, width: dashW, height: 1), accent)
            y += h
        }

        // Usage example: German (italic) over its English translation, hanging past the dash.
        if let de = nonEmpty(c.example_de) {
            y += 2.4 * u
            let ex = 4.2 * u, exW = min(w, 74 * u) - ex
            let df = fonts.font(.textItalic, 1.9 * u, opsz: 20)
            let ds = attr(de, df, Palette.ink, lineHeight: 1.4 * df.pointSize, lang: "de")
            let dh = ceil(ds.boundingRect(with: CGSize(width: exW, height: 10_000), options: [.usesLineFragmentOrigin, .usesFontLeading]).height)
            L.texts.append(TextItem(string: ds, origin: CGPoint(x: ex, y: y), width: exW))
            y += dh
            if let en = nonEmpty(c.example_en) {
                y += 0.4 * u
                let ef2 = fonts.font(.text, 1.5 * u, opsz: 16)
                let es = attr(en, ef2, Palette.ink3, lineHeight: 1.45 * ef2.pointSize, lang: "en")
                let eh = ceil(es.boundingRect(with: CGSize(width: exW, height: 10_000), options: [.usesLineFragmentOrigin, .usesFontLeading]).height)
                L.texts.append(TextItem(string: es, origin: CGPoint(x: ex, y: y), width: exW))
                y += eh
            }
        }

        // Definitions: hairline, then two columns.
        y += 4.2 * u
        L.lines.append((CGRect(x: 0, y: y, width: w, height: 1), Palette.hair))
        y += 1 + 2.4 * u
        let colGap = 5 * u, colW = (w - colGap) / 2
        let hf = fonts.font(.sans, 0.9 * u, weight: 500)
        var colH: CGFloat = 0
        for (i, (title, text, italic, lang)) in [("English", c.definition_en, false, "en"), ("Deutsch", c.definition_de, true, "de")].enumerated() {
            let cx = CGFloat(i) * (colW + colGap)
            let h2 = attr(title.uppercased(), hf, Palette.ink3, trackingEm: 0.22)
            L.texts.append(TextItem(string: h2, origin: CGPoint(x: cx, y: y + hf.ascender + max(0, hf.leading) / 2), width: nil))
            var cy = y + lineBox(hf) + 1.1 * u
            if let t = nonEmpty(text) {
                let pf = fonts.font(italic ? .textItalic : .text, 1.75 * u, opsz: 20)
                let p = attr(t, pf, Palette.ink2, lineHeight: 1.5 * pf.pointSize, lang: lang, hyphenate: 0.3)
                let ph = ceil(p.boundingRect(with: CGSize(width: colW, height: 10_000), options: [.usesLineFragmentOrigin, .usesFontLeading]).height)
                L.texts.append(TextItem(string: p, origin: CGPoint(x: cx, y: cy), width: colW))
                cy += ph
            }
            colH = max(colH, cy - y)
        }
        y += colH
        L.height = y
        return L
    }

    enum InflectionKind { case text, pluralArticle, note }

    /// The parts after the headword, as core/app.js builds them: "die Häuser", "kein Plural",
    /// "fährt" · "fuhr" · "ist gefahren", "besser" · "am besten". Words inside a part are joined
    /// with no-break spaces so lines only break between parts.
    func inflection(_ c: Card) -> [[(String, InflectionKind)]]? {
        let nb = "\u{00a0}"
        if c.pos == "noun" {
            if let p = nonEmpty(c.plural) { return [[(c.plural_article ?? "die", .pluralArticle), (nb + p, .text)]] }
            if let n = nonEmpty(c.plural_note), c.number != "plural_only" { return [[(n, .note)]] }
            return nil
        }
        let line = c.pos == "verb" ? c.principal_parts : c.pos == "adj" ? c.forms_line : nil
        guard let line = nonEmpty(line) else { return nil }
        return line.components(separatedBy: " · ").map { [($0.replacingOccurrences(of: " ", with: nb), .text)] }
    }

    // MARK: Drawing

    func drawBackground(in ctx: CGContext, size: CGSize) {
        ctx.setFillColor(Palette.bg.cgColor)
        ctx.fill(CGRect(origin: .zero, size: size))
        // radial-gradient(ellipse at 50% 45%, rgba(255,250,240,.035), transparent 70%), farthest-corner.
        let cx = size.width / 2, cy = size.height * 0.45
        let k = sqrt(1 + pow(0.55 / 0.45, 2))
        let rx = k * size.width / 2, ry = k * 0.45 * size.height
        let colors = [NSColor(srgbRed: 1, green: 250 / 255, blue: 240 / 255, alpha: 0.035).cgColor,
                      NSColor(srgbRed: 1, green: 250 / 255, blue: 240 / 255, alpha: 0).cgColor] as CFArray
        guard let g = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: colors, locations: [0, 0.7]) else { return }
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.scaleBy(x: rx, y: ry)
        ctx.drawRadialGradient(g, startCenter: .zero, startRadius: 0, endCenter: .zero, endRadius: 1, options: [.drawsAfterEndLocation])
        ctx.restoreGState()
    }

    func draw(_ L: CardLayout, in ctx: CGContext) {
        for (r, color) in L.lines { ctx.setFillColor(color.cgColor); ctx.fill(r) }
        if let (r, accent) = L.gradientRule {
            // linear-gradient(90deg, accent, hair 30%, transparent) at opacity .55
            let colors = [accent.cgColor, Palette.hair.cgColor, Palette.hair.withAlphaComponent(0).cgColor] as CFArray
            if let g = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: colors, locations: [0, 0.3, 1]) {
                ctx.saveGState(); ctx.setAlpha(0.55); ctx.clip(to: r)
                ctx.drawLinearGradient(g, start: CGPoint(x: r.minX, y: r.midY), end: CGPoint(x: r.maxX, y: r.midY), options: [])
                ctx.restoreGState()
            }
        }
        if let (r, accent) = L.englishDash { ctx.setFillColor(accent.withAlphaComponent(0.8).cgColor); ctx.fill(r) }
        for t in L.texts { t.draw() }
    }

    /// Bottom colophon: brand, progress hairline, credit (full-screen only).
    func drawColophon(in ctx: CGContext, size: CGSize, u: CGFloat, progress: CGFloat) {
        let f = fonts.font(.sans, 0.85 * u)
        let brand = attr("WORTSCHATZ", f, Palette.ink3, trackingEm: 0.22)
        let credit = attr("WIKTIONARY · OPEN ENGLISH WORDNET", f, Palette.ink3, trackingEm: 0.12)
        let left = 4 * u, right = size.width - 4 * u
        let rowH = lineBox(f)
        let top = size.height - 3 * u - rowH
        let mid = top + rowH / 2
        let baseline = mid - (f.ascender + f.descender) / 2
        ctx.saveGState()
        ctx.setAlpha(0.55)
        TextItem(string: brand, origin: CGPoint(x: left, y: baseline), width: nil).draw()
        let cw = width(credit)
        TextItem(string: credit, origin: CGPoint(x: right - cw, y: baseline), width: nil).draw()
        let tx = left + width(brand) + 2 * u, tw = right - cw - 2 * u - tx
        if tw > 0 {
            ctx.setFillColor(Palette.hair.cgColor); ctx.fill(CGRect(x: tx, y: mid - 0.5, width: tw, height: 1))
            ctx.setFillColor(Palette.ink3.cgColor); ctx.fill(CGRect(x: tx, y: mid - 0.5, width: tw * max(0, min(1, progress)), height: 1))
        }
        ctx.restoreGState()
    }
}
