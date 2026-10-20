import SwiftUI

@main
struct BibleApp: App {
    @State private var store = Store()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .task {
                    // The search text is built once the first page is up.
                    try? await Task.sleep(for: .milliseconds(300))
                    store.warm()
                    await store.refreshReminders()
                }
        }
        .onChange(of: phase) {
            if phase != .active { store.save() }
            // Notifications may have been allowed or turned off in Settings meanwhile.
            if phase == .active { Task { await store.refreshReminders() } }
        }
    }
}
