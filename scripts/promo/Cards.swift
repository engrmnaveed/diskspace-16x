import AppKit
import CoreText
import ImageIO
import UniformTypeIdentifiers

struct ExplorerCapture: Decodable {
    struct Span: Decodable {
        let column: Int
        let columns: Int
        let text: String
        let foreground: UInt32
        let background: UInt32?
        let bold: Bool
    }

    let columns: Int
    let frames: [String: [[Span]]]
}

@MainActor
struct Cards {
    let output: URL
    let icon: CGImage
    let report: [String]
    let explorer: ExplorerCapture
    let desktop: [String: CGImage]

    let mint: UInt32 = 0x72F6DC
    let white: UInt32 = 0xF2FAFF
    let muted: UInt32 = 0xADC7D8
    let border: UInt32 = 0x37566C

    func render() throws {
        try short("opening", section: "DISK SPACE ANALYZER", title: ["What’s eating", "your disk space?"],
                  detail: "Find the folders and files using it.", progress: 1) { canvas in
            appPanel("treemap", on: canvas, top: 641)
            explorerPanel("folders", on: canvas, x: 74, top: 985, width: 916, limit: 8)
        }
        for name in ["launch", "folders"] {
            try short(name, section: "CLI  /  macOS + LINUX", title: ["Open the", "disk explorer."],
                      detail: "Type space16x in any folder.", progress: 2) { canvas in
                if name == "launch" {
                    command(on: canvas, top: 646)
                } else {
                    explorerPanel(name, on: canvas, x: 74, top: 646, width: 916)
                }
            }
        }
        try short("drill", section: "CLI  /  macOS + LINUX", title: ["Open a folder.", "See what’s big."],
                  detail: "Press Enter to open Exports.", progress: 2) { canvas in
            explorerPanel("drill", on: canvas, x: 74, top: 646, width: 916)
        }
        try short("rank", section: "CLI  /  macOS + LINUX", title: ["Find the", "largest files."],
                  detail: "Go back and press f to rank files.", progress: 2) { canvas in
            explorerPanel("rank", on: canvas, x: 74, top: 646, width: 916)
        }
        try short("report", section: "CLI  /  macOS + LINUX", title: ["Get a report", "in one command."],
                  detail: "Print your top five files.", progress: 2) { canvas in
            terminal(on: canvas, top: 646)
        }
        try short("map", section: "DESKTOP  /  macOS", title: ["Spot the", "biggest folder."],
                  detail: "Bigger blocks use more disk space.", progress: 3) { canvas in
            appPanel("treemap", on: canvas, top: 646)
        }
        try short("share", section: "DESKTOP  /  macOS", title: ["See each", "folder’s share."],
                  detail: "Switch to the donut chart.", progress: 4) { canvas in
            appPanel("donut", on: canvas, top: 646)
        }
        try short("sort", section: "DESKTOP  /  macOS", title: ["Sort by size,", "items, or name."],
                  detail: "Click a column to re-sort.", progress: 5) { canvas in
            appPanel("list", on: canvas, top: 646)
        }
        try short("end", section: "DISK SPACE ANALYZER", title: ["Find the", "space hogs."],
                  detail: "Free and open source.", progress: 6) { canvas in
            canvas.image(icon, in: CGRect(x: 231, y: 621, width: 588, height: 588))
            canvas.panel(CGRect(x: 74, y: 1216, width: 916, height: 144), fill: 0x102B3C, border: border)
            canvas.text("GitHub link on our profile", x: 102, y: 1241, size: 42, weight: .bold, ink: white)
            canvas.text("engrmnaveed/diskspace-16x", x: 104, y: 1297, size: 32, ink: mint, mono: true)
        }

        for (index, (terminal, screenshot)) in [("folders", "treemap"), ("drill", "donut"), ("rank", "list")].enumerated() {
            let canvas = PromoCanvas(width: 900, height: 500)
            canvas.background()
            canvas.image(icon, in: CGRect(x: 21, y: 14, width: 66, height: 66))
            canvas.text("Space 16X", x: 96, y: 28, size: 31, weight: .bold, ink: white)
            canvas.text("Disk space analyzer", x: 96, y: 65, size: 19, ink: muted)
            canvas.text("CLI  /  macOS + LINUX", x: 32, y: 113, size: 15, weight: .semibold, ink: mint, mono: true)
            canvas.text("DESKTOP  /  macOS", x: 451, y: 113, size: 15, weight: .semibold, ink: mint, mono: true)
            explorerPanel(terminal, on: canvas, x: 32, top: 148, width: 400)
            let image = detailImage(screenshot)
            let height = 418 * CGFloat(image.height) / CGFloat(image.width)
            let shot = CGRect(x: 450, y: 148, width: 418, height: height)
            canvas.image(image, in: shot, radius: 12)
            canvas.outline(shot, radius: 12, ink: border)
            try canvas.write(output.appending(path: "preview-\(index).png"))
        }
    }

    private func short(
        _ name: String, section: String, title: [String], detail: String, progress: Int,
        draw: (PromoCanvas) -> Void
    ) throws {
        let canvas = PromoCanvas(width: 1080, height: 1920)
        canvas.background()
        canvas.image(icon, in: CGRect(x: 59, y: 142, width: 99, height: 99))
        canvas.text("SPACE 16X", x: 164, y: 172, size: 32, weight: .bold, ink: white, mono: true)
        canvas.text(section, x: 78, y: 295, size: 28, weight: .semibold, ink: mint, mono: true)
        for (index, line) in title.enumerated() {
            canvas.text(line, x: 72, y: 340 + CGFloat(index) * 104, size: 100, weight: .heavy, ink: white)
        }
        canvas.text(detail, x: 78, y: 566, size: 42, weight: .medium, ink: muted)
        draw(canvas)
        for position in 0..<6 {
            let bar = CGRect(x: 76 + 140 * CGFloat(position), y: 1503, width: 124, height: 6)
            canvas.panel(bar, fill: position < progress ? mint : border, radius: 3)
        }
        try canvas.write(output.appending(path: "\(name).png"))
    }

    private func detailImage(_ name: String) -> CGImage {
        let image = desktop[name]!
        // Focus on the results pane. The mount path and sidebar sit outside this crop.
        let scale = CGFloat(image.width) / 810
        return image.cropping(to: CGRect(x: 216 * scale, y: 32 * scale,
                                         width: 594 * scale, height: CGFloat(image.height) - 32 * scale))!
    }

    private func appPanel(_ name: String, on canvas: PromoCanvas, top: CGFloat) {
        let image = detailImage(name)
        let height = 916 * CGFloat(image.height) / CGFloat(image.width)
        let frame = CGRect(x: 74, y: top, width: 916, height: height + 57)
        canvas.panel(frame, fill: 0x132D40)
        canvas.text("Space 16X", x: 101, y: top + 11, size: 27, weight: .semibold, ink: white)
        let imageRect = CGRect(x: 74, y: top + 57, width: 916, height: height)
        canvas.clipped(to: frame, radius: 28) { canvas.image(image, in: imageRect) }
        canvas.outline(frame, radius: 28, ink: border)
    }

    private func terminalWindow(_ frame: CGRect, on canvas: PromoCanvas) {
        let scale = frame.width / 916
        canvas.panel(frame, fill: 0x0A1724, border: border, radius: 28 * scale)
        for (index, ink) in [UInt32(0xF0848D), 0xF7C16E, mint].enumerated() {
            canvas.panel(CGRect(x: frame.minX + (31 + CGFloat(index) * 26) * scale,
                                y: frame.minY + 26 * scale, width: 12 * scale, height: 12 * scale),
                         fill: ink, radius: 6 * scale)
        }
        canvas.text("Terminal", x: frame.minX + 119 * scale, y: frame.minY + 18 * scale,
                    size: 24 * scale, weight: .medium, ink: muted)
        canvas.panel(CGRect(x: frame.minX + 1, y: frame.minY + 64 * scale, width: frame.width - 2, height: 2 * scale),
                     fill: border, radius: 0)
    }

    private func command(on canvas: PromoCanvas, top: CGFloat) {
        terminalWindow(CGRect(x: 74, y: top, width: 916, height: 226), on: canvas)
        canvas.text("$ space16x", x: 102, y: top + 111,
                    size: 49, weight: .semibold, ink: mint, mono: true)
        canvas.panel(CGRect(x: 404, y: top + 121, width: 24, height: 42), fill: mint, radius: 2)
    }

    private func explorerPanel(
        _ name: String, on canvas: PromoCanvas, x: CGFloat, top: CGFloat, width: CGFloat, limit: Int = .max
    ) {
        let rows = explorer.frames[name]!.prefix(limit)
        let scale = width / 916
        let inset = 26 * scale
        let cellWidth = (width - 2 * inset) / CGFloat(explorer.columns)
        let unitFont = NSFont.monospacedSystemFont(ofSize: 100, weight: .regular)
        let unitWidth = ("0" as NSString).size(withAttributes: [.font: unitFont]).width / 100
        let font = cellWidth / unitWidth
        let lineHeight = 39 * scale
        let contentTop = top + 88 * scale
        let height = 118 * scale + CGFloat(rows.count) * lineHeight
        terminalWindow(CGRect(x: x, y: top, width: width, height: height), on: canvas)
        for (index, row) in rows.enumerated() {
            let y = contentTop + CGFloat(index) * lineHeight
            for span in row {
                let start = x + inset + CGFloat(span.column) * cellWidth
                if let background = span.background {
                    canvas.panel(CGRect(x: start, y: y, width: CGFloat(span.columns) * cellWidth,
                                        height: lineHeight), fill: background, radius: 0)
                }
                canvas.text(span.text, x: start, y: y + 2 * scale, size: font,
                            weight: span.bold ? .bold : .regular, ink: span.foreground, mono: true)
            }
        }
    }

    private func terminal(on canvas: PromoCanvas, top: CGFloat) {
        let frame = CGRect(x: 74, y: top, width: 916, height: 213 + CGFloat(report.count) * 58)
        terminalWindow(frame, on: canvas)
        canvas.text("$ space16x scan Projects --top 5", x: 102, y: top + 92,
                    size: 33, weight: .semibold, ink: mint, mono: true)
        for (index, row) in report.enumerated() {
            let y = top + 161 + 58 * CGFloat(index)
            if index == 0 {
                canvas.panel(CGRect(x: 93, y: y - 6, width: frame.width - 38, height: 55), fill: 0x163A45, radius: 11)
            }
            canvas.text(row, x: 102, y: y, size: 31, ink: index == 0 ? white : muted, mono: true)
        }
    }
}

@MainActor
private struct PromoCanvas {
    let context: CGContext
    let width: CGFloat
    let height: CGFloat
    private let space = CGColorSpace(name: CGColorSpace.sRGB)!

    init(width: Int, height: Int) {
        self.width = CGFloat(width)
        self.height = CGFloat(height)
        context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                            bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
        context.interpolationQuality = .high
    }

    func background() {
        let colors = [color(0x164B50), color(0x101F33), color(0x080F1A)] as CFArray
        let gradient = CGGradient(colorsSpace: space, colors: colors, locations: [0, 0.55, 1])!
        context.drawRadialGradient(gradient, startCenter: CGPoint(x: width * 0.89, y: height * 0.60),
                                   startRadius: 0, endCenter: CGPoint(x: width * 0.89, y: height * 0.60),
                                   endRadius: max(width, height * 0.75), options: [.drawsAfterEndLocation])
        context.setStrokeColor(color(0x80C8C6, alpha: 0.075))
        context.setLineWidth(1)
        for x in stride(from: 0.0, to: width, by: 58) {
            context.move(to: CGPoint(x: x, y: 0))
            context.addLine(to: CGPoint(x: x, y: height))
        }
        for y in stride(from: 0.0, to: height, by: 58) {
            context.move(to: CGPoint(x: 0, y: y))
            context.addLine(to: CGPoint(x: width, y: y))
        }
        context.strokePath()
    }

    func panel(_ rect: CGRect, fill: UInt32, border: UInt32? = nil, radius: CGFloat = 28) {
        context.addPath(path(rect, radius: radius))
        context.setFillColor(color(fill))
        context.fillPath()
        if let border { outline(rect, radius: radius, ink: border) }
    }

    func outline(_ rect: CGRect, radius: CGFloat, ink: UInt32) {
        context.addPath(path(rect, radius: radius))
        context.setStrokeColor(color(ink))
        context.setLineWidth(2)
        context.strokePath()
    }

    func clipped(to rect: CGRect, radius: CGFloat, draw: () -> Void) {
        context.saveGState()
        context.addPath(path(rect, radius: radius))
        context.clip()
        draw()
        context.restoreGState()
    }

    func image(_ image: CGImage, in rect: CGRect, radius: CGFloat = 0) {
        clipped(to: rect, radius: radius) { context.draw(image, in: flipped(rect)) }
    }

    func text(
        _ text: String, x: CGFloat, y: CGFloat, size: CGFloat,
        weight: NSFont.Weight = .regular, ink: UInt32, mono: Bool = false
    ) {
        let font = mono ? NSFont.monospacedSystemFont(ofSize: size, weight: weight)
                        : NSFont.systemFont(ofSize: size, weight: weight)
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: [
            .init(kCTFontAttributeName as String): font,
            .init(kCTForegroundColorAttributeName as String): color(ink),
        ]))
        context.textPosition = CGPoint(x: x, y: height - y - size)
        CTLineDraw(line, context)
    }

    func write(_ url: URL) throws {
        guard let image = context.makeImage(),
              let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { throw CocoaError(.fileWriteUnknown, userInfo: [NSURLErrorKey: url]) }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else {
            throw CocoaError(.fileWriteUnknown, userInfo: [NSURLErrorKey: url])
        }
    }

    private func color(_ hex: UInt32, alpha: CGFloat = 1) -> CGColor {
        CGColor(srgbRed: CGFloat((hex >> 16) & 255) / 255, green: CGFloat((hex >> 8) & 255) / 255,
                blue: CGFloat(hex & 255) / 255, alpha: alpha)
    }

    private func path(_ rect: CGRect, radius: CGFloat) -> CGPath {
        CGPath(roundedRect: flipped(rect), cornerWidth: radius, cornerHeight: radius, transform: nil)
    }

    private func flipped(_ rect: CGRect) -> CGRect {
        CGRect(x: rect.minX, y: height - rect.maxY, width: rect.width, height: rect.height)
    }
}
