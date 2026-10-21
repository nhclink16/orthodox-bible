import SwiftUI

// GATE (temporary, removed after the gate): `--passage-gate <item id>` shows that rule item's
// parts on 21 Oct 2026, to set beside the reader.
struct PassageGate: View {
    @Environment(Store.self) private var store
    let item: String
    let theme: Theme

    static var item: String? {
        let a = ProcessInfo.processInfo.arguments
        return a.firstIndex(of: "--passage-gate").flatMap { a.indices.contains($0 + 1) ? a[$0 + 1] : nil }
    }

    var body: some View {
        let date = Calendar.current.date(from: DateComponents(year: 2026, month: 10, day: 21))!
        let found = store.ruleDay(date).blocks.flatMap(\.items).first { $0.id == item }
        NavigationStack {
            PassageView(parts: found?.parts ?? [], intro: found?.detail, theme: theme, place: nil, reached: { _, _ in }) {
                Text("footer")
            }
            .navigationTitle(found?.title ?? "")
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}
