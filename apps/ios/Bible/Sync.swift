import Foundation

/// Sync with the Mac (Orthodox fork) through a folder in iCloud Drive that the person picked:
/// each device writes its changes to a file of its own there and merges the others'
/// (`bible_core::sync`). The folder is kept as a bookmark, so no iCloud entitlement is needed.
enum FolderSync {
    /// What one sync did.
    struct Outcome {
        var report: SyncReport?
        /// Why it couldn't sync, as a sentence to show.
        var error: String?
        /// "Orthodox Bible › Sync", once the folder is found.
        var folder: String?
        /// A fresh bookmark, when the saved one went stale.
        var bookmark: Data?
        /// Files still in iCloud, or not yet their latest version, asked for: there for a later sync.
        var downloading = 0
    }

    static func bookmark(_ url: URL) throws -> Data {
        try url.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil)
    }

    /// The folder and the one it's in: "Orthodox Bible › Sync".
    static func name(_ url: URL) -> String {
        url.pathComponents.suffix(2).joined(separator: " › ")
    }

    /// The folder the Mac syncs: Orthodox Bible › Sync, and not in Recently Deleted.
    static func isMacs(_ url: URL) -> Bool {
        !url.path.contains("/.Trash/") && url.pathComponents.suffix(2) == ["Orthodox Bible", "Sync"]
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
        outcome.folder = name(url)
        // A bookmark follows its folder anywhere, into Recently Deleted too, while the Mac keeps
        // to its place: a moved folder isn't followed.
        if stale && !isMacs(url) {
            outcome.error = "The sync folder was moved or deleted. Choose iCloud Drive › Orthodox Bible › Sync again."
            return outcome
        }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        if !scoped && !FileManager.default.isReadableFile(atPath: url.path) {
            outcome.error = "The app can no longer open the sync folder. Choose it again."
            return outcome
        }
        if stale { outcome.bookmark = try? Self.bookmark(url) }
        var coordination: NSError?
        NSFileCoordinator().coordinate(writingItemAt: url, options: .forMerging, error: &coordination) { url in
            outcome.downloading = download(url)
            do {
                outcome.report = try bible.sync(folder: url.path)
            } catch BibleError.Failed(let message) {
                outcome.error = message
            } catch {
                outcome.error = "Couldn’t sync. Try again in a moment."
            }
        }
        if coordination != nil { outcome.error = "The sync folder couldn’t be opened. Try again in a moment." }
        return outcome
    }

    /// Asks iCloud for the sync files it hasn't brought down, or not in their latest version:
    /// the core would skip a placeholder (".<name>.icloud") and read an old copy as it is.
    /// Returns how many.
    private static func download(_ folder: URL) -> Int {
        let fm = FileManager.default
        var waiting = 0
        for name in (try? fm.contentsOfDirectory(atPath: folder.path)) ?? [] {
            let file: URL
            if name.hasPrefix("."), name.hasSuffix(".json.icloud") {
                file = folder.appendingPathComponent(String(name.dropFirst().dropLast(".icloud".count)))
            } else if !name.hasPrefix("."), name.hasSuffix(".json") {
                file = folder.appendingPathComponent(name)
                let values = try? file.resourceValues(forKeys: [.ubiquitousItemDownloadingStatusKey])
                // Not in iCloud (a test's folder), or already its latest version.
                guard let status = values?.ubiquitousItemDownloadingStatus, status != .current else { continue }
            } else {
                continue
            }
            try? fm.startDownloadingUbiquitousItem(at: file)
            waiting += 1
        }
        return waiting
    }
}
