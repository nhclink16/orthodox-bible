import SwiftUI

/// Reading plans: those under way, and the catalogue to start one from.
struct PlansView: View {
    @Environment(Store.self) private var store
    let theme: Theme

    var body: some View {
        let active = store.activePlans()
        let started = Set(active.map(\.plan.id))
        List {
            if !active.isEmpty {
                Section("Under way") {
                    ForEach(active, id: \.plan.id) { a in
                        NavigationLink(value: TodayRoute.plan(a.plan.id)) {
                            PlanRow(title: a.plan.title, line: progress(a), theme: theme)
                        }
                    }
                }
            }
            Section(active.isEmpty ? "Choose a plan" : "More plans") {
                ForEach(readingPlans().filter { !started.contains($0.id) }, id: \.id) { plan in
                    NavigationLink(value: TodayRoute.plan(plan.id)) {
                        PlanRow(title: plan.title, line: plan.meta, theme: theme)
                    }
                }
            }
        }
        .navigationTitle("Reading plans")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func progress(_ a: ActivePlan) -> String {
        if a.day <= Int64(a.plan.days) { return "Day \(a.day) of \(a.plan.days)" }
        return a.next.map { "Next: day \($0)" } ?? "Every day read"
    }
}

private struct PlanRow: View {
    let title: String
    let line: String
    let theme: Theme

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).foregroundStyle(Color(theme.ink))
            Text(line).font(.subheadline).foregroundStyle(Color(theme.ink2))
        }
        .padding(.vertical, 2)
    }
}

/// One plan: what it is, today's reading and every day, or an offer to start it.
struct PlanView: View {
    @Environment(Store.self) private var store
    let id: String
    let theme: Theme
    @State private var stopping = false

    var body: some View {
        let info = readingPlans().first { $0.id == id }
        let active = store.activePlans().first { $0.plan.id == id }
        List {
            if let info {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(info.title).font(.reading(22)).foregroundStyle(Color(theme.ink))
                        Text(info.description).font(.subheadline).foregroundStyle(Color(theme.ink2))
                        Text(info.meta).font(.footnote).foregroundStyle(Color(theme.ink3))
                    }
                    .padding(.vertical, 4)
                    if active == nil {
                        Button("Start today") { store.startPlan(id) }
                            .fontWeight(.semibold)
                            .accessibilityIdentifier("start-plan")
                    }
                }
            }
            if let active, active.day >= 1, active.day <= Int64(active.plan.days),
                let today = store.planDay(id, UInt32(active.day))
            {
                Section("Today · Day \(today.day)") {
                    ForEach(Array(today.passages.enumerated()), id: \.offset) { i, passage in
                        PassageRow(plan: id, day: today.day, index: i, passage: passage, theme: theme)
                    }
                }
            }
            Section(active == nil ? "What you'll read" : "Every day") {
                ForEach(store.planDays(id), id: \.day) { day in
                    NavigationLink(value: TodayRoute.planDay(id, day.day)) {
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text("Day \(day.day)")
                                .font(.subheadline.monospacedDigit())
                                .foregroundStyle(Color(theme.ink2))
                                .frame(minWidth: 64, alignment: .leading)
                            Text(day.label).foregroundStyle(Color(theme.ink))
                            Spacer(minLength: 0)
                            if day.read {
                                Image(systemName: "checkmark")
                                    .font(.footnote.weight(.semibold))
                                    .foregroundStyle(Color(theme.accent))
                                    .accessibilityLabel("Read")
                            }
                        }
                    }
                }
            }
            if active != nil {
                Section {
                    Button("Stop this plan", role: .destructive) { stopping = true }
                        .foregroundStyle(Color(theme.danger))
                }
            }
        }
        .navigationTitle(info?.title ?? "Plan")
        .navigationBarTitleDisplayMode(.inline)
        .confirmationDialog("Stop this plan?", isPresented: $stopping, titleVisibility: .visible) {
            Button("Stop plan", role: .destructive) { store.stopPlan(id) }
        } message: {
            Text("What you've checked off in it is forgotten.")
        }
    }
}

/// A day of a plan: its passages to check off and open.
struct PlanDayView: View {
    @Environment(Store.self) private var store
    let plan: String
    let day: UInt32
    let theme: Theme

    var body: some View {
        List {
            if let d = store.planDay(plan, day) {
                Section(d.label) {
                    ForEach(Array(d.passages.enumerated()), id: \.offset) { i, passage in
                        PassageRow(plan: plan, day: day, index: i, passage: passage, theme: theme)
                    }
                }
            }
        }
        .navigationTitle("Day \(day)")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// A passage of a plan's day: a check box, and the passage to open in the reader.
struct PassageRow: View {
    @Environment(Store.self) private var store
    let plan: String
    let day: UInt32
    let index: Int
    let passage: PlanPassage
    let theme: Theme

    var body: some View {
        HStack(spacing: 12) {
            CheckButton(done: passage.read, theme: theme) {
                store.setRead(plan, day: day, passage: index, !passage.read)
            }
            .accessibilityLabel(passage.label)
            .accessibilityIdentifier("passage-\(plan)-\(day)-\(index)")
            Button {
                store.read(passage, plan: plan, day: day, index: index)
            } label: {
                HStack {
                    Text(passage.label)
                        .foregroundStyle(passage.read ? Color(theme.ink2) : Color(theme.ink))
                    Spacer(minLength: 0)
                    Image(systemName: "book")
                        .font(.footnote)
                        .foregroundStyle(Color(theme.ink3))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint("Opens in the reader and checks it off")
            .accessibilityIdentifier("read-\(plan)-\(day)-\(index)")
        }
    }
}
