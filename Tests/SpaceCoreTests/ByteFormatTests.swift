@testable import SpaceCore
import XCTest

final class ByteFormatTests: XCTestCase {
    func testFormatsBinarySizesWithIECUnits() {
        // Keep in sync with formats_binary_sizes_with_iec_units in cli/src/report.rs.
        let cases: [(UInt64, String)] = [
            (1_023, "1023 B"),
            (1_024, "1 KiB"),
            (1_535, "1 KiB"),
            (1_536, "2 KiB"),
            (2_560, "3 KiB"),
            (1_048_063, "1023 KiB"),
            (1_048_064, "1.0 MiB"),
            (1_101_004, "1.0 MiB"),
            (1_101_005, "1.1 MiB"),
            (1_073_689_395, "1023.9 MiB"),
            (1_073_689_396, "1.00 GiB"),
            (45_888_520_192, "42.74 GiB"),
            (245_349_000_000, "228.50 GiB"),
            (1_099_506_259_066, "1023.99 GiB"),
            (1_099_506_259_067, "1.00 TiB"),
            (.max, "16.00 EiB"),
        ]
        for (bytes, expected) in cases {
            XCTAssertEqual(ByteFormat.binary(bytes), expected, "\(bytes) bytes")
        }
    }
}
