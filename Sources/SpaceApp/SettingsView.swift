import SpaceCore
import SwiftUI

/// Settings → Exclude: patterns the next scan skips without reading them.
struct SettingsView: View {
    @AppStorage(ExcludeSettings.key) private var text = ""

    private var validation: String? {
        do {
            _ = try ExcludeRules(patterns: ExcludeRules.patterns(fromLines: text))
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    var body: some View {
        Form {
            Section {
                TextEditor(text: $text)
                    .font(.body.monospaced())
                    .frame(minHeight: 150)
                    .accessibilityLabel("Exclude patterns, one per line")
                if let validation {
                    Label(validation, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .font(.callout)
                }
            } header: {
                Text("Exclude from scans")
            } footer: {
                VStack(alignment: .leading, spacing: 6) {
                    Text("One pattern per line. Matching files and folders are skipped without being read, and the next scan applies your changes.")
                    Text("`node_modules` or `*.log` match a name anywhere · `/build` matches at the top of the scanned folder · `cache/` matches folders only · `**` spans folders · lines starting with `#` are comments")
                        .foregroundStyle(.secondary)
                }
                .font(.callout)
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
        .fixedSize(horizontal: false, vertical: true)
    }
}
