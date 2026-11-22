import AppKit
import CoreText
import ImageIO
import UniformTypeIdentifiers

/// What `scripts/build-guide-media.py` asks for: the terminal frames and the images to draw.
struct Spec: Decodable {
    struct Span: Decodable {
        let column: Int
        let text: String
        let fg: UInt32
        let bg: UInt32?
        let bold: Bool
    }

    struct Terminal: Decodable {
        let columns: Int
        let frames: [String: [[Span]]]
    }

    struct Layer: Decodable {
        let type: String
        var x: CGFloat = 0, y: CGFloat = 0, w: CGFloat = 0, h: CGFloat = 0
        var source: String?
        var frame: String?
        var rows: Int?
        var text: String?
        var size: CGFloat?
        var weight: String?
        var color: UInt32?
        var mono: Bool?
        var align: String?
        var items: [String]?
        var progress: Int?
        var total: Int?

        enum CodingKeys: String, CodingKey {
            case type, x, y, w, h, source, frame, rows, text, size, weight, color, mono, align, items, progress, total
        }

        init(from decoder: Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            type = try values.decode(String.self, forKey: .type)
            x = try values.decodeIfPresent(CGFloat.self, forKey: .x) ?? 0
            y = try values.decodeIfPresent(CGFloat.self, forKey: .y) ?? 0
            w = try values.decodeIfPresent(CGFloat.self, forKey: .w) ?? 0
            h = try values.decodeIfPresent(CGFloat.self, forKey: .h) ?? 0
            source = try values.decodeIfPresent(String.self, forKey: .source)
            frame = try values.decodeIfPresent(String.self, forKey: .frame)
            rows = try values.decodeIfPresent(Int.self, forKey: .rows)
            text = try values.decodeIfPresent(String.self, forKey: .text)
            size = try values.decodeIfPresent(CGFloat.self, forKey: .size)
            weight = try values.decodeIfPresent(String.self, forKey: .weight)
            color = try values.decodeIfPresent(UInt32.self, forKey: .color)
            mono = try values.decodeIfPresent(Bool.self, forKey: .mono)
            align = try values.decodeIfPresent(String.self, forKey: .align)
            items = try values.decodeIfPresent([String].self, forKey: .items)
            progress = try values.decodeIfPresent(Int.self, forKey: .progress)
            total = try values.decodeIfPresent(Int.self, forKey: .total)
        }
    }

    struct Output: Decodable {
        /// `shot` draws one window at `width`, as tall as it needs. `canvas` draws `layers`.
        let kind: String
        let path: String
        let width: CGFloat
        let height: CGFloat?
        let background: String?
        let source: String?
        let frame: String?
        let layers: [Layer]?
    }

    let terminal: Terminal
    let outputs: [Output]
}

@MainActor
struct Compositor {
    let spec: Spec
    let windows: [String: CGImage]
    let icon: CGImage

    func render() throws {
        for output in spec.outputs {
            let image: CGImage
            if output.kind == "shot" {
                image = try shot(output)
            } else {
                let canvas = GuideCanvas(width: Int(output.width), height: Int(output.height ?? output.width))
                background(output.background ?? "light", on: canvas)
                for layer in output.layers ?? [] { try draw(layer, on: canvas) }
                image = canvas.image()
            }
            try GuideCanvas.write(image, to: URL(filePath: output.path))
        }
    }

    private func window(_ name: String?) throws -> CGImage {
        guard let name, let image = windows[name] else { throw GuideError("No captured window “\(name ?? "")”.") }
        return image
    }

    private func frame(_ name: String?) throws -> [[Spec.Span]] {
        guard let name, let rows = spec.terminal.frames[name] else { throw GuideError("No terminal frame “\(name ?? "")”.") }
        return rows
    }

    /// A screenshot: one window, edge to edge, with a hairline border.
    private func shot(_ output: Spec.Output) throws -> CGImage {
        if let source = output.source {
            let image = try window(source)
            let height = (output.width * CGFloat(image.height) / CGFloat(image.width)).rounded()
            let canvas = GuideCanvas(width: Int(output.width), height: Int(height))
            let rect = CGRect(x: 0, y: 0, width: output.width, height: height)
            canvas.image(image, in: rect)
            canvas.stroke(rect.insetBy(dx: 0.5, dy: 0.5), color: 0x000000, alpha: 0.16, radius: 0, width: 1)
            return canvas.image()
        }
        let rows = try frame(output.frame)
        let height = Terminal.height(rows: Terminal.trimmed(rows).count, width: output.width, columns: spec.terminal.columns)
        let canvas = GuideCanvas(width: Int(output.width), height: Int(height.rounded(.up)))
        canvas.fill(CGRect(x: 0, y: 0, width: output.width, height: height.rounded(.up)), color: 0xFFFFFF)
        Terminal.draw(Terminal.trimmed(rows), columns: spec.terminal.columns, on: canvas,
                      at: CGPoint(x: 0, y: 0), width: output.width, radius: 0)
        return canvas.image()
    }

    private func background(_ style: String, on canvas: GuideCanvas) {
        let rect = CGRect(x: 0, y: 0, width: canvas.width, height: canvas.height)
        switch style {
        case "brand":
            canvas.brandBackground()
        case "none":
            break
        default:
            canvas.fill(rect, color: 0xF4F6F8)
        }
    }

    private func draw(_ layer: Spec.Layer, on canvas: GuideCanvas) throws {
        switch layer.type {
        case "window":
            let image = try window(layer.source)
            let scale = min(layer.w / CGFloat(image.width), layer.h / CGFloat(image.height))
            let size = CGSize(width: CGFloat(image.width) * scale, height: CGFloat(image.height) * scale)
            let rect = CGRect(x: layer.x + (layer.w - size.width) / 2, y: layer.y + (layer.h - size.height) / 2,
                              width: size.width, height: size.height)
            let radius = 20 * scale // 10-point corners on a 2x capture
            canvas.shadowed(rect, radius: radius, blur: 36 * size.width / 1200, alpha: 0.32)
            canvas.image(image, in: rect, radius: radius)
            canvas.stroke(rect, color: 0x000000, alpha: 0.18, radius: radius, width: 1)
        case "terminal":
            var rows = try frame(layer.frame)
            if let pad = layer.rows {
                rows = Array(Terminal.trimmed(rows).prefix(pad))
                while rows.count < pad { rows.append([]) }
            } else {
                rows = Terminal.trimmed(rows)
            }
            var width = layer.w
            var height = Terminal.height(rows: rows.count, width: width, columns: spec.terminal.columns)
            if layer.h > 0, height > layer.h {
                width *= layer.h / height
                height = layer.h
            }
            let origin = CGPoint(x: layer.x + (layer.w - width) / 2, y: layer.y + max(0, (layer.h - height) / 2))
            let radius = 10 * width / 1000
            canvas.shadowed(CGRect(origin: origin, size: CGSize(width: width, height: height)),
                            radius: radius, blur: 36 * width / 1200, alpha: 0.32)
            Terminal.draw(rows, columns: spec.terminal.columns, on: canvas, at: origin, width: width, radius: radius)
        case "text":
            canvas.text(layer.text ?? "", x: layer.x, y: layer.y, size: layer.size ?? 24,
                        weight: Self.weight(layer.weight), color: layer.color ?? 0x1F2A33,
                        mono: layer.mono ?? false, width: layer.w > 0 ? layer.w : nil,
                        align: layer.align == "center" ? .center : layer.align == "right" ? .right : .left)
        case "keys":
            Keycap.draw(layer.items ?? [], on: canvas, x: layer.x, y: layer.y, height: layer.size ?? 48,
                        alignRight: layer.align == "right")
        case "icon":
            canvas.image(icon, in: CGRect(x: layer.x, y: layer.y, width: layer.w, height: layer.w))
        case "rect":
            canvas.fill(CGRect(x: layer.x, y: layer.y, width: layer.w, height: layer.h),
                        color: layer.color ?? 0xFFFFFF, radius: layer.size ?? 0)
        case "progress":
            let total = max(layer.total ?? 1, 1)
            let gap: CGFloat = 10
            let segment = (layer.w - gap * CGFloat(total - 1)) / CGFloat(total)
            for index in 0..<total {
                let rect = CGRect(x: layer.x + CGFloat(index) * (segment + gap), y: layer.y, width: segment, height: layer.h)
                canvas.fill(rect, color: index < (layer.progress ?? 0) ? 0x72F6DC : 0x37566C, radius: layer.h / 2)
            }
        default:
            throw GuideError("Unknown layer “\(layer.type)”.")
        }
    }

    static func weight(_ name: String?) -> NSFont.Weight {
        switch name {
        case "heavy": .heavy
        case "bold": .bold
        case "semibold": .semibold
        case "medium": .medium
        default: .regular
        }
    }
}

/// A light terminal window drawing the explorer's cells on a fixed grid.
@MainActor
enum Terminal {
    static let advance: CGFloat = {
        let font = NSFont.monospacedSystemFont(ofSize: 100, weight: .regular)
        return ("0" as NSString).size(withAttributes: [.font: font]).width / 100
    }()

    static func metrics(width: CGFloat, columns: Int) -> (scale: CGFloat, inset: CGFloat, cell: CGFloat, line: CGFloat, bar: CGFloat) {
        let scale = width / 1000
        let inset = 16 * scale
        let cell = (width - 2 * inset) / CGFloat(columns)
        return (scale, inset, cell, (cell * 2.05).rounded(), 34 * scale)
    }

    static func height(rows: Int, width: CGFloat, columns: Int) -> CGFloat {
        let m = metrics(width: width, columns: columns)
        return m.bar + 2 * m.inset + CGFloat(rows) * m.line
    }

    static func trimmed(_ rows: [[Spec.Span]]) -> [[Spec.Span]] {
        var rows = rows
        while let last = rows.last, last.allSatisfy({ $0.text.trimmingCharacters(in: .whitespaces).isEmpty && $0.bg == nil }) {
            rows.removeLast()
        }
        return rows
    }

    static func draw(_ rows: [[Spec.Span]], columns: Int, on canvas: GuideCanvas, at origin: CGPoint, width: CGFloat, radius: CGFloat) {
        let m = metrics(width: width, columns: columns)
        let height = height(rows: rows.count, width: width, columns: columns)
        let frame = CGRect(origin: origin, size: CGSize(width: width, height: height))
        canvas.fill(frame, color: 0xFFFFFF, radius: radius)
        canvas.clip(frame, radius: radius) {
            canvas.fill(CGRect(x: origin.x, y: origin.y, width: width, height: m.bar), color: 0xECEEF1)
            canvas.fill(CGRect(x: origin.x, y: origin.y + m.bar - 1, width: width, height: 1), color: 0xD9DDE1)
        }
        for (index, color) in [UInt32(0xFF5F57), 0xFEBC2E, 0x28C840].enumerated() {
            let center = CGPoint(x: origin.x + (18 + CGFloat(index) * 19) * m.scale, y: origin.y + m.bar / 2)
            let r = 5.5 * m.scale
            canvas.fill(CGRect(x: center.x - r, y: center.y - r, width: 2 * r, height: 2 * r), color: color, radius: r)
        }
        canvas.text("Terminal", x: origin.x, y: origin.y + m.bar / 2 - 8 * m.scale, size: 13 * m.scale,
                    weight: .semibold, color: 0x5B636B, width: width, align: .center)
        canvas.stroke(frame, color: 0x000000, alpha: 0.16, radius: radius, width: 1)

        let fontSize = m.cell / advance
        let top = origin.y + m.bar + m.inset
        for (row, spans) in rows.enumerated() {
            let y = top + CGFloat(row) * m.line
            for span in spans {
                let characters = Array(span.text)
                let x = origin.x + m.inset + CGFloat(span.column) * m.cell
                if let bg = span.bg {
                    canvas.fill(CGRect(x: x, y: y, width: CGFloat(characters.count) * m.cell, height: m.line), color: bg)
                }
                // Plain text draws in runs. Other characters draw one per cell, so the grid holds
                // even when a glyph comes from a fallback font.
                var run = ""
                var runStart = 0
                func flush() {
                    guard !run.isEmpty else { return }
                    canvas.text(run, x: x + CGFloat(runStart) * m.cell, y: y + (m.line - fontSize * 1.2) / 2,
                                size: fontSize, weight: span.bold ? .bold : .regular, color: span.fg, mono: true)
                    run = ""
                }
                for (offset, character) in characters.enumerated() {
                    // Whole pixels, so neighboring blocks join without seams.
                    let left = (x + CGFloat(offset) * m.cell).rounded(), right = (x + CGFloat(offset + 1) * m.cell).rounded()
                    let cell = CGRect(x: left, y: y.rounded(), width: right - left, height: m.line)
                    if let scalar = character.unicodeScalars.first, character.unicodeScalars.count == 1,
                       scalar.value >= 0x20, scalar.value < 0x250 {
                        if run.isEmpty { runStart = offset }
                        run.append(character)
                        continue
                    }
                    flush()
                    if character == " " { continue }
                    if !Self.drawShape(character, in: cell, color: span.fg, canvas: canvas) {
                        canvas.text(String(character), x: cell.minX, y: y + (m.line - fontSize * 1.2) / 2,
                                    size: fontSize, weight: span.bold ? .bold : .regular, color: span.fg,
                                    mono: true, width: m.cell, align: .center)
                    }
                }
                flush()
            }
        }
    }

    /// Block and box-drawing characters as shapes, so bars and borders join seamlessly.
    private static func drawShape(_ character: Character, in cell: CGRect, color: UInt32, canvas: GuideCanvas) -> Bool {
        let line = max(1, (cell.width * 0.1).rounded())
        let midX = (cell.midX - line / 2).rounded(), midY = (cell.midY - line / 2).rounded()
        let left = CGRect(x: cell.minX, y: midY, width: cell.midX - cell.minX + line / 2, height: line)
        let right = CGRect(x: midX, y: midY, width: cell.maxX - midX, height: line)
        let up = CGRect(x: midX, y: cell.minY, width: line, height: cell.midY - cell.minY + line / 2)
        let down = CGRect(x: midX, y: midY, width: line, height: cell.maxY - midY)
        // Shade blocks draw as a bar through the middle of the line, as a meter reads best.
        let bar = cell.insetBy(dx: 0, dy: (cell.height * 0.27).rounded())
        let parts: [CGRect]
        switch character {
        case "█": canvas.fill(bar, color: color); return true
        case "▓": canvas.fill(bar, color: color, alpha: 0.7); return true
        case "▒": canvas.fill(bar, color: color, alpha: 0.45); return true
        case "░": canvas.fill(bar, color: color, alpha: 0.16); return true
        case "─", "━": parts = [left, right]
        case "│", "┃": parts = [up, down]
        case "┌", "╭": parts = [right, down]
        case "┐", "╮": parts = [left, down]
        case "└", "╰": parts = [right, up]
        case "┘", "╯": parts = [left, up]
        case "├": parts = [up, down, right]
        case "┤": parts = [up, down, left]
        case "┬": parts = [left, right, down]
        case "┴": parts = [left, right, up]
        case "┼": parts = [left, right, up, down]
        default:
            if let scalar = character.unicodeScalars.first, (0x2589...0x258F).contains(scalar.value) {
                // Left blocks, from seven eighths (U+2589) to one eighth (U+258F).
                let eighths = CGFloat(0x2590 - scalar.value)
                canvas.fill(CGRect(x: bar.minX, y: bar.minY, width: bar.width * eighths / 8, height: bar.height), color: color)
                return true
            }
            return false
        }
        for part in parts { canvas.fill(part, color: color) }
        return true
    }
}

/// Keyboard keys, drawn as keycaps, for the key a scene presses.
@MainActor
enum Keycap {
    static func draw(_ keys: [String], on canvas: GuideCanvas, x: CGFloat, y: CGFloat, height: CGFloat, alignRight: Bool) {
        let size = height * 0.46
        let gap = height * 0.22
        let widths = keys.map { key in
            max(height, GuideCanvas.measure(key, size: size, weight: .semibold) + height * 0.6)
        }
        var cursor = alignRight ? x - widths.reduce(0, +) - gap * CGFloat(keys.count - 1) : x
        for (key, width) in zip(keys, widths) {
            let rect = CGRect(x: cursor, y: y, width: width, height: height)
            canvas.fill(rect.offsetBy(dx: 0, dy: height * 0.07), color: 0x8FA3B3, radius: height * 0.2)
            canvas.fill(rect, color: 0xF7FAFC, radius: height * 0.2)
            canvas.text(key, x: rect.minX, y: rect.minY + (height - size * 1.25) / 2, size: size,
                        weight: .semibold, color: 0x14212B, width: width, align: .center)
            cursor += width + gap
        }
    }
}

/// A bitmap with a top-left origin, for AppKit text and symbols and Core Graphics shapes.
@MainActor
final class GuideCanvas {
    let width: CGFloat
    let height: CGFloat
    let context: CGContext
    private let graphics: NSGraphicsContext
    private static let space = CGColorSpace(name: CGColorSpace.sRGB)!

    init(width: Int, height: Int) {
        self.width = CGFloat(width)
        self.height = CGFloat(height)
        context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                            space: Self.space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: 1, y: -1)
        context.interpolationQuality = .high
        graphics = NSGraphicsContext(cgContext: context, flipped: true)
    }

    func image() -> CGImage { context.makeImage()! }

    static func write(_ image: CGImage, to url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { throw CocoaError(.fileWriteUnknown, userInfo: [NSURLErrorKey: url]) }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { throw CocoaError(.fileWriteUnknown, userInfo: [NSURLErrorKey: url]) }
    }

    static func color(_ hex: UInt32, alpha: CGFloat = 1) -> NSColor {
        NSColor(srgbRed: CGFloat((hex >> 16) & 255) / 255, green: CGFloat((hex >> 8) & 255) / 255,
                blue: CGFloat(hex & 255) / 255, alpha: alpha)
    }

    private func path(_ rect: CGRect, radius: CGFloat) -> CGPath {
        let radius = min(radius, rect.width / 2, rect.height / 2)
        return CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    }

    func fill(_ rect: CGRect, color: UInt32, alpha: CGFloat = 1, radius: CGFloat = 0) {
        context.addPath(path(rect, radius: radius))
        context.setFillColor(Self.color(color, alpha: alpha).cgColor)
        context.fillPath()
    }

    func stroke(_ rect: CGRect, color: UInt32, alpha: CGFloat = 1, radius: CGFloat, width: CGFloat) {
        context.addPath(path(rect, radius: radius))
        context.setStrokeColor(Self.color(color, alpha: alpha).cgColor)
        context.setLineWidth(width)
        context.strokePath()
    }

    func clip(_ rect: CGRect, radius: CGFloat, draw: () -> Void) {
        context.saveGState()
        context.addPath(path(rect, radius: radius))
        context.clip()
        draw()
        context.restoreGState()
    }

    /// A soft shadow under a rounded rectangle that is drawn over afterwards.
    func shadowed(_ rect: CGRect, radius: CGFloat, blur: CGFloat, alpha: CGFloat) {
        context.saveGState()
        context.setShadow(offset: CGSize(width: 0, height: blur * 0.35), blur: blur,
                          color: Self.color(0x000000, alpha: alpha).cgColor)
        fill(rect, color: 0xFFFFFF, radius: radius)
        context.restoreGState()
    }

    func image(_ image: CGImage, in rect: CGRect, radius: CGFloat = 0) {
        clip(rect, radius: radius) {
            context.saveGState()
            context.translateBy(x: rect.minX, y: rect.maxY)
            context.scaleBy(x: 1, y: -1)
            context.draw(image, in: CGRect(origin: .zero, size: rect.size))
            context.restoreGState()
        }
    }

    private static func attributes(size: CGFloat, weight: NSFont.Weight, color: UInt32, mono: Bool,
                                   align: NSTextAlignment) -> [NSAttributedString.Key: Any] {
        let font = mono ? NSFont.monospacedSystemFont(ofSize: size, weight: weight)
                        : NSFont.systemFont(ofSize: size, weight: weight)
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = align
        paragraph.lineBreakMode = .byWordWrapping
        return [.font: font, .foregroundColor: Self.color(color), .paragraphStyle: paragraph]
    }

    static func measure(_ text: String, size: CGFloat, weight: NSFont.Weight, mono: Bool = false) -> CGFloat {
        (text as NSString).size(withAttributes: attributes(size: size, weight: weight, color: 0, mono: mono, align: .left)).width
    }

    /// Draws text with its top at `y`, wrapping within `width` when given.
    func text(_ text: String, x: CGFloat, y: CGFloat, size: CGFloat, weight: NSFont.Weight = .regular,
              color: UInt32, mono: Bool = false, width: CGFloat? = nil, align: NSTextAlignment = .left) {
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = graphics
        let attributed = NSAttributedString(string: text, attributes: Self.attributes(
            size: size, weight: weight, color: color, mono: mono, align: align
        ))
        let box = width ?? 100_000
        let origin = align == .right && width == nil ? x - box : x
        attributed.draw(with: CGRect(x: origin, y: y, width: box, height: 10_000),
                        options: [.usesLineFragmentOrigin, .usesFontLeading])
        NSGraphicsContext.restoreGraphicsState()
    }

    func symbol(_ name: String, center: CGPoint, size: CGFloat, color: UInt32) {
        guard let base = NSImage(systemSymbolName: name, accessibilityDescription: nil) else { return }
        let configuration = NSImage.SymbolConfiguration(pointSize: size, weight: .regular)
            .applying(NSImage.SymbolConfiguration(paletteColors: [Self.color(color)]))
        guard let symbol = base.withSymbolConfiguration(configuration) else { return }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = graphics
        let rect = CGRect(x: center.x - symbol.size.width / 2, y: center.y - symbol.size.height / 2,
                          width: symbol.size.width, height: symbol.size.height)
        symbol.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        NSGraphicsContext.restoreGraphicsState()
    }

    /// The promo's dark backdrop: a teal glow and a faint grid.
    func brandBackground() {
        let colors = [Self.color(0x164B50).cgColor, Self.color(0x101F33).cgColor, Self.color(0x080F1A).cgColor] as CFArray
        let gradient = CGGradient(colorsSpace: Self.space, colors: colors, locations: [0, 0.55, 1])!
        let center = CGPoint(x: width * 0.85, y: height * 0.15)
        context.drawRadialGradient(gradient, startCenter: center, startRadius: 0, endCenter: center,
                                   endRadius: max(width, height) * 0.95, options: [.drawsAfterEndLocation])
        context.setStrokeColor(Self.color(0x80C8C6, alpha: 0.06).cgColor)
        context.setLineWidth(1)
        for x in stride(from: 0.0, to: width, by: 60) {
            context.move(to: CGPoint(x: x, y: 0))
            context.addLine(to: CGPoint(x: x, y: height))
        }
        for y in stride(from: 0.0, to: height, by: 60) {
            context.move(to: CGPoint(x: 0, y: y))
            context.addLine(to: CGPoint(x: width, y: y))
        }
        context.strokePath()
    }
}
