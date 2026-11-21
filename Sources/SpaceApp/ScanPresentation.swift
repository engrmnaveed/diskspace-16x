import AppKit
import SpaceCore
import SwiftUI

enum SpaceColor {
    static let accent = Color(nsColor: NSColor(name: nil) { appearance in
        let dark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        return dark
            ? NSColor(srgbRed: 77 / 255, green: 242 / 255, blue: 207 / 255, alpha: 1)
            : NSColor(srgbRed: 8 / 255, green: 105 / 255, blue: 88 / 255, alpha: 1)
    })
    static let onAccent = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 16 / 255, green: 34 / 255, blue: 56 / 255, alpha: 1)
            : .white
    })
    static let canvas = Color(nsColor: .windowBackgroundColor)
}

@MainActor
enum ScanFormat {
    private static let byteFormatter: ByteCountFormatter = {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        formatter.includesActualByteCount = false
        formatter.zeroPadsFractionDigits = true
        return formatter
    }()

    static func size(_ bytes: UInt64) -> String {
        if bytes == 0 { return exactSize(bytes) }
        if let signed = Int64(exactly: bytes) {
            return byteFormatter.string(fromByteCount: signed)
        }
        let exabytes = Double(bytes) / 1e18
        return exabytes.formatted(.number.precision(.fractionLength(2))) + " EB"
    }

    static func exactSize(_ bytes: UInt64) -> String {
        bytes == 1 ? "1 byte" : "\(bytes.formatted()) bytes"
    }

    /// Exact bytes plus the binary size, for tooltips beside a decimal size.
    static func sizeDetail(_ bytes: UInt64) -> String {
        bytes < 1024 ? exactSize(bytes) : "\(exactSize(bytes)) (\(ByteFormat.binary(bytes)))"
    }

    /// A signed change in size, such as "+1.2 GB" or "−300 MB".
    static func change(_ delta: Int64) -> String {
        if delta == 0 { return "No change" }
        let size = size(delta.magnitude)
        return delta > 0 ? "+\(size)" : "−\(size)"
    }

    static func items(_ count: Int) -> String {
        count == 1 ? "1 item" : "\(count.formatted()) items"
    }

    static func items(_ count: UInt64) -> String {
        count == 1 ? "1 item" : "\(count.formatted()) items"
    }

    static func excluded(_ count: UInt64) -> String {
        "\(count.formatted()) excluded"
    }

    static func protectedItems(_ count: Int) -> String {
        count == 1 ? "One protected item" : "\(count.formatted()) protected items"
    }
}

struct SpaceMark: View {
    let size: CGFloat

    private static let packagedIcon = Bundle.main.url(forResource: "AppIcon", withExtension: "icns")
        .flatMap { NSImage(contentsOf: $0) }

    var body: some View {
        Group {
            if let icon = Self.packagedIcon {
                Image(nsImage: icon)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .scaleEffect(1.20)
            } else {
                fallbackMark
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private var fallbackMark: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.23)
                .fill(
                    LinearGradient(
                        colors: [Color(red: 0.08, green: 0.16, blue: 0.25), Color(red: 0.035, green: 0.08, blue: 0.13)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
            Canvas { context, bounds in
                let center = CGPoint(x: bounds.width / 2, y: bounds.height / 2)
                let radius = bounds.width
                let sectors: [(Double, Double, Double, Color)] = [
                    (-61, 37, 0.29, Color(red: 0.30, green: 0.95, blue: 0.81)),
                    (41, 189, 0.18, Color(red: 0.31, green: 0.45, blue: 0.64)),
                    (193, 295, 0.23, Color(red: 0.31, green: 0.65, blue: 0.74))
                ]
                for (start, end, scale, color) in sectors {
                    var wedge = Path()
                    wedge.addArc(
                        center: center, radius: radius * scale,
                        startAngle: .degrees(start), endAngle: .degrees(end), clockwise: false
                    )
                    wedge.addArc(
                        center: center, radius: radius * 0.109,
                        startAngle: .degrees(end), endAngle: .degrees(start), clockwise: true
                    )
                    wedge.closeSubpath()
                    context.fill(wedge, with: .color(color))
                }
                let origin = CGRect(x: center.x - radius * 0.017, y: center.y - radius * 0.017,
                                    width: radius * 0.034, height: radius * 0.034)
                context.fill(Path(ellipseIn: origin), with: .color(.white.opacity(0.75)))
            }
        }
    }
}
