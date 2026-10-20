import SwiftUI

/// A month of the church year: each day's fast as a quiet band, great feasts in red, and the
/// rule as a dot, filled on a day it was kept and hollow on one partly kept (nothing ahead of
/// today). Tapping a day shows it on Today.
struct MonthView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let theme: Theme
    /// The day Today shows.
    let shown: Date
    /// Called with the day tapped (days since 1970-01-01).
    let choose: (Int64) -> Void
    @State private var month: Date?

    private let calendar = Calendar.current

    var body: some View {
        let first = month ?? calendar.date(from: calendar.dateComponents([.year, .month], from: shown)) ?? shown
        let civil = CivilDate(first)
        let days = store.month(year: civil.year, month: civil.month)
        let today = CivilDate(Date()).number
        let selected = CivilDate(shown).number
        let weekStart = calendar.firstWeekday - 1
        let lead = (Int(days.first?.weekday ?? 0) - weekStart + 7) % 7
        let symbols = calendar.shortStandaloneWeekdaySymbols
        List {
            Section {
                VStack(spacing: 16) {
                    HStack {
                        Button { step(-1, from: first) } label: { Image(systemName: "chevron.left") }
                            .accessibilityLabel("Previous month")
                        Spacer()
                        Text(first.formatted(.dateTime.month(.wide).year()))
                            .font(.headline)
                        Spacer()
                        Button { step(1, from: first) } label: { Image(systemName: "chevron.right") }
                            .accessibilityLabel("Next month")
                    }
                    .buttonStyle(.borderless)
                    .padding(.top, 4)
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 2), count: 7), spacing: 4) {
                        ForEach(0..<7, id: \.self) { i in
                            Text(symbols[(i + weekStart) % 7])
                                .font(.caption2.weight(.medium))
                                .foregroundStyle(Color(theme.ink3))
                                .padding(.bottom, 4)
                                .accessibilityHidden(true)
                        }
                        // Blank cells before the 1st, with ids of their own.
                        ForEach((0..<lead).map { -1 - $0 }, id: \.self) { _ in
                            Color.clear.frame(height: 1)
                        }
                        ForEach(days, id: \.day) { d in
                            Button {
                                choose(d.day)
                                dismiss()
                            } label: {
                                DayCell(
                                    day: d, today: d.day == today, selected: d.day == selected, ahead: d.day > today,
                                    theme: theme)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(label(d, first: first, ahead: d.day > today))
                            .accessibilityAddTraits(d.day == selected ? .isSelected : [])
                            .accessibilityIdentifier("day-\(d.date)")
                        }
                    }
                }
                .padding(.vertical, 4)
            }

            Section {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 130), alignment: .leading)], alignment: .leading, spacing: 10) {
                    ForEach(FastLevel.shown, id: \.self) { level in
                        HStack(spacing: 8) {
                            FastBand(level: level, theme: theme).frame(width: 18)
                            Text(level.short).font(.caption)
                        }
                    }
                    HStack(spacing: 8) {
                        Text("6").font(.caption.weight(.semibold)).foregroundStyle(Color(theme.rubric)).frame(width: 18)
                        Text("Great feast").font(.caption)
                    }
                    HStack(spacing: 8) {
                        KeptDot(kept: .fully, theme: theme).frame(width: 18)
                        Text("Rule kept").font(.caption)
                    }
                    HStack(spacing: 8) {
                        KeptDot(kept: .partly, theme: theme).frame(width: 18)
                        Text("Some of the rule").font(.caption)
                    }
                }
                .foregroundStyle(Color(theme.ink2))
                .padding(.vertical, 4)
            } header: {
                Text("Key")
            } footer: {
                Text("Tap a day to see it on Today.")
            }
        }
        .navigationTitle("Calendar")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func step(_ months: Int, from first: Date) {
        month = calendar.date(byAdding: .month, value: months, to: first)
    }

    private func label(_ d: MonthDay, first: Date, ahead: Bool) -> String {
        var parts = [first.formatted(.dateTime.month(.wide)) + " \(d.date)"]
        if d.greatFeast { parts.append("Great feast") }
        if d.fast != .none { parts.append(d.fast.short) }
        if !ahead && d.kept == .fully { parts.append("Rule kept") }
        if !ahead && d.kept == .partly { parts.append("Some of the rule kept") }
        return parts.joined(separator: ", ")
    }
}

private struct DayCell: View {
    let day: MonthDay
    let today: Bool
    let selected: Bool
    let ahead: Bool
    let theme: Theme

    var body: some View {
        VStack(spacing: 5) {
            Text("\(day.date)")
                .font(.system(size: 17, weight: day.greatFeast ? .semibold : .regular).monospacedDigit())
                .foregroundStyle(day.greatFeast ? Color(theme.rubric) : Color(theme.ink))
            KeptDot(kept: ahead ? .nothing : day.kept, theme: theme)
            FastBand(level: day.fast, theme: theme)
                .frame(width: 22)
        }
        .frame(maxWidth: .infinity, minHeight: 56)
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color(theme.accentSoft))
            } else if today {
                RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color(theme.accent), lineWidth: 1)
            }
        }
        .contentShape(Rectangle())
    }
}

/// The rule on a day: a filled dot when kept, a ring when partly, nothing otherwise.
private struct KeptDot: View {
    let kept: RuleKept
    let theme: Theme

    var body: some View {
        Group {
            switch kept {
            case .fully: Circle().fill(Color(theme.accent))
            case .partly: Circle().strokeBorder(Color(theme.accent), lineWidth: 1.2)
            case .nothing: Color.clear
            }
        }
        .frame(width: 6, height: 6)
    }
}

/// A day's fast as a short band, from no meat to strict; nothing on a day without one.
private struct FastBand: View {
    let level: FastLevel
    let theme: Theme

    var body: some View {
        Capsule()
            .fill(level.color(theme))
            .frame(height: 3)
    }
}

extension FastLevel {
    /// The levels the key shows, from least to most.
    static let shown: [FastLevel] = [.noMeat, .fish, .wineOil, .wine, .strict]

    var short: String {
        switch self {
        case .none: "No fast"
        case .noMeat: "No meat"
        case .fish: "Fish allowed"
        case .wineOil: "Wine and oil"
        case .wine: "Wine, no oil"
        case .strict: "Strict fast"
        }
    }

    /// The highlight swatches the reader uses, for what each level allows: dairy, fish, wine
    /// and oil, wine; plain ink for a strict fast.
    func color(_ theme: Theme) -> Color {
        switch self {
        case .none: .clear
        case .noMeat: Color(theme.swatch(.yellow))
        case .fish: Color(theme.swatch(.blue))
        case .wineOil: Color(theme.swatch(.purple))
        case .wine: Color(theme.swatch(.pink))
        case .strict: Color(theme.ink2)
        }
    }
}
