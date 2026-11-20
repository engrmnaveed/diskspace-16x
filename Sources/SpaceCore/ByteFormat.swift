/// Size text for detail views. Primary sizes use ByteCountFormatter's decimal file style, as Finder does.
public enum ByteFormat {
    private static let binaryUnits = ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"]
    // Kilo units show whole numbers, mega units one decimal, and larger units two.
    private static let fractionDigits = [0, 1, 2, 2, 2, 2]

    /// Formats a size in binary units with the same rounding as the CLI's detail panel.
    public static func binary(_ bytes: UInt64) -> String {
        let base: UInt64 = 1024
        guard bytes >= base else { return "\(bytes) B" }
        var divisor = base
        for (index, unit) in binaryUnits.enumerated() {
            let digits = fractionDigits[index]
            let scale: UInt64 = digits == 0 ? 1 : digits == 1 ? 10 : 100
            // (bytes × scale × 2 + divisor) ÷ (divisor × 2) rounds ties up. Full-width math avoids overflow.
            let product = bytes.multipliedFullWidth(by: scale * 2)
            let (low, carry) = product.low.addingReportingOverflow(divisor)
            let rounded = (divisor * 2).dividingFullWidth((product.high + (carry ? 1 : 0), low)).quotient
            if rounded < base * scale || index == binaryUnits.count - 1 {
                let whole = rounded / scale
                guard digits > 0 else { return "\(whole) \(unit)" }
                let fraction = String(rounded % scale)
                let padding = String(repeating: "0", count: digits - fraction.count)
                return "\(whole).\(padding)\(fraction) \(unit)"
            }
            divisor *= base
        }
        preconditionFailure("The largest unit accepts every byte count.")
    }
}
