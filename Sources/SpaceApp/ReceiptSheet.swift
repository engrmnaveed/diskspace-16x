import SwiftUI

/// Pro: names a Build Footprint receipt, such as “first Xcode build”, before saving it.
struct ReceiptSheet: View {
    let workspace: ScanWorkspace
    @Environment(\.dismiss) private var dismiss
    @ViewState private var label = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("Save Footprint Receipt", systemImage: "doc.text")
                .font(.title2.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
            Text("A receipt records what changed on disk since \(workspace.comparison.map { "“\($0.name)”" } ?? "the saved scan"): both scan times, the totals, and the files that grew, shrank, appeared or disappeared. Name the operation you measured.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            TextField("Operation", text: $label, prompt: Text("For example, first Xcode build"))
                .textFieldStyle(.roundedBorder)
                .onSubmit(save)
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save…", action: save)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(width: 460)
    }

    private func save() {
        dismiss()
        workspace.saveReceipt(label: label)
    }
}
