import SpacePro
import SwiftUI

/// Pro: names a Build Footprint receipt, such as “first Xcode build”, before saving it, with
/// an optional size budget remembered for each operation name.
struct ReceiptSheet: View {
    let workspace: ScanWorkspace
    @Environment(\.dismiss) private var dismiss
    @ViewState private var label = ""
    @ViewState private var budgetText = ""

    /// The budget, or nil when the field is empty. Unreadable text keeps Save disabled.
    private var budget: UInt64? { FootprintReceipt.parseBudget(budgetText) }
    private var budgetIsValid: Bool {
        budgetText.trimmingCharacters(in: .whitespaces).isEmpty || budget != nil
    }

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
                // With a prompt, SwiftUI shows no title and gives VoiceOver none either.
                .accessibilityLabel("Operation name")
                .onSubmit(save)
                .onChange(of: label) { _, name in
                    if let remembered = Self.budgets[Self.key(name)] { budgetText = remembered }
                }
            VStack(alignment: .leading, spacing: 4) {
                TextField("Budget", text: $budgetText, prompt: Text("Optional budget, for example 500 MB"))
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel("Budget, optional")
                    .onSubmit(save)
                Text(budgetIsValid
                    ? "The receipt says whether the operation grew by more than this. DiskUse16x remembers it for the operation’s name."
                    : "Enter a size such as 500 MB or 2 GB.")
                    .font(.callout)
                    .foregroundStyle(budgetIsValid ? Color.secondary : Color.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save…", action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!budgetIsValid)
            }
        }
        .padding(24)
        .frame(width: 460)
    }

    private func save() {
        guard budgetIsValid else { return }
        var budgets = Self.budgets
        let key = Self.key(label)
        if !key.isEmpty {
            budgets[key] = budget == nil ? nil : budgetText.trimmingCharacters(in: .whitespaces)
            UserDefaults.standard.set(budgets, forKey: FootprintReceipt.budgetsKey)
        }
        dismiss()
        workspace.saveReceipt(label: label, budget: budget)
    }

    /// Each operation's last budget as typed, by its lowercased name.
    private static var budgets: [String: String] {
        UserDefaults.standard.dictionary(forKey: FootprintReceipt.budgetsKey) as? [String: String] ?? [:]
    }

    private static func key(_ name: String) -> String {
        name.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }
}
