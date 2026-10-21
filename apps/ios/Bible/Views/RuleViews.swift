import SwiftUI

/// A round check box. Done shows as done: no counts, no streaks.
struct CheckButton: View {
    let done: Bool
    let theme: Theme
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: done ? "checkmark.circle.fill" : "circle")
                .font(.system(size: 22, weight: .regular))
                .foregroundStyle(done ? Color(theme.accent) : Color(theme.ink3))
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .accessibilityValue(done ? "Done" : "Not done")
        .accessibilityAddTraits(done ? .isSelected : [])
    }
}

/// An item's title, note and what it opens on the day.
struct RuleItemLabel: View {
    let item: RuleDayItem
    let theme: Theme
    /// Opens a chapter in the reader.
    var reads = false

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .foregroundStyle(item.done || item.open == .notToday ? Color(theme.ink2) : Color(theme.ink))
                let line = [item.optional ? "Optional" : nil, item.note.isEmpty ? nil : item.note].compactMap { $0 }
                if !line.isEmpty {
                    Text(line.joined(separator: " · "))
                        .font(.subheadline)
                        .foregroundStyle(Color(theme.ink2))
                }
                if let detail = item.detail {
                    Text(detail)
                        .font(.footnote)
                        .foregroundStyle(
                            [.nothing, .notToday].contains(item.open) ? Color(theme.ink3) : Color(theme.accent))
                }
            }
            Spacer(minLength: 0)
            if reads {
                Image(systemName: "book")
                    .font(.footnote)
                    .foregroundStyle(Color(theme.ink3))
            } else if case .fathers = item.open {
                // As the reader's Fathers action shows it.
                Image(systemName: "text.book.closed")
                    .font(.footnote)
                    .foregroundStyle(Color(theme.ink3))
                    .accessibilityHidden(true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .padding(.vertical, 2)
    }
}

/// A prayer to read, or the Jesus Prayer to count, with its check box for the day.
struct RuleItemView: View {
    @Environment(Store.self) private var store
    let item: RuleDayItem
    let date: Date
    let theme: Theme

    /// Whether it's checked off now (it may change here).
    private var done: Bool {
        store.ruleDay(date).blocks.flatMap(\.items).first { $0.id == item.id }?.done ?? item.done
    }

    private var future: Bool { CivilDate(date).number > CivilDate(Date()).number }

    var body: some View {
        Group {
            switch item.open {
            case .text(let title, let text, let source):
                PrayerText(title: title, text: text, source: source, theme: theme) { mark }
            case .jesusPrayer(let count):
                JesusPrayerCounter(count: Int(count), theme: theme) {
                    if !done && !future { set(true) }
                }
            default:
                EmptyView()
            }
        }
        // A prayer carries its title on the page.
        .navigationTitle(isCount ? item.title : "")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var isCount: Bool {
        if case .jesusPrayer = item.open { return true }
        return false
    }

    private var mark: some View {
        Button {
            set(!done)
        } label: {
            Label(done ? "Done" : "Mark as done", systemImage: done ? "checkmark.circle.fill" : "circle")
        }
        .buttonStyle(.bordered)
        .disabled(future)
        .accessibilityIdentifier("mark-done")
    }

    private func set(_ on: Bool) {
        store.setDone(item, day: CivilDate(date).number, on)
    }
}

/// A prayer in the reader's type, on its paper.
private struct PrayerText<Footer: View>: View {
    let title: String
    let text: String
    let source: String?
    let theme: Theme
    @ViewBuilder let footer: Footer

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text(title)
                    .font(.reading(24))
                    .foregroundStyle(Color(theme.ink))
                    .accessibilityAddTraits(.isHeader)
                Text(text)
                    .font(.reading(20))
                    .lineSpacing(9)
                    .foregroundStyle(Color(theme.ink))
                    .textSelection(.enabled)
                if let source {
                    Text(source).font(.footnote).foregroundStyle(Color(theme.ink2))
                }
                footer.padding(.top, 8)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 28)
            .frame(maxWidth: 640, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(Color(theme.paper))
    }
}

/// The Jesus Prayer, counted on a large, quiet tap target: a light tick each time and a clear
/// finish at the count.
private struct JesusPrayerCounter: View {
    let count: Int
    let theme: Theme
    let finished: () -> Void
    @State private var said = 0

    private var done: Bool { said >= count }
    /// The words, as the core's public-domain source prints them.
    private let words = prayers().first { $0.id == "jesus-prayer" }?.text
        ?? "Lord Jesus Christ, Son of God, have mercy upon me a sinner."

    var body: some View {
        // The whole prayer always shows: the page scrolls when it can't fit (large text, or the
        // phone on its side), and the tap target fills what's left when it can.
        ViewThatFits(in: .vertical) {
            page
            ScrollView { page }
        }
        .background(Color(theme.paper))
    }

    private var page: some View {
        VStack(spacing: 24) {
            Text(words)
                .font(.reading(20))
                .lineSpacing(6)
                .multilineTextAlignment(.center)
                .foregroundStyle(Color(theme.ink))
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 8)
            Button {
                guard !done else { return }
                said += 1
                if done { finished() }
            } label: {
                VStack(spacing: 8) {
                    if done {
                        Image(systemName: "checkmark")
                            .font(.system(size: 40, weight: .light))
                            .foregroundStyle(Color(theme.accent))
                        Text("Finished")
                            .font(.headline)
                            .foregroundStyle(Color(theme.ink))
                    } else {
                        Text("\(said)")
                            .font(Font(Fonts.literata(72, weight: 300)))
                            .monospacedDigit()
                            // Each count simply replaces the last.
                            .contentTransition(.identity)
                            .foregroundStyle(Color(theme.ink))
                        Text("of \(count)")
                            .font(.subheadline)
                            .foregroundStyle(Color(theme.ink2))
                    }
                }
                .frame(maxWidth: .infinity, minHeight: 160, maxHeight: .infinity)
                .background(Color(theme.fill), in: RoundedRectangle(cornerRadius: 28, style: .continuous))
                .contentShape(RoundedRectangle(cornerRadius: 28, style: .continuous))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(done ? "Finished, \(count) of \(count)" : "\(said) of \(count). Tap to count one")
            .accessibilityIdentifier("count")
            .sensoryFeedback(trigger: said) { old, new in
                new == count ? .success : new > old ? .impact(weight: .light, intensity: 0.6) : nil
            }
            Button("Start again") { said = 0 }
                .font(.subheadline)
                .opacity(said == 0 ? 0 : 1)
                .disabled(said == 0)
        }
        .padding(20)
    }
}
