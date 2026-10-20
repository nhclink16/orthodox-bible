import SwiftUI
import UniformTypeIdentifiers

/// The Church Fathers on a verse (Orthodox fork), oldest first. Until the quotes are installed it
/// explains where to get the database and installs it from Files.
struct FathersView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let ref: VerseRef
    let theme: Theme
    @State private var quotes: [FatherQuote] = []
    @State private var installed = false
    @State private var importing = false
    @State private var open: Set<Int> = []

    static let sourceURL = URL(string: "https://github.com/HistoricalChristianFaith/Commentaries-Database/releases")!

    var body: some View {
        NavigationStack {
            Group {
                if !installed {
                    notInstalled
                } else if quotes.isEmpty {
                    ContentUnavailableView(
                        "No quotes on this verse", systemImage: "text.book.closed",
                        description: Text("The Fathers in the database don't comment on \(label)."))
                } else {
                    list
                }
            }
            .navigationTitle("Church Fathers")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 0) {
                        Text("Church Fathers").font(.headline)
                        Text(installed ? "\(label) · \(quotes.count) quotes" : label)
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .onAppear(perform: load)
        .fileImporter(isPresented: $importing, allowedContentTypes: [UTType(filenameExtension: "sqlite") ?? .data]) {
            result in
            guard case .success(let url) = result else { return }
            install(from: url)
        }
    }

    private var label: String { rangeLabel(from: ref, to: ref) }

    private var list: some View {
        List {
            ForEach(Array(quotes.enumerated()), id: \.offset) { i, q in
                VStack(alignment: .leading, spacing: 6) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(q.author).font(.subheadline.weight(.semibold))
                        Text(String(q.year)).font(.caption).foregroundStyle(.secondary)
                    }
                    if !q.source.isEmpty {
                        Text(q.source).font(.caption).foregroundStyle(.secondary)
                    }
                    Text(q.text)
                        .font(Font(Fonts.literata(17)))
                        .lineLimit(open.contains(i) ? nil : 6)
                        .foregroundStyle(Color(theme.ink))
                    HStack(spacing: 20) {
                        if q.text.count > 360 {
                            Button(open.contains(i) ? "Show less" : "Show more") {
                                if open.contains(i) { open.remove(i) } else { open.insert(i) }
                            }
                        }
                        Button("Add to Note") { store.appendToNote(ref, Self.note(q)) }
                    }
                    .font(.footnote.weight(.medium))
                    .buttonStyle(.borderless)
                }
                .padding(.vertical, 6)
            }
            Section {
                Text("From the Historical Christian Faith Commentaries Database: Fathers before 1054 the Orthodox Church receives.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
    }

    private var notInstalled: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Quotes from the Fathers aren't installed yet.")
                .font(.headline)
            Text("Download commentaries.sqlite (about 150 MB) from the Historical Christian Faith Commentaries Database, then choose it here. It stays on this device.")
                .font(.subheadline).foregroundStyle(.secondary)
            HStack(spacing: 12) {
                Link("Download Page", destination: Self.sourceURL)
                    .buttonStyle(.bordered)
                Button("Install from Files…") { importing = true }
                    .buttonStyle(.borderedProminent)
            }
            Spacer()
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The quote as it goes into a note, as on the Mac (`bible_core::fathers::Quote::as_note`).
    static func note(_ q: FatherQuote) -> String {
        let source = q.source.isEmpty ? "" : ", \(q.source)"
        return "\u{201C}\(q.text.trimmingCharacters(in: .whitespacesAndNewlines))\u{201D}\n\u{2014} \(q.author)\(source)"
    }

    private func load() {
        installed = store.bible.fathersInstalled()
        quotes = (try? store.bible.fathers(verse: ref)) ?? []
    }

    private func install(from url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let kept = try store.bible.installFathers(path: url.path)
            store.show("Installed \(kept) quotes from the Fathers")
            load()
        } catch {
            store.show("Couldn't install the Fathers: \(error.localizedDescription)")
        }
    }
}
