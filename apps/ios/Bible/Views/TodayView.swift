import SwiftUI

/// Where Today leads.
enum TodayRoute: Hashable {
    case month
    /// A prayer to read or the Jesus Prayer to count, on a day.
    case item(RuleDayItem, Date)
    case plans
    case plan(String)
    case planDay(String, UInt32)
}

/// Today in the Church (Orthodox fork): the day's feasts and fast, the prayer rule to keep, the
/// reading plans under way and the Psalter's kathismata, on the New or Old calendar. Days step
/// back and forward, or are picked from the month.
struct TodayView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var phase
    let theme: Theme
    /// Read again at midnight and on coming back to the front, so "today" moves on.
    @State private var now = Date()
    @State private var offset = 0
    @State private var editing = false
    @State private var starting: PlanInfo?

    var body: some View {
        @Bindable var store = store
        let date = Calendar.current.date(byAdding: .day, value: offset, to: now) ?? now
        let civil = CivilDate(date)
        let day = churchDay(year: civil.year, month: civil.month, day: civil.day, oldCalendar: store.settings.oldCalendar)
        let rule = store.ruleDay(date)
        NavigationStack {
            List {
                Section {
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

                ForEach(rule.blocks, id: \.id) { block in
                    Section {
                        ForEach(block.items, id: \.id) { item in
                            row(item, day: rule.day, date: date)
                        }
                    } header: {
                        Text([block.title, RuleTime(block.time)?.label].compactMap { $0 }.joined(separator: " · "))
                    }
                }
                Section {
                    if rule.blocks.isEmpty {
                        Text("Nothing in your rule for this day.").foregroundStyle(.secondary)
                    }
                    Button { editing = true } label: { Label("Edit rule", systemImage: "square.and.pencil") }
                        .accessibilityIdentifier("edit-rule")
                } footer: {
                    if offset > 0 { Text("Items are checked off on the day itself.") }
                }

                plans(date)

                Section {
                    kathismata("Matins", day.matins)
                    kathismata("Vespers", day.vespers)
                } header: {
                    Text("Psalter")
                } footer: {
                    Text("The weekly order for ordinary weeks. In winter a third kathisma is read at Matins, and in Great Lent the Psalter is read twice a week.")
                }

                Section("Calendar") {
                    Picker("Calendar", selection: $store.settings.oldCalendar) {
                        Text("New calendar").tag(false)
                        Text("Old calendar").tag(true)
                    }
                    .pickerStyle(.segmented)
                }
            }
            .navigationTitle(offset == 0 ? "Today" : "Church Calendar")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    NavigationLink(value: TodayRoute.month) { Image(systemName: "calendar") }
                        .accessibilityLabel("Month")
                        .accessibilityIdentifier("month")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .navigationDestination(for: TodayRoute.self) { route in
                Group {
                    switch route {
                    case .month:
                        MonthView(theme: theme, shown: date, today: now) { chosen in
                            offset = Int(chosen - CivilDate(now).number)
                        }
                    case .item(let item, let date): RuleItemView(item: item, date: date, theme: theme)
                    case .plans: PlansView(theme: theme)
                    case .plan(let id): PlanView(id: id, theme: theme)
                    case .planDay(let id, let day): PlanDayView(plan: id, day: day, theme: theme)
                    }
                }
                // Pushed pages keep the sheet's paper in sepia.
                .sheetChrome(theme)
            }
        }
        .sheet(isPresented: $editing) {
            RuleEditor(theme: theme)
                .toasts(theme)
                .environment(store)
                .sheetChrome(theme)
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.significantTimeChangeNotification)) { _ in
            now = Date()
        }
        .onChange(of: phase) { if phase == .active { now = Date() } }
        // A reminder was tapped.
        .onChange(of: store.todayRequest) {
            now = Date()
            offset = 0
        }
        .confirmationDialog(
            "Start “\(starting?.title ?? "")”?", isPresented: Binding(get: { starting != nil }, set: { if !$0 { starting = nil } }),
            titleVisibility: .visible, presenting: starting
        ) { plan in
            Button("Start today") { store.startPlan(plan.id) }
        } message: { plan in
            Text(plan.meta)
        }
    }

    /// An item of the rule: its check box (none on a day with nothing to do), and what it opens.
    private func row(_ item: RuleDayItem, day: Int64, date: Date) -> some View {
        let toggle = { store.setDone(item, day: day, !item.done) }
        let idle = item.open == .notToday
        return HStack(spacing: 12) {
            CheckButton(done: item.done, theme: theme, action: toggle)
                .disabled(offset > 0 || idle)
                .opacity(idle ? 0 : 1)
                .accessibilityHidden(idle)
                .accessibilityLabel(item.title)
                .accessibilityIdentifier("rule-\(item.id)")
            Group {
                switch item.open {
                case .text, .jesusPrayer:
                    NavigationLink(value: TodayRoute.item(item, date)) { RuleItemLabel(item: item, theme: theme) }
                case .chapter(let chapter):
                    Button { store.read(Int(chapter)) } label: { RuleItemLabel(item: item, theme: theme, reads: true) }
                        .buttonStyle(.plain)
                case .plan(let plan, let day):
                    NavigationLink(value: TodayRoute.planDay(plan, day)) { RuleItemLabel(item: item, theme: theme) }
                case .startPlan(let plan, _):
                    Button { starting = reading(plan) } label: { RuleItemLabel(item: item, theme: theme) }
                        .buttonStyle(.plain)
                case .nothing, .comingSoon:
                    Button(action: toggle) { RuleItemLabel(item: item, theme: theme) }
                        .buttonStyle(.plain)
                        .disabled(offset > 0)
                case .notToday:
                    RuleItemLabel(item: item, theme: theme)
                }
            }
            .accessibilityIdentifier("open-\(item.id)")
        }
    }

    private func reading(_ plan: String) -> PlanInfo? {
        readingPlans().first { $0.id == plan }
    }

    /// The plans under way: the day's passages, to check off and open.
    @ViewBuilder
    private func plans(_ date: Date) -> some View {
        let active = store.activePlans(date).filter { $0.day >= 1 }
        ForEach(active, id: \.plan.id) { a in
            let inPlan = a.day <= Int64(a.plan.days)
            Section {
                if inPlan, let day = store.planDay(a.plan.id, UInt32(a.day)) {
                    ForEach(Array(day.passages.enumerated()), id: \.offset) { i, passage in
                        PassageRow(plan: a.plan.id, day: day.day, index: i, passage: passage, theme: theme)
                    }
                } else if let next = a.next {
                    NavigationLink("Continue with day \(next)", value: TodayRoute.planDay(a.plan.id, next))
                } else {
                    Text("Every day read").foregroundStyle(.secondary)
                }
            } header: {
                Text(inPlan ? "\(a.plan.title) · Day \(a.day) of \(a.plan.days)" : a.plan.title)
            }
        }
        Section {
            NavigationLink(value: TodayRoute.plans) {
                Label(active.isEmpty ? "Choose a reading plan" : "All reading plans", systemImage: "book")
            }
            .accessibilityIdentifier("plans")
        } header: {
            if active.isEmpty { Text("Reading plans") }
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
