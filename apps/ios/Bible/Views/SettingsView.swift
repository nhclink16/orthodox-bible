import SwiftUI
import UniformTypeIdentifiers

/// Theme, type and reading options, applied as you change them.
struct SettingsView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let theme: Theme
    /// Turning sync on: which folder to choose, then the Files picker for it.
    @State private var choosing = false
    @State private var picking = false
    /// A folder picked that isn't the one the Mac syncs, to confirm.
    @State private var unlike: URL?

    var body: some View {
        @Bindable var store = store
        NavigationStack {
            Form {
                Section {
                    HStack(spacing: 12) {
                        ForEach(Appearance.allCases) { appearance in
                            ThemeSwatch(
                                appearance: appearance, selected: store.settings.appearance == appearance, accent: theme
                            ) {
                                store.settings.appearance = appearance
                            }
                        }
                    }
                    .padding(.vertical, 6)
                }

                Section("Text") {
                    HStack(spacing: 14) {
                        Image(systemName: "textformat.size.smaller")
                            .foregroundStyle(.secondary)
                        Slider(value: $store.settings.textSize, in: ReaderSettings.textSizes, step: 1)
                            .accessibilityLabel("Text size")
                            .accessibilityValue("\(Int(store.settings.textSize)) points")
                        Image(systemName: "textformat.size.larger")
                            .foregroundStyle(.secondary)
                    }
                    HStack(spacing: 14) {
                        Image(systemName: "text.alignleft")
                            .foregroundStyle(.secondary)
                        Slider(value: $store.settings.lineSpacing, in: ReaderSettings.lineSpacings, step: 0.1)
                            .accessibilityLabel("Line spacing")
                            .accessibilityValue(store.settings.lineSpacing.formatted(.number.precision(.fractionLength(1))))
                        Image(systemName: "text.justify")
                            .foregroundStyle(.secondary)
                    }
                    Text("Text follows the system’s text size too.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }

                Section("Reading") {
                    Toggle("Words of Jesus in red", isOn: $store.settings.redLetters)
                    Toggle("Verse numbers", isOn: $store.settings.verseNumbers)
                    Toggle("One verse per line", isOn: $store.settings.versePerLine)
                    Toggle("Section headings", isOn: $store.settings.headings)
                    Toggle("Search whole words only", isOn: $store.settings.wholeWords)
                    Toggle("Septuagint psalm numbers", isOn: $store.settings.septuagintPsalms)
                }

                Section {
                    Picker("Calendar", selection: $store.settings.oldCalendar) {
                        Text("New").tag(false)
                        Text("Old (Julian)").tag(true)
                    }
                    Picker("Readings", selection: $store.settings.usage) {
                        Text("Greek Orthodox Archdiocese").tag(ReadingUsage.greek)
                        Text("Antiochian Archdiocese").tag(ReadingUsage.antiochian)
                    }
                    // A page of their own: the names are too long for a menu's width.
                    .pickerStyle(.navigationLink)
                } header: {
                    Text("Church calendar")
                } footer: {
                    Text("Which archdiocese’s calendar of readings to follow. Your parish’s bulletin is the best check.")
                }

                Section("Notes and highlights") {
                    ExportButton(title: "Export as Markdown", format: .markdown, file: "Bible notes.md")
                    ExportButton(title: "Export as JSON", format: .json, file: "Bible notes.json")
                }

                Section {
                    Toggle("Sync with your Mac", isOn: Binding(get: { store.syncOn }, set: turnSync))
                        .accessibilityIdentifier("sync")
                    if store.syncOn {
                        if let folder = store.syncFolder {
                            LabeledContent("Folder", value: folder)
                        }
                        Button("Sync now") { store.syncNow() }
                            .disabled(store.syncing)
                            .accessibilityIdentifier("sync-now")
                        if store.syncError != nil {
                            Button("Choose folder…") { choosing = true }
                        }
                    }
                } footer: {
                    VStack(alignment: .leading, spacing: 6) {
                        if store.syncOn { SyncStatus() }
                        Text("Through iCloud Drive: notes, highlights, bookmarks, reading plans, your prayer rule and what you’ve ticked.")
                    }
                }

                if let t = store.translation {
                    Section("About the \(t.abbrev)") {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(t.name).font(.headline)
                            if let description = t.description {
                                Text(description).font(.subheadline).foregroundStyle(.secondary)
                            }
                            Text(t.copyright.isEmpty ? t.license : t.copyright)
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 2)
                    }
                }

                Section {
                    LabeledContent("Version", value: Self.version)
                }
            }
            .navigationTitle("Reading")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationBackgroundInteraction(.enabled(upThrough: .medium))
        // The Files picker can't carry a line of its own: which folder to choose comes first.
        .confirmationDialog("Sync with your Mac", isPresented: $choosing, titleVisibility: .visible) {
            Button("Choose folder") { picking = true }
        } message: {
            Text("Choose iCloud Drive › Orthodox Bible › Sync. The Mac app makes this folder; you can also make it in Files.")
        }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result else { return }
            if FolderSync.isMacs(url) { store.startSync(folder: url) } else { unlike = url }
        }
        .confirmationDialog(
            "This isn’t the Orthodox Bible › Sync folder your Mac uses. Use it anyway?",
            isPresented: Binding(get: { unlike != nil }, set: { if !$0 { unlike = nil } }),
            titleVisibility: .visible, presenting: unlike
        ) { url in
            Button("Use folder") { store.startSync(folder: url) }
            Button("Choose again") { picking = true }
        }
    }

    /// On asks for the folder (a UI test's own, if it named one); off forgets it.
    private func turnSync(_ on: Bool) {
        guard on else { return store.stopSync() }
        if let folder = Store.testSyncFolder {
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            store.startSync(folder: folder)
        } else {
            choosing = true
        }
    }

    static var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "?"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "\(short) (\(build))"
    }
}

private struct ThemeSwatch: View {
    let appearance: Appearance
    let selected: Bool
    let accent: Theme
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                ZStack {
                    if appearance == .system {
                        HStack(spacing: 0) {
                            Color(Theme.light.paper)
                            Color(Theme.dark.paper)
                        }
                    } else {
                        Color(Theme.named(appearance.resolved(.light)).paper)
                    }
                    Text("Aa")
                        .font(.reading(17))
                        .foregroundStyle(appearance == .system ? Color.gray : Color(Theme.named(appearance.resolved(.light)).ink))
                }
                .frame(height: 52)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(selected ? Color(accent.accent) : Color(accent.border), lineWidth: selected ? 2.5 : 1)
                }
                Text(appearance.label)
                    .font(.caption.weight(selected ? .semibold : .regular))
                    .foregroundStyle(selected ? Color(accent.accent) : .secondary)
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(appearance.label) theme")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// When the Mac was last synced with, or why it couldn't be.
private struct SyncStatus: View {
    @Environment(Store.self) private var store

    var body: some View {
        // Drawn again a minute after each sync, and each minute after that.
        TimelineView(.periodic(from: store.lastSynced ?? .now, by: 60)) { context in
            VStack(alignment: .leading, spacing: 2) {
                if let error = store.syncError {
                    Text(error)
                } else if store.syncWaiting > 0 {
                    Text("Waiting for iCloud Drive…")
                } else if let last = store.lastSynced {
                    let when = context.date.timeIntervalSince(last) < 60
                        ? "Last synced just now"
                        : "Last synced \(last.formatted(.relative(presentation: .named, unitsStyle: .wide)))"
                    Text(store.syncDevices == 0 ? "\(when) · nothing from your Mac in this folder yet" : when)
                } else if store.syncing {
                    Text("Syncing…")
                }
                ForEach(store.syncSkipped, id: \.self) { Text("Couldn’t read \($0)") }
            }
        }
    }
}

/// Writes the export to a file and offers it to share or save.
private struct ExportButton: View {
    @Environment(Store.self) private var store
    let title: String
    let format: ExportFormat
    let file: String
    @State private var url: URL?

    var body: some View {
        Group {
            if let url {
                ShareLink(item: url) { Label(title, systemImage: "square.and.arrow.up") }
            } else {
                Label(title, systemImage: "square.and.arrow.up").foregroundStyle(.secondary)
            }
        }
        .task {
            guard let text = store.export(format) else { return }
            let url = FileManager.default.temporaryDirectory.appendingPathComponent(file)
            if (try? text.write(to: url, atomically: true, encoding: .utf8)) != nil { self.url = url }
        }
    }
}
