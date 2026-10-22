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

    static let sourceURL = URL(string: "https://github.com/HistoricalChristianFaith/Commentaries-Database/releases")!

    var body: some View {
        NavigationStack {
            Group {
                if !installed {
                    VStack {
                        FathersInstall(theme: theme, installed: load)
                        Spacer()
                    }
                    .padding(20)
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
    }

    private var label: String { rangeLabel(from: ref, to: ref) }

    private var list: some View {
        List {
            ForEach(Array(quotes.enumerated()), id: \.offset) { _, q in
                FatherQuoteRow(quote: q, ref: ref, theme: theme)
                    .padding(.vertical, 6)
            }
            Section {
                Text(FathersInstall.credit).font(.footnote).foregroundStyle(.secondary)
            }
        }
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
}

/// The Fathers' quotes on a verse, oldest first, for the Fathers step in Today: under the verse,
/// in its scroll.
struct FathersQuotes: View {
    @Environment(Store.self) private var store
    let ref: VerseRef
    let theme: Theme
    @State private var quotes: [FatherQuote]?

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            if quotes?.isEmpty == true {
                Text("Nothing from the Fathers on this verse.")
                    .font(.reading(17, italic: true))
                    .foregroundStyle(Color(theme.ink2))
            }
            ForEach(Array((quotes ?? []).enumerated()), id: \.offset) { _, q in
                FatherQuoteRow(quote: q, ref: ref, theme: theme)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .task(id: ref) { quotes = (try? store.bible.fathers(verse: ref)) ?? [] }
    }
}

/// One Father's words on a verse: who and where, the first lines until asked for the rest, and
/// Add to Note.
private struct FatherQuoteRow: View {
    @Environment(Store.self) private var store
    let quote: FatherQuote
    let ref: VerseRef
    let theme: Theme
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(quote.author).font(.subheadline.weight(.semibold)).foregroundStyle(Color(theme.ink))
                Text(String(quote.year)).font(.caption).foregroundStyle(.secondary)
            }
            if !quote.source.isEmpty {
                Text(quote.source).font(.caption).foregroundStyle(.secondary)
            }
            Text(quote.text)
                .font(.reading(17))
                .lineLimit(open ? nil : 6)
                .foregroundStyle(Color(theme.ink))
            HStack(spacing: 20) {
                if quote.text.count > 360 {
                    Button(open ? "Show less" : "Show more") { open.toggle() }
                }
                Button("Add to Note") { store.appendToNote(ref, FathersView.note(quote)) }
            }
            .font(.footnote.weight(.medium))
            .buttonStyle(.borderless)
        }
    }
}

/// Where to get the Fathers' quotes, and installing them from Files.
struct FathersInstall: View {
    @Environment(Store.self) private var store
    let theme: Theme
    let installed: () -> Void
    @State private var importing = false

    static let credit =
        "From the Historical Christian Faith Commentaries Database: Fathers before 1054 the Orthodox Church receives."

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Quotes from the Fathers aren't installed yet.")
                .font(.headline)
            Text("Download commentaries.sqlite (about 150 MB) from the Historical Christian Faith Commentaries Database, then choose it here. It stays on this device.")
                .font(.subheadline).foregroundStyle(.secondary)
            HStack(spacing: 12) {
                Link("Download Page", destination: FathersView.sourceURL)
                    .buttonStyle(.bordered)
                Button("Install from Files…") { importing = true }
                    .buttonStyle(.borderedProminent)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .fileImporter(isPresented: $importing, allowedContentTypes: [UTType(filenameExtension: "sqlite") ?? .data]) {
            result in
            guard case .success(let url) = result else { return }
            install(from: url)
        }
    }

    private func install(from url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let kept = try store.bible.installFathers(path: url.path)
            store.show("Installed \(kept) quotes from the Fathers")
            installed()
        } catch {
            store.show("Couldn't install the Fathers: \(error.localizedDescription)")
        }
    }
}
