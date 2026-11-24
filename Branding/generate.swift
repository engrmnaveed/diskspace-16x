import AppKit
import CoreGraphics
import CoreText
import ImageIO
import UniformTypeIdentifiers

private let rgb = CGColorSpace(name: CGColorSpace.sRGB)!

private func color(_ hex: UInt32, alpha: CGFloat = 1) -> CGColor {
    CGColor(
        red: CGFloat((hex >> 16) & 255) / 255,
        green: CGFloat((hex >> 8) & 255) / 255,
        blue: CGFloat(hex & 255) / 255,
        alpha: alpha
    )
}

private func gradient(
    _ context: CGContext, from first: UInt32, to second: UInt32,
    start: CGPoint, end: CGPoint
) {
    let colors = [color(first), color(second)] as CFArray
    let blend = CGGradient(colorsSpace: rgb, colors: colors, locations: [0, 1])!
    context.drawLinearGradient(
        blend, start: start, end: end, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation]
    )
}

private func sector(
    _ context: CGContext,
    center: CGPoint,
    inner: CGFloat,
    outer: CGFloat,
    from start: CGFloat,
    to end: CGFloat,
    colors: (UInt32, UInt32)
) {
    let startAngle = start * .pi / 180
    let endAngle = end * .pi / 180
    let path = CGMutablePath()
    path.addArc(center: center, radius: outer, startAngle: startAngle, endAngle: endAngle, clockwise: false)
    path.addArc(center: center, radius: inner, startAngle: endAngle, endAngle: startAngle, clockwise: true)
    path.closeSubpath()
    context.saveGState()
    context.addPath(path)
    context.clip()
    gradient(
        context, from: colors.0, to: colors.1,
        start: CGPoint(x: center.x - outer * 0.65, y: center.y - outer),
        end: CGPoint(x: center.x + outer * 0.65, y: center.y + outer)
    )
    context.restoreGState()
}

private func drawDisk(_ context: CGContext) {
    let center = CGPoint(x: 512, y: 520)
    let track = CGRect(x: center.x - 261, y: center.y - 261, width: 522, height: 522)
    context.setStrokeColor(color(0x9FDDE5, alpha: 0.13))
    context.setLineWidth(1.5)
    context.strokeEllipse(in: track)

    sector(context, center: center, inner: 88, outer: 237, from: -37, to: 61, colors: (0x0DA691, 0x74FFDD))
    sector(context, center: center, inner: 88, outer: 192, from: 65, to: 167, colors: (0x1F778E, 0x6DC9DB))
    sector(context, center: center, inner: 88, outer: 151, from: 171, to: 319, colors: (0x31577E, 0x648BBA))

    let hub = CGRect(x: center.x - 60, y: center.y - 60, width: 120, height: 120)
    context.setFillColor(color(0x102238))
    context.fillEllipse(in: hub)
    context.setStrokeColor(color(0x8CD6E5, alpha: 0.31))
    context.setLineWidth(2.5)
    context.strokeEllipse(in: hub)

    let origin = CGRect(x: center.x - 14, y: center.y - 14, width: 28, height: 28)
    context.setFillColor(color(0x76F9E0))
    context.fillEllipse(in: origin)
}

private func drawIcon(_ context: CGContext, size: CGFloat) {
    context.saveGState()
    context.scaleBy(x: size / 1024, y: size / 1024)
    let tile = CGRect(x: 100, y: 108, width: 824, height: 824)
    let outline = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)

    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -12), blur: 30, color: color(0x020914, alpha: 0.44))
    context.setFillColor(color(0x0A1729))
    context.addPath(outline)
    context.fillPath()
    context.restoreGState()

    context.saveGState()
    context.addPath(outline)
    context.clip()
    gradient(
        context, from: 0x081322, to: 0x1D3850,
        start: CGPoint(x: 470, y: 90), end: CGPoint(x: 542, y: 965)
    )
    let inset = tile.insetBy(dx: 5, dy: 5)
    context.addPath(CGPath(roundedRect: inset, cornerWidth: 180, cornerHeight: 180, transform: nil))
    context.setStrokeColor(color(0xB6E9F1, alpha: 0.19))
    context.setLineWidth(4)
    context.strokePath()
    drawDisk(context)
    context.restoreGState()
    context.restoreGState()
}

private func drawText(
    _ value: String, size: CGFloat, weight: NSFont.Weight,
    color ink: UInt32, at location: CGPoint, in context: CGContext
) {
    let font = NSFont.systemFont(ofSize: size, weight: weight)
    let attributes: [NSAttributedString.Key: Any] = [
        .init(kCTFontAttributeName as String): font,
        .init(kCTForegroundColorAttributeName as String): color(ink),
    ]
    let line = CTLineCreateWithAttributedString(NSAttributedString(string: value, attributes: attributes))
    context.textPosition = location
    CTLineDraw(line, context)
}

private func drawLockup(_ context: CGContext) {
    let panel = CGPath(
        roundedRect: CGRect(x: 0, y: 0, width: 1200, height: 320),
        cornerWidth: 30, cornerHeight: 30, transform: nil
    )
    context.addPath(panel)
    context.clip()
    gradient(
        context, from: 0x12283B, to: 0x091420,
        start: CGPoint(x: 0, y: 255), end: CGPoint(x: 1200, y: 25)
    )

    context.saveGState()
    context.translateBy(x: 28, y: 26)
    drawIcon(context, size: 264)
    context.restoreGState()

    drawText("DiskUse16x", size: 94, weight: .bold, color: 0xF2F8FF, at: CGPoint(x: 314, y: 168), in: context)
    drawText("Disk space analyzer", size: 36, weight: .medium, color: 0xB9CCD9, at: CGPoint(x: 318, y: 108), in: context)
    drawText("16X BRAINS", size: 23, weight: .semibold, color: 0x76F9E0, at: CGPoint(x: 320, y: 63), in: context)
}

/// The repository's social preview, 1280 × 640, with the content clear of the edges
/// that sites crop.
private func drawSocialPreview(_ context: CGContext) {
    gradient(
        context, from: 0x12283B, to: 0x091420,
        start: CGPoint(x: 0, y: 640), end: CGPoint(x: 1280, y: 0)
    )
    context.saveGState()
    context.translateBy(x: 96, y: 178)
    drawIcon(context, size: 300)
    context.restoreGState()

    drawText("DiskUse16x", size: 112, weight: .bold, color: 0xF2F8FF, at: CGPoint(x: 444, y: 352), in: context)
    drawText("See what's using your disk space", size: 40, weight: .medium, color: 0xB9CCD9, at: CGPoint(x: 450, y: 282), in: context)
    drawText("Mac app and terminal explorer · macOS and Linux", size: 28, weight: .regular, color: 0xB9CCD9, at: CGPoint(x: 451, y: 226), in: context)
    drawText("FREE AND OPEN SOURCE  ·  16X BRAINS", size: 23, weight: .semibold, color: 0x76F9E0, at: CGPoint(x: 452, y: 174), in: context)
}

private func writePNG(
    width: Int, height: Int, to destination: URL, draw: (CGContext) -> Void
) throws {
    guard let context = CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8,
        bytesPerRow: 0, space: rgb, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else {
        throw CocoaError(.fileWriteUnknown, userInfo: [NSURLErrorKey: destination])
    }
    draw(context)
    guard let image = context.makeImage(),
          let writer = CGImageDestinationCreateWithURL(destination as CFURL, UTType.png.identifier as CFString, 1, nil)
    else {
        throw CocoaError(.fileWriteUnknown, userInfo: [NSURLErrorKey: destination])
    }
    CGImageDestinationAddImage(writer, image, nil)
    guard CGImageDestinationFinalize(writer) else {
        throw CocoaError(.fileWriteUnknown, userInfo: [NSURLErrorKey: destination])
    }
}

private func buildAssets(in directory: URL) throws {
    let manager = FileManager.default
    let work = manager.temporaryDirectory.appendingPathComponent("diskuse16x-brand-\(UUID().uuidString)")
    let iconset = work.appendingPathComponent("AppIcon.iconset")
    try manager.createDirectory(at: iconset, withIntermediateDirectories: true)
    defer { try? manager.removeItem(at: work) }

    for size in [16, 32, 128, 256, 512] {
        for scale in [1, 2] {
            let suffix = scale == 2 ? "@2x" : ""
            let png = iconset.appendingPathComponent("icon_\(size)x\(size)\(suffix).png")
            let pixels = size * scale
            try writePNG(width: pixels, height: pixels, to: png) { drawIcon($0, size: CGFloat(pixels)) }
        }
    }

    let compiledIcon = work.appendingPathComponent("AppIcon.icns")
    let iconutil = Process()
    iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
    iconutil.arguments = ["--convert", "icns", "--output", compiledIcon.path, iconset.path]
    try iconutil.run()
    iconutil.waitUntilExit()
    guard iconutil.terminationStatus == 0 else { throw CocoaError(.fileWriteUnknown) }
    let destination = directory.appendingPathComponent("AppIcon.icns")
    try Data(contentsOf: compiledIcon).write(to: destination, options: .atomic)

    try writePNG(width: 1200, height: 320, to: directory.appendingPathComponent("Lockup.png"), draw: drawLockup)
    try writePNG(width: 1280, height: 640, to: directory.appendingPathComponent("SocialPreview.png"), draw: drawSocialPreview)
}

try buildAssets(in: URL(fileURLWithPath: #filePath).deletingLastPathComponent())
