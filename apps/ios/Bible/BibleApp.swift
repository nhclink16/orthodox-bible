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
                    store.syncNow()
                    // The search text is built once the first page is up.
                    try? await Task.sleep(for: .milliseconds(300))
                    store.warm()
                    await store.refreshReminders()
                }
        }
        .onChange(of: phase) {
            if phase != .active { store.save() }
            if phase == .background { store.syncInBackground() }
            // Notifications may have been allowed or turned off in Settings meanwhile, and the
            // Mac may have synced.
            if phase == .active {
                Task { await store.refreshReminders() }
                store.syncNow()
            }
        }
    }
}
