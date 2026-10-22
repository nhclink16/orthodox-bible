import SwiftUI

/// Where Today leads.
enum TodayRoute: Hashable {
    case month
    /// The part of the rule under way (`Store.session`), an item at a time.
    case session
    /// Readings opened from the Readings, the Psalter or a plan (`Store.walk`).
    case walk
    case plans
    case plan(String)
    case planDay(String, UInt32)
}

/// Today in the Church (Orthodox fork): the day's feasts, readings and fast, the prayer rule to
/// keep, the reading plans under way and the Psalter's kathismata, on the New or Old calendar.
/// Days step back and forward, or are picked from the month. The part of the rule due now sits
/// at the top, and Begin prays through it here, an item at a time; readings open here too.
struct TodayView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var phase
    let theme: Theme
    /// Read again at midnight, at each part of the day's time and on coming back to the front,
    /// so "today" and the Now card move on.
    @State private var now = Date()
    @State private var editing = false
    /// Every one of a long day's readings is shown.
    @State private var allReadings = false

    var body: some View {
        @Bindable var store = store
        let offset = store.todayOffset
        let date = Calendar.current.date(byAdding: .day, value: offset, to: now) ?? now
        let civil = CivilDate(date)
        let day = churchDay(year: civil.year, month: civil.month, day: civil.day, oldCalendar: store.settings.oldCalendar)
        let rule = store.ruleDay(date, now: offset == 0 ? Date() : nil)
        NavigationStack(path: $store.todayPath) {
            List {
                Section {
                    HStack {
                        Button { store.todayOffset -= 1 } label: { Image(systemName: "chevron.left") }
                            .accessibilityLabel("Previous day")
                        Spacer()
                        VStack(spacing: 2) {
                            Text(date.formatted(.dateTime.weekday(.wide).day().month(.wide)))
                                .font(.headline)
                            Text(paschaLine(day.daysToPascha)).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button { store.todayOffset += 1 } label: { Image(systemName: "chevron.right") }
                            .accessibilityLabel("Next day")
                    }
                    .buttonStyle(.borderless)
                    if offset != 0 {
                        Button("Back to Today") { store.todayOffset = 0 }
                    }
                }

                if let due = rule.now, let block = rule.blocks.first(where: { $0.id == due.block }),
                    store.startItem(block.id, on: date) != nil
                {
                    nowCard(block, date: date)
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

                readings(civil, date: date)

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
                            row(item, block: block, day: rule.day, date: date)
                        }
                    } header: {
                        header(block, date: date)
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
                    kathismata("Matins", day.matins, rule: rule, date: date)
                    kathismata("Vespers", day.vespers, rule: rule, date: date)
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
            // Titled Today on every day, so a step's ‹ always reads Today; another day shows
            // Church Calendar.
            .navigationTitle("Today")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if offset != 0 {
                    ToolbarItem(placement: .principal) {
                        Text("Church Calendar").font(.headline).accessibilityAddTraits(.isHeader)
                    }
                }
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
                            store.todayOffset = Int(chosen - CivilDate(now).number)
                        }
                    case .session: SessionView(theme: theme)
                    case .walk: WalkView(theme: theme)
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
        .onChange(of: store.todayOffset) { allReadings = false }
        // A reminder was tapped.
        .onChange(of: store.todayRequest) { now = Date() }
        // The Now card moves on at the next part of the day's time.
        .task(id: now) {
            let next = store.bible.rule().blocks.compactMap { RuleTime($0.time)?.date }.filter { $0 > now }.min()
            let midnight = Calendar.current.startOfDay(for: now.addingTimeInterval(86_400))
            try? await Task.sleep(for: .seconds((next ?? midnight).timeIntervalSince(now) + 1))
            guard !Task.isCancelled else { return }
            now = Date()
        }
    }

    /// The part of the rule due now, or next today: what's in it, ticked as it's done, and Begin.
    private func nowCard(_ block: RuleDayBlock, date: Date) -> some View {
        let begun = store.begun(block, on: date)
        let items = block.items.filter { $0.open != .notToday }
        let names = items.enumerated().reduce(Text("")) { line, entry in
            let (i, item) = entry
            let name = item.done
                ? Text("\(Image(systemName: "checkmark")) \(item.title)").foregroundStyle(Color(theme.ink3))
                : Text(item.title).foregroundStyle(Color(theme.ink2))
            return i == 0 ? name : line + Text(" · ").foregroundStyle(Color(theme.ink3)) + name
        }
        let next = begun ? store.startItem(block.id, on: date).flatMap { id in items.first { $0.id == id }?.title } : nil
        return Section {
            VStack(alignment: .leading, spacing: 8) {
                Text(title(block))
                    .font(.headline)
                    .foregroundStyle(Color(theme.ink))
                names.font(.subheadline)
                if let next {
                    Text("Next: \(next)").font(.subheadline).foregroundStyle(Color(theme.ink))
                }
                Button {
                    store.begin(block.id, on: date)
                } label: {
                    Text(begun ? "Continue" : "Begin")
                        .font(.headline)
                        .foregroundStyle(Color(theme.onAccent))
                        .frame(maxWidth: .infinity, minHeight: 36)
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
                .padding(.top, 6)
                .accessibilityLabel("\(begun ? "Continue" : "Begin") \(block.title.lowercased())")
                .accessibilityIdentifier("now-begin")
            }
            .padding(.vertical, 6)
        }
    }

    /// "Evening · 8:30 PM", with Begin (Continue once under way) for a part of the day with more
    /// than one item and something left to do in it, on today or a day gone.
    private func header(_ block: RuleDayBlock, date: Date) -> some View {
        HStack {
            Text(title(block))
            Spacer()
            if store.todayOffset <= 0, block.items.count > 1, store.startItem(block.id, on: date) != nil {
                let begun = store.begun(block, on: date)
                Button(begun ? "Continue" : "Begin") { store.begin(block.id, on: date) }
                    .font(.subheadline.weight(.semibold))
                    .textCase(nil)
                    .buttonStyle(.borderless)
                    .accessibilityLabel("\(begun ? "Continue" : "Begin") \(block.title.lowercased())")
                    .accessibilityIdentifier("block-begin-\(block.id)")
            }
        }
    }

    private func title(_ block: RuleDayBlock) -> String {
        [block.title, RuleTime(block.time)?.label].compactMap { $0 }.joined(separator: " · ")
    }

    /// An item of the rule: its check box (none on a day with nothing to do), and its step in
    /// Today; an item prayed from his own book just ticks, as its check box does.
    private func row(_ item: RuleDayItem, block: RuleDayBlock, day: Int64, date: Date) -> some View {
        let toggle = { store.setDone(item, day: day, !item.done) }
        let idle = item.open == .notToday
        let paused = store.paused(in: block.id, on: date) == item.id
        return HStack(spacing: 12) {
            CheckButton(done: item.done, theme: theme, action: toggle)
                .disabled(store.todayOffset > 0 || idle)
                .opacity(idle ? 0 : 1)
                .accessibilityHidden(idle)
                .accessibilityLabel(item.title)
                .accessibilityIdentifier("rule-\(item.id)")
            Group {
                switch item.open {
                case .nothing:
                    Button(action: toggle) { RuleItemLabel(item: item, theme: theme, paused: paused) }
                        .buttonStyle(.plain)
                        .disabled(store.todayOffset > 0)
                case .notToday:
                    RuleItemLabel(item: item, theme: theme)
                default:
                    Button {
                        store.open(item.id, block: block.id, on: date)
                    } label: {
                        RuleItemLabel(item: item, theme: theme, reads: !item.parts.isEmpty, opens: true, paused: paused)
                    }
                    .buttonStyle(.plain)
                }
            }
            .accessibilityIdentifier("open-\(item.id)")
        }
    }

    /// The day's readings, each opening here to read on through the rest: the Liturgy's Epistle
    /// and Gospel first, then the other services' smaller, saying once where a run of them is
    /// read and what for. A long day (Holy Friday's twenty-nine) shows its first few until asked
    /// for the rest.
    @ViewBuilder
    private func readings(_ civil: CivilDate, date: Date) -> some View {
        let all = dayReadings(
            year: civil.year, month: civil.month, day: civil.day, oldCalendar: store.settings.oldCalendar,
            usage: store.settings.usage)
        let main = all.filter(\.isMain)
        let rest = all.filter { !$0.isMain }
        let shown = allReadings || rest.count <= 4 ? rest.count : 3
        let steps = (main + rest).map(WalkStep.init)
        if !all.isEmpty {
            Section {
                ForEach(Array(main.enumerated()), id: \.offset) { i, r in
                    readingButton(r, after: i > 0 ? main[i - 1] : nil, id: i) { store.read(steps, at: i, on: date) }
                }
                ForEach(0..<shown, id: \.self) { i in
                    readingButton(rest[i], after: i > 0 ? rest[i - 1] : nil, id: main.count + i) {
                        store.read(steps, at: main.count + i, on: date)
                    }
                }
                if shown < rest.count {
                    Button("\(rest.count - shown) more readings") { allReadings = true }
                        .accessibilityIdentifier("more-readings")
                }
            } header: {
                Text("Readings")
            } footer: {
                if all.contains(where: { $0.confidence == .low }) {
                    Text("Some of the day’s readings vary from parish to parish. Check your parish calendar.")
                }
            }
        }
    }

    /// A reading, after `previous` in the same list.
    private func readingButton(_ r: Reading, after previous: Reading?, id: Int, open: @escaping () -> Void) -> some View {
        Button(action: open) {
            ReadingRow(
                reading: r, occasion: previous?.occasion == r.occasion ? nil : r.occasion,
                service: previous.map { $0.service != r.service || $0.served != r.served } ?? true, theme: theme)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("reading-\(id)")
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

    /// A service's kathismata, each read here: as the rule's step when the rule reads it today and
    /// it isn't done, so Done ticks it.
    @ViewBuilder
    private func kathismata(_ service: String, _ ks: [Kathisma], rule: RuleDay, date: Date) -> some View {
        if ks.isEmpty {
            LabeledContent(service, value: "No kathisma")
        }
        ForEach(ks, id: \.number) { k in
            Button {
                let label = "Kathisma \(k.number)"
                let items = rule.blocks.flatMap { b in b.items.map { (b, $0) } }
                let (block, item) = items.first { $0.1.parts.contains { $0.label == label } } ?? (nil, nil)
                if let block, let item, !item.done, store.todayOffset <= 0 {
                    store.open(item.id, block: block.id, on: date)
                } else {
                    let part = item?.parts.first { $0.label == label } ?? kathisma(k)
                    store.read(
                        [WalkStep(title: label, service: service, intro: part.reference, parts: [part])], at: 0, on: date)
                }
            } label: {
                LabeledContent(service) {
                    Text("Kathisma \(k.number) · \(k.psalms)")
                }
            }
            .foregroundStyle(Color(theme.ink))
        }
    }

    /// A kathisma's psalms, as The Psalter in 20 Days reads it on the kathisma's day: whole
    /// psalms (Psalm 151 left out).
    private func kathisma(_ k: Kathisma) -> RulePart {
        let days = store.planDays("psalter-20")
        let chapters = days.indices.contains(Int(k.number) - 1) ? days[Int(k.number) - 1].passages.map(\.chapterIndex) : []
        let passages = chapters.filter { store.book(of: Int($0)).index == 25 }.map { c in
            ReadingPassage(
                from: store.ref(chapter: Int(c), verse: 1), to: store.ref(chapter: Int(c), verse: 255), chapterIndex: c)
        }
        return RulePart(
            label: "Kathisma \(k.number)", reference: k.psalms, passages: passages, end: "End of kathisma \(k.number)")
    }

    private func paschaLine(_ days: Int32) -> String {
        switch days {
        case 0: "Christ is risen!"
        case 1: "Pascha is tomorrow"
        default: "Pascha in \(days) days"
        }
    }
}

/// A reading: the Liturgy's large, with its kind over it; the other services' smaller, with
/// where and when they're read under them.
private struct ReadingRow: View {
    let reading: Reading
    /// What it's read for, if not the day itself.
    let occasion: String?
    /// Names its service, and when that's held.
    let service: Bool
    let theme: Theme

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                if reading.isMain {
                    // Only a Liturgy other than the usual morning one is named (the Presanctified,
                    // the Vesperal of St Basil); an occasion says what the title would.
                    let liturgy = service && reading.service != "Liturgy" ? reading.service : nil
                    Text([reading.kind.label, liturgy, occasion == nil ? reading.title : nil].compactMap { $0 }.joined(separator: " · "))
                        .font(.subheadline)
                        .foregroundStyle(Color(theme.ink2))
                    Text(reading.label.unbroken)
                        .font(.reading(19))
                        .foregroundStyle(Color(theme.ink))
                        .accessibilityLabel(reading.label)
                } else {
                    Text(reading.label.unbroken)
                        .font(.subheadline)
                        .foregroundStyle(Color(theme.ink))
                        .accessibilityLabel(reading.label)
                    let line = service
                        ? [reading.service, reading.title, reading.kind.label, reading.served]
                        : [reading.title, reading.kind.label]
                    Text(line.compactMap { $0 }.joined(separator: " · "))
                        .font(.footnote)
                        .foregroundStyle(Color(theme.ink2))
                }
                if let occasion {
                    Text(occasion)
                        .font(.footnote)
                        .foregroundStyle(Color(theme.ink2))
                }
            }
            Spacer(minLength: 0)
            Image(systemName: "book")
                .font(.footnote)
                .foregroundStyle(Color(theme.ink3))
                .accessibilityHidden(true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .padding(.vertical, reading.isMain ? 4 : 1)
    }
}

extension Reading {
    /// The Liturgy's Epistle or Gospel, shown first.
    var isMain: Bool { atLiturgy && kind != .oldTestament }
}

extension WalkStep {
    /// One of the day's readings, to read in Today.
    init(_ r: Reading) {
        let end = r.kind == .oldTestament ? "End of the reading" : "End of the \(r.kind.label)"
        self.init(
            title: r.kind.label, service: [r.service, r.title, r.served].compactMap { $0 }.joined(separator: " · "),
            intro: [r.label, r.occasion].compactMap { $0 }.joined(separator: " · "),
            parts: [RulePart(label: r.kind.label, reference: r.label, passages: r.passages, end: end)])
    }
}

extension String {
    /// A reference that breaks onto a new line only between its parts ("; ", ", "), never
    /// inside one: no "2 Timothy" over "2:1–10" at the largest text sizes.
    var unbroken: String {
        var out = ""
        var previous: Character = ";"
        for c in self {
            out.append(c == " " && previous != ";" && previous != "," ? "\u{00A0}" : c)
            // A word joiner: no break after the dash.
            if c == "–" { out.append("\u{2060}") }
            previous = c
        }
        return out
    }
}

extension ReadingKind {
    var label: String {
        switch self {
        case .oldTestament: "Old Testament"
        case .epistle: "Epistle"
        case .gospel: "Gospel"
        }
    }
}
