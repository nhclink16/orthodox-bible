import SwiftUI

/// Today in the Church (Orthodox fork): the day's feasts, its fast and the Psalter's kathismata,
/// on the New or Old calendar. Days step back and forward.
struct TodayView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let theme: Theme
    @State private var offset = 0

    var body: some View {
        @Bindable var store = store
        let date = Calendar.current.date(byAdding: .day, value: offset, to: Date()) ?? Date()
        let parts = Calendar(identifier: .gregorian).dateComponents([.year, .month, .day], from: date)
        let day = churchDay(
            year: Int32(parts.year ?? 2026), month: UInt8(parts.month ?? 1), day: UInt8(parts.day ?? 1),
            oldCalendar: store.settings.oldCalendar)
        NavigationStack {
            List {
                Section {
                    Picker("Calendar", selection: $store.settings.oldCalendar) {
                        Text("New calendar").tag(false)
                        Text("Old calendar").tag(true)
                    }
                    .pickerStyle(.segmented)
                    HStack {
                        Button { offset -= 1 } label: { Image(systemName: "chevron.left") }
                            .accessibilityLabel("Previous day")
                        Spacer()
                        VStack(spacing: 2) {
                            Text(date.formatted(.dateTime.weekday(.wide).day().month(.wide)))
                                .font(.headline)
                            Text(paschaLine(day.daysToPascha)).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button { offset += 1 } label: { Image(systemName: "chevron.right") }
                            .accessibilityLabel("Next day")
                    }
                    .buttonStyle(.borderless)
                    if offset != 0 {
                        Button("Back to Today") { offset = 0 }
                    }
                }

                Section("The day") {
                    if day.feasts.isEmpty {
                        Text(day.season ?? "No great feast").foregroundStyle(.secondary)
                    } else {
                        ForEach(day.feasts, id: \.self) { Text($0).font(Font(Fonts.literata(19))) }
                        if let season = day.season {
                            Text(season).font(.subheadline).foregroundStyle(.secondary)
                        }
                    }
                }

                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(day.fast).font(.headline)
                            .foregroundStyle(day.fast == "No fast" ? Color(theme.ink) : Color(theme.accent))
                        Text(day.fastDetail).font(.subheadline).foregroundStyle(.secondary)
                    }
                } header: {
                    Text("Fasting")
                } footer: {
                    Text("A general guide to the fasting rules most parishes publish. Practice varies; follow your parish calendar and your priest's guidance.")
                }

                Section {
                    kathismata("Matins", day.matins)
                    kathismata("Vespers", day.vespers)
                } header: {
                    Text("Psalter")
                } footer: {
                    Text("The weekly order for ordinary weeks. In winter a third kathisma is read at Matins, and in Great Lent the Psalter is read twice a week.")
                }
            }
            .navigationTitle(offset == 0 ? "Today" : "Church Calendar")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    @ViewBuilder
    private func kathismata(_ service: String, _ ks: [Kathisma]) -> some View {
        if ks.isEmpty {
            LabeledContent(service, value: "No kathisma")
        }
        ForEach(ks, id: \.number) { k in
            Button {
                store.go(to: Int(k.chapterIndex))
                dismiss()
            } label: {
                LabeledContent(service) {
                    Text("Kathisma \(k.number) · \(k.psalms)")
                }
            }
            .foregroundStyle(Color(theme.ink))
        }
    }

    private func paschaLine(_ days: Int32) -> String {
        switch days {
        case 0: "Christ is risen!"
        case 1: "Pascha is tomorrow"
        default: "Pascha in \(days) days"
        }
    }
}
