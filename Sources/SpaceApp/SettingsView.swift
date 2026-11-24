import SpaceCore
import SpacePro
import SwiftUI

/// Settings: exclude patterns the next scan skips without reading them, and the switch
/// for Pro features.
struct SettingsView: View {
    @AppStorage(ExcludeSettings.key) private var text = ""
    @AppStorage(ProFeatures.defaultsKey) private var proFeatures = false
    @AppStorage(LowSpaceAlert.monitorKey) private var showsFreeSpace = false
    @AppStorage(LowSpaceAlert.thresholdKey) private var thresholdGB = LowSpaceAlert.defaultThresholdGB

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

            Section {
                // A grouped form draws each label apart from its control and leaves the control
                // unnamed in the accessibility tree, so name each one for VoiceOver.
                Toggle("Turn on Pro features", isOn: $proFeatures)
                    .accessibilityLabel("Turn on Pro features")
                if proFeatures {
                    Toggle("Show free space in the menu bar", isOn: $showsFreeSpace)
                        .accessibilityLabel("Show free space in the menu bar")
                    Picker("Notify when free space drops below", selection: $thresholdGB) {
                        ForEach(LowSpaceAlert.thresholdChoices, id: \.self) { gigabytes in
                            Text(gigabytes == 0 ? "Never" : "\(gigabytes) GB").tag(gigabytes)
                        }
                    }
                    .accessibilityLabel("Notify when free space drops below")
                    .disabled(!showsFreeSpace)
                }
            } header: {
                Text("Pro features")
            } footer: {
                Text("DiskUse16x Pro is free too. It adds **Compare** and a scan history, a **Modified** column, **Types**, **Cleanup** and **Duplicates**, **Explain Disk Space**, and a menu-bar free-space monitor that checks once a minute while it's shown. Pro features stay out of the way until you turn them on.")
                    .font(.callout)
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
        .fixedSize(horizontal: false, vertical: true)
    }
}
