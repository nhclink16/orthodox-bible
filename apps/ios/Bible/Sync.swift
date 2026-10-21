import Foundation

/// Sync with the Mac (Orthodox fork) through a folder in iCloud Drive that the person picked:
/// each device writes its changes to a file of its own there and merges the others'
/// (`bible_core::sync`). The folder is kept as a bookmark, so no iCloud entitlement is needed.
enum FolderSync {
    /// What one sync did.
    struct Outcome {
        var report: SyncReport?
        var error: String?
        /// "Orthodox Bible › Sync", once the folder is found.
        var folder: String?
        /// A fresh bookmark, when the saved one went stale.
        var bookmark: Data?
        /// Files still in iCloud, asked for: there for the next sync.
        var downloading = 0
    }

    static func bookmark(_ url: URL) throws -> Data {
        try url.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil)
    }

    /// The folder and the one it's in: "Orthodox Bible › Sync".
    static func name(_ url: URL) -> String {
        url.pathComponents.suffix(2).joined(separator: " › ")
    }

    /// Opens the folder from its bookmark and syncs it, coordinated with iCloud. It can wait on
    /// the file system: call it off the main thread.
    static func run(_ bible: Bible, bookmark: Data) -> Outcome {
        var outcome = Outcome()
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: bookmark, bookmarkDataIsStale: &stale) else {
            outcome.error = "The sync folder can’t be found. Choose it again."
            return outcome
        }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        outcome.folder = name(url)
        if stale { outcome.bookmark = try? Self.bookmark(url) }
        var coordination: NSError?
        NSFileCoordinator().coordinate(writingItemAt: url, options: .forMerging, error: &coordination) { url in
            outcome.downloading = download(url)
            do {
                outcome.report = try bible.sync(folder: url.path)
            } catch {
                outcome.error = error.localizedDescription
            }
        }
        if let coordination { outcome.error = coordination.localizedDescription }
        return outcome
    }

    /// iCloud keeps files it hasn't downloaded as ".<name>.icloud", which the core skips: asks
    /// for each. Returns how many.
    private static func download(_ folder: URL) -> Int {
        let fm = FileManager.default
        let waiting = ((try? fm.contentsOfDirectory(atPath: folder.path)) ?? [])
            .filter { $0.hasPrefix(".") && $0.hasSuffix(".icloud") }
        for name in waiting {
            let file = String(name.dropFirst().dropLast(".icloud".count))
            try? fm.startDownloadingUbiquitousItem(at: folder.appendingPathComponent(file))
        }
        return waiting.count
    }
}
