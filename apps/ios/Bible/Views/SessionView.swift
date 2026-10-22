import SwiftUI

/// A part of the rule prayed through in Today (Orthodox fork): its items one at a time, each with
/// Done to tick it and open the next, then a short end page. The title menu goes back to an
/// item or finishes one early; ‹ Today or closing Today pauses it, and Continue opens it again
/// where it was. Nothing is counted.
struct SessionView: View {
    @Environment(Store.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let theme: Theme
    /// The psalm or chapter in view of a long reading, for the subtitle ("Psalm 87"), by the
    /// step it's in: a step crossfading away may still report.
    @State private var inView: (step: String, label: String?)?
    /// Bumped by Done, for its haptic.
    @State private var dones = 0

    var body: some View {
        let session = store.session
        let block = session.flatMap { store.block($0.block, on: $0.date) }
        let item = block?.items.first { $0.id == session?.item }
        // A plan's item shows the day's next passage in place of the one read.
        let key = "\(item?.id ?? "end")|\(item?.parts.first?.reference ?? "")"
        ZStack {
            if let session, let block {
                Group {
                    if let item {
                        step(item, block: block, date: session.date)
                    } else {
                        EndPage(
                            title: block.title, next: store.nextInRule(after: block.id, on: session.date),
                            theme: theme, back: { store.finish() })
                    }
                }
                .id(key)
                .transition(.opacity)
            } else {
                Color(theme.paper)
            }
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.25), value: key)
        .sensoryFeedback(.success, trigger: dones)
        .stepTitle(
            item?.title ?? block?.title ?? "", subtitle: subtitle(session, block: block, item: item),
            menu: item != nil
        ) {
            if let session, let block, let item { menu(block, current: item, date: session.date) }
        }
        .toolbar {
            if let session, let item, reads(item) {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { openInReader(item, date: session.date) } label: { Image(systemName: "book") }
                        .accessibilityLabel("Open in the reader")
                        .accessibilityIdentifier("session-reader")
                }
            }
        }
        // ‹ Today from the end page: the part of the day is over.
        .onDisappear { if store.session?.item == nil { store.finish() } }
    }

    /// "Evening · Psalm 87"; a day other than today is named first.
    private func subtitle(_ session: RuleSession?, block: RuleDayBlock?, item: RuleDayItem?) -> String? {
        guard let session, let block, let item else { return nil }
        let day = Store.sameDay(session.date, Date())
            ? nil : session.date.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
        let shown = inView?.step == item.id ? inView?.label : nil
        return [day, block.title, shown].compactMap { $0 }.joined(separator: " · ")
    }

    @ViewBuilder
    private func step(_ item: RuleDayItem, block: RuleDayBlock, date: Date) -> some View {
        let key = Store.placeKey(item.id, on: date)
        let footer = footer(item, block: block, date: date)
        let note = [item.optional ? "Optional" : nil, item.note.isEmpty ? nil : item.note].compactMap { $0 }
            .joined(separator: " · ")
        switch item.open {
        case .text(let title, let text, let source):
            PrayerText(title: title, note: note.isEmpty ? nil : note, text: text, source: source, theme: theme) {
                footer
            }
        case .jesusPrayer(let count):
            JesusPrayerCounter(
                count: Int(count), said: Binding(get: { store.counts[key] ?? 0 }, set: { store.counts[key] = $0 }),
                theme: theme,
                finished: {
                    if !item.done && !Store.ahead(date) { store.setDone(item, day: CivilDate(date).number, true) }
                }
            ) { footer }
        case .chapter, .passage, .plan:
            PassageView(
                parts: item.parts, intro: item.detail ?? (note.isEmpty ? nil : note), theme: theme,
                place: store.places[key],
                reached: { place, content in
                    store.places[key] = place
                    let shown = Self.inView(place, content, parts: item.parts)
                    if inView?.step != item.id || inView?.label != shown { inView = (item.id, shown) }
                }
            ) { footer }
        case .fathers:
            // "On the day's Gospel · Luke 9:44–50"; on a day without one, only the line that says so.
            let gospel = item.detail?.hasPrefix("No Gospel") != true
            FathersStep(
                parts: item.parts,
                intro: [gospel ? item.note : "", item.detail ?? ""].filter { !$0.isEmpty }.joined(separator: " · "),
                key: key, theme: theme
            ) { footer }
        case .startPlan(let plan, let title):
            StartPlanStep(
                title: title, meta: readingPlans().first { $0.id == plan }?.meta, theme: theme,
                start: { store.startPlan(plan) }, skip: { store.skip() })
        case .nothing, .notToday:
            BookStep(item: item, note: note, message: message(item, date: date), theme: theme) { footer }
        }
    }

    /// Done and Skip, or Next on an item already done or a day still to come (Back to Today after
    /// its last).
    private func footer(_ item: RuleDayItem, block: RuleDayBlock, date: Date) -> StepFooter {
        let ahead = Store.ahead(date)
        // A plan's day with another passage to read reads it next.
        var next = store.ruleNext(block.id, after: item.id, on: date).flatMap { id in
            block.items.first { $0.id == id }?.title
        }
        if case .plan(let plan, let day) = item.open, !ahead, let i = store.unreadPassage(plan, day: day) {
            let passages = store.planDay(plan, day)?.passages ?? []
            next = passages.indices.first { $0 > i && !passages[$0].read }.map { passages[$0].label } ?? next
        }
        let done = item.done && !ahead
        return StepFooter(
            theme: theme, primary: ahead && next == nil ? "Back to Today" : done || ahead ? "Next" : "Done",
            action: {
                dones += 1
                store.done()
            },
            next: next.map { "Next: \($0)" }, doneLine: done ? "Done today" : nil,
            note: ahead ? "Items are checked off on the day itself." : nil,
            skip: done || ahead ? nil : { store.skip() },
            undo: done ? ("Mark as not done", { store.setDone(item, day: CivilDate(date).number, false) }) : nil)
    }

    /// The title menu: the part of the day's items to go back to, then Done, Skip and Today, as
    /// the footer has them. What's done and what's open say so in words, which every text size
    /// shows (the menu's icons go at the largest) and VoiceOver reads; a menu item takes no traits.
    @ViewBuilder
    private func menu(_ block: RuleDayBlock, current: RuleDayItem, date: Date) -> some View {
        let ahead = Store.ahead(date)
        Section {
            ForEach(block.items, id: \.id) { i in
                Button {
                    store.open(i.id, block: block.id, on: date)
                } label: {
                    if i.open == .notToday {
                        Text(i.title)
                        Text(i.detail ?? "")
                    } else if i.done {
                        Label(i.title, systemImage: "checkmark")
                        Text("Done today")
                    } else if i.id == current.id {
                        Label(i.title, systemImage: "circle.fill")
                        Text("You’re here")
                    } else {
                        Text(i.title)
                    }
                }
                .disabled(i.open == .notToday)
            }
        }
        Section {
            if !isStart(current), !ahead || store.ruleNext(block.id, after: current.id, on: date) != nil {
                Button(current.done || ahead ? "Next" : "Done") {
                    dones += 1
                    store.done()
                }
            }
            if !current.done, !ahead { Button("Skip") { store.skip() } }
            Button("Back to Today") { store.todayPath = [] }
        }
    }

    private func isStart(_ item: RuleDayItem) -> Bool {
        if case .startPlan = item.open { return true }
        return false
    }

    private func reads(_ item: RuleDayItem) -> Bool {
        switch item.open {
        case .chapter, .passage, .plan, .fathers: !item.parts.isEmpty
        default: false
        }
    }

    /// The book button: the reader at the verse in view, or the Fathers' verse.
    private func openInReader(_ item: RuleDayItem, date: Date) {
        let key = Store.placeKey(item.id, on: date)
        if case .fathers = item.open {
            let verses = store.bible.fathersCounts(passages: item.parts.flatMap(\.passages))
            guard let v = verses.dropFirst(store.fathersAt[key] ?? 0).first ?? verses.first else { return }
            return store.openInReader(chapter: store.chapterIndex(of: v.verse), verse: Int(v.verse.verse))
        }
        let spans = item.parts.flatMap { $0.passages.flatMap(\.spans) }
        let place = store.places[key]
        guard let span = place.flatMap({ spans.indices.contains($0.span) ? spans[$0.span] : nil }) ?? spans.first
        else { return }
        store.openInReader(chapter: span.chapter, verse: place?.verse ?? span.from)
    }

    /// What a reading the subtitle names is at: the psalm or chapter of a part with several, or
    /// the part of one with several ("Gospel"); `nil` for one stretch of text.
    static func inView(_ place: PassagePlace, _ content: ChapterContent, parts: [RulePart]) -> String? {
        var first = 0
        for part in parts {
            let n = part.passages.flatMap(\.spans).count
            if place.span < first + n {
                if n > 1 {
                    return content.book == 25 ? "Psalm \(content.shownChapter)" : "\(content.bookName) \(content.shownChapter)"
                }
                return parts.count > 1 ? part.label : nil
            }
            first += n
        }
        return nil
    }

    private func message(_ item: RuleDayItem, date: Date) -> String {
        let finish = Store.ahead(date) ? "" : " When you’ve finished, tap Done."
        switch store.kind(of: item.id) {
        case .prayers?: return "Pray from your prayer book." + finish
        case .readings?, .fathers?: return "No readings are listed for this day."
        default: return finish.trimmingCharacters(in: .whitespaces)
        }
    }
}

/// Readings opened from Today's Readings, Psalter or a plan, read in Today: Next reading goes on
/// through the day's readings, and Done checks off a plan's passage.
struct WalkView: View {
    @Environment(Store.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let theme: Theme
    @State private var inView: (step: Int, label: String?)?

    var body: some View {
        let walk = store.walk
        let step = walk.flatMap { $0.steps.indices.contains($0.index) ? $0.steps[$0.index] : nil }
        ZStack {
            if let walk, let step {
                let key = Self.key(walk)
                PassageView(
                    parts: step.parts, intro: step.intro, theme: theme, place: store.places[key],
                    reached: { place, content in
                        store.places[key] = place
                        let shown = SessionView.inView(place, content, parts: step.parts)
                        if inView?.step != walk.index || inView?.label != shown { inView = (walk.index, shown) }
                    }
                ) { footer(walk, step) }
                .id(walk.index)
                .transition(.opacity)
            } else {
                Color(theme.paper)
            }
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.25), value: walk?.index)
        .stepTitle(
            step?.title ?? "",
            subtitle: [step?.service, inView?.step == walk?.index ? inView?.label : nil].compactMap { $0 }
                .joined(separator: " · "),
            menu: (walk?.steps.count ?? 0) > 1
        ) {
            if let walk { menu(walk) }
        }
        .toolbar {
            if let walk, let step {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        let spans = step.parts.flatMap { $0.passages.flatMap(\.spans) }
                        let place = store.places[Self.key(walk)]
                        guard let span = place.flatMap({ spans.indices.contains($0.span) ? spans[$0.span] : nil })
                            ?? spans.first
                        else { return }
                        store.openInReader(chapter: span.chapter, verse: place?.verse ?? span.from)
                    } label: {
                        Image(systemName: "book")
                    }
                    .accessibilityLabel("Open in the reader")
                    .accessibilityIdentifier("session-reader")
                }
            }
        }
    }

    private static func key(_ walk: ReadingWalk) -> String {
        Store.placeKey("walk-\(walk.index)-\(walk.steps[walk.index].title)", on: walk.date)
    }

    private func footer(_ walk: ReadingWalk, _ step: WalkStep) -> StepFooter {
        let ahead = Store.ahead(walk.date)
        let later = walk.steps.indices.contains(walk.index + 1) ? walk.steps[walk.index + 1] : nil
        guard let plan = step.plan else {
            if let later {
                return StepFooter(
                    theme: theme, primary: "Next reading", action: { store.walk(to: walk.index + 1) },
                    next: later.parts.first.map { "Next: \($0.reference)" })
            }
            return StepFooter(theme: theme, primary: "Back to Today", id: "end-today", action: { store.todayPath = [] })
        }
        let read = store.planDay(plan.id, plan.day)?.passages.dropFirst(plan.passage).first?.read == true
        return StepFooter(
            theme: theme, primary: read || ahead ? "Next" : "Done", action: { store.walkDone() },
            next: walk.steps.dropFirst(walk.index + 1).first { later in
                later.plan.map { store.planDay($0.id, $0.day)?.passages.dropFirst($0.passage).first?.read == false } ?? false
            }.map { "Next: \($0.title)" },
            doneLine: read ? "Checked off" : nil,
            note: ahead ? "Passages are checked off on the day itself." : nil,
            undo: read ? ("Mark as not read", { store.setRead(plan.id, day: plan.day, passage: plan.passage, false) }) : nil)
    }

    /// The readings by service, to pick from, and Today.
    @ViewBuilder
    private func menu(_ walk: ReadingWalk) -> some View {
        let groups = walk.steps.indices.reduce(into: [[Int]]()) { groups, i in
            if let last = groups.last?.last, walk.steps[last].service == walk.steps[i].service {
                groups[groups.count - 1].append(i)
            } else {
                groups.append([i])
            }
        }
        ForEach(groups, id: \.self) { group in
            Section(walk.steps[group[0]].service) {
                ForEach(group, id: \.self) { i in
                    Button {
                        store.walk(to: i)
                    } label: {
                        let reference = walk.steps[i].parts.first?.reference ?? walk.steps[i].title
                        if i == walk.index {
                            Label(reference, systemImage: "circle.fill")
                        } else {
                            Text(reference)
                        }
                        Text(walk.steps[i].title)
                    }
                }
            }
        }
        Section {
            Button("Back to Today") { store.todayPath = [] }
        }
    }
}

/// The step's main button, at the end of its text and not pinned: Done (or Next), what it opens,
/// then Skip or Mark as not done.
struct StepFooter: View {
    let theme: Theme
    /// "Done", "Next", "Next reading", "Back to Today".
    let primary: String
    var id = "mark-done"
    let action: () -> Void
    /// Under it: "Next: The Psalter".
    var next: String?
    /// Over it: "Done today".
    var doneLine: String?
    /// "Items are checked off on the day itself."
    var note: String?
    var skip: (() -> Void)?
    var undo: (String, () -> Void)?

    var body: some View {
        VStack(spacing: 10) {
            if let doneLine {
                Text("\(doneLine) \(Image(systemName: "checkmark"))")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(Color(theme.accent))
            }
            Button(action: action) {
                Text(primary)
                    .font(.headline)
                    .foregroundStyle(Color(theme.onAccent))
                    .frame(maxWidth: .infinity, minHeight: 44)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.capsule)
            .accessibilityIdentifier(id)
            if let next {
                Text(next)
                    .font(.subheadline)
                    .foregroundStyle(Color(theme.ink2))
                    .multilineTextAlignment(.center)
            }
            if let note {
                Text(note)
                    .font(.footnote)
                    .foregroundStyle(Color(theme.ink2))
                    .multilineTextAlignment(.center)
            }
            if let undo {
                Button(undo.0, action: undo.1)
                    .frame(minHeight: 44)
            }
            if let skip {
                Button("Skip", action: skip)
                    .frame(minHeight: 44)
                    .accessibilityIdentifier("session-skip")
            }
        }
        .frame(maxWidth: .infinity)
    }
}

/// An item prayed from his own book, or done away from the phone: its title and note, and Done.
private struct BookStep<Footer: View>: View {
    let item: RuleDayItem
    let note: String
    let message: String
    let theme: Theme
    @ViewBuilder let footer: Footer

    var body: some View {
        ScrollView {
            VStack(spacing: 8) {
                Text(item.title)
                    .font(.reading(26))
                    .foregroundStyle(Color(theme.ink))
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)
                if !note.isEmpty {
                    Text(note).font(.subheadline).foregroundStyle(Color(theme.ink2)).multilineTextAlignment(.center)
                }
                if !message.isEmpty {
                    Text(message)
                        .font(.reading(19))
                        .foregroundStyle(Color(theme.ink))
                        .multilineTextAlignment(.center)
                        .padding(.top, 24)
                }
                footer.padding(.top, 32)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 40)
            .frame(maxWidth: 520)
            .frame(maxWidth: .infinity)
        }
        .background(Color(theme.paper))
    }
}

/// A plan the rule follows that hasn't been started: Start today shows its first day in place.
private struct StartPlanStep: View {
    let title: String
    /// "30 days · a chapter a day".
    let meta: String?
    let theme: Theme
    let start: () -> Void
    let skip: () -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                VStack(spacing: 8) {
                    Text("You haven’t started \(title).")
                        .font(.reading(22))
                        .foregroundStyle(Color(theme.ink))
                    if let meta {
                        Text(meta).font(.subheadline).foregroundStyle(Color(theme.ink2))
                    }
                }
                .multilineTextAlignment(.center)
                StepFooter(theme: theme, primary: "Start today", id: "start-plan", action: start, skip: skip)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 40)
            .frame(maxWidth: 520)
            .frame(maxWidth: .infinity)
        }
        .background(Color(theme.paper))
    }
}

/// The Fathers on the day's Gospel: a chip for each verse (dim where they say nothing), the verse
/// in the reader's type, its quotes, then the next verse. Done and Skip work without the quotes
/// installed: they may be read from a book.
private struct FathersStep<Footer: View>: View {
    @Environment(Store.self) private var store
    @Environment(\.dynamicTypeSize) private var dynamicType
    let parts: [RulePart]
    let intro: String
    let key: String
    let theme: Theme
    @ViewBuilder let footer: Footer
    @State private var verses: [VerseCount] = []
    @State private var at = 0
    @State private var installed = true
    @State private var width: CGFloat = 0

    private var typography: Typography {
        var style = store.typography(theme, dynamicType: dynamicType)
        style.width = width
        return style
    }

    var body: some View {
        let style = typography
        let column = style.column
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Group {
                        if !intro.isEmpty {
                            Text(intro)
                                .font(Font(Fonts.literata(style.s(17).rounded())))
                                .foregroundStyle(Color(theme.ink2))
                        }
                        if !installed {
                            FathersInstall(theme: theme, installed: load)
                        } else if verses.indices.contains(at) {
                            chips
                        }
                    }
                    .frame(width: column.width, alignment: .leading)
                    .padding(.leading, column.x)
                    .id("top")
                    if installed, verses.indices.contains(at) {
                        VerseText(ref: verses[at].verse, theme: theme, style: style)
                        Group {
                            FathersQuotes(ref: verses[at].verse, theme: theme)
                            if verses.indices.contains(at + 1) {
                                let v = verses[at + 1].verse
                                Button("Next verse: \(v.chapter):\(v.verse)") { go(at + 1) }
                                    .font(.headline)
                                    .frame(maxWidth: .infinity, minHeight: 44)
                            }
                        }
                        .frame(width: column.width, alignment: .leading)
                        .padding(.leading, column.x)
                    }
                    footer
                        .frame(width: column.width)
                        .padding(.leading, column.x)
                        .padding(.top, 8)
                }
                .padding(.vertical, 20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: at) { withAnimation { proxy.scrollTo("top", anchor: .top) } }
        }
        .background(Color(theme.paper))
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .onAppear(perform: load)
    }

    /// The verses, sideways in one row, in order at every text size. A reading over several
    /// chapters (Holy Friday) starts each chapter's with its number: "14:1".
    private var chips: some View {
        let chapters = Set(verses.map(\.verse.chapter)).count > 1
        return ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(spacing: 8) {
                    ForEach(verses.indices, id: \.self) { i in
                        let v = verses[i]
                        let starts = i == 0 || verses[i - 1].verse.chapter != v.verse.chapter
                        Button { go(i) } label: {
                            Text(chapters && starts ? "\(v.verse.chapter):\(v.verse.verse)" : "\(v.verse.verse)")
                                .font(.subheadline.weight(i == at ? .semibold : .regular))
                                .monospacedDigit()
                                .padding(.horizontal, 10)
                                .frame(minWidth: 44, minHeight: 44)
                                .foregroundStyle(
                                    i == at ? Color(theme.onAccent) : v.quotes == 0 ? Color(theme.ink3) : Color(theme.ink))
                                .background(
                                    i == at ? Color(theme.accent) : v.quotes == 0 ? Color.clear : Color(theme.fill),
                                    in: Capsule())
                                .contentShape(Capsule())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(rangeLabel(from: v.verse, to: v.verse))
                        .accessibilityValue(v.quotes == 0 ? "Nothing from the Fathers" : "")
                        .accessibilityAddTraits(i == at ? .isSelected : [])
                        .id(i)
                    }
                }
            }
            .onAppear { proxy.scrollTo(at, anchor: .center) }
            .onChange(of: at) { withAnimation { proxy.scrollTo(at, anchor: .center) } }
        }
    }

    private func go(_ i: Int) {
        at = i
        store.fathersAt[key] = i
    }

    private func load() {
        installed = store.bible.fathersInstalled()
        verses = store.bible.fathersCounts(passages: parts.flatMap(\.passages))
        at = min(store.fathersAt[key] ?? 0, max(verses.count - 1, 0))
    }
}

/// One verse in the reader's type and settings, as the Fathers step shows it.
private struct VerseText: View {
    @Environment(Store.self) private var store
    let ref: VerseRef
    let theme: Theme
    let style: Typography
    @State private var layout: ChapterLayout?
    @State private var tracker = PassageTracker()

    var body: some View {
        Group {
            if let layout {
                SpanCanvasView(layout: layout, theme: theme, tracker: tracker, index: 0)
                    .frame(height: layout.textBottom)
            } else {
                Color.clear.frame(height: style.lineHeight)
            }
        }
        .task(id: Typesetting(ref: ref, style: style, translation: store.settings.translation)) {
            guard style.width > 0 else { return }
            let chapter = store.chapterIndex(of: ref)
            var content = store.bible.chapterSpan(
                translation: store.settings.translation, index: UInt16(chapter), from: ref.verse, to: ref.verse,
                options: style.options)
            // A verse on its own: no chapter head over it, even the first.
            content.head = false
            let typeset = ChapterLayout(content: content, typography: style)
            typeset.marks = Dictionary(
                store.bible.marks(translation: store.settings.translation, chapter: UInt16(chapter)).map { (Int($0.verse), $0) },
                uniquingKeysWith: { a, _ in a })
            layout = typeset
        }
    }

    private struct Typesetting: Equatable {
        let ref: VerseRef
        let style: Typography
        let translation: String
    }
}

/// The calm close of a part of the day: no list and nothing counted, what comes next in the
/// rule, and the way back.
private struct EndPage: View {
    let title: String
    let next: String?
    let theme: Theme
    let back: () -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: 12) {
                Text("†")
                    .font(.reading(34))
                    .foregroundStyle(Color(theme.accent))
                    .accessibilityHidden(true)
                Text(title)
                    .font(.reading(28))
                    .foregroundStyle(Color(theme.ink))
                    .accessibilityAddTraits(.isHeader)
                Text("Glory to God for all things.")
                    .font(.reading(20, italic: true))
                    .foregroundStyle(Color(theme.ink2))
                    .multilineTextAlignment(.center)
                if let next {
                    Text(next)
                        .font(.subheadline)
                        .foregroundStyle(Color(theme.ink2))
                        .multilineTextAlignment(.center)
                        .padding(.top, 20)
                }
                StepFooter(theme: theme, primary: "Back to Today", id: "end-today", action: back)
                    .padding(.top, 28)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 56)
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity)
        }
        .background(Color(theme.paper))
    }
}

extension View {
    /// A step's title with the subtitle under it, and the title menu.
    func stepTitle<Items: View>(
        _ title: String, subtitle: String?, menu: Bool, @ViewBuilder content: () -> Items
    ) -> some View {
        modifier(StepTitle(title: title, subtitle: subtitle?.isEmpty == false ? subtitle : nil, menu: menu, content: content()))
    }
}

private struct StepTitle<Items: View>: ViewModifier {
    let title: String
    let subtitle: String?
    let menu: Bool
    let content: Items

    func body(content view: Content) -> some View {
        #if compiler(>=6.2)
            if #available(iOS 26, *), menu {
                view
                    .navigationTitle(title)
                    .navigationSubtitle(subtitle ?? "")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbarTitleMenu { content }
            } else if #available(iOS 26, *) {
                view
                    .navigationTitle(title)
                    .navigationSubtitle(subtitle ?? "")
                    .navigationBarTitleDisplayMode(.inline)
            } else {
                principal(view)
            }
        #else
            principal(view)
        #endif
    }

    /// Before iOS 26: the title and subtitle drawn as one, opening the menu.
    private func principal(_ view: Content) -> some View {
        view
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Menu {
                        if menu { content }
                    } label: {
                        VStack(spacing: 0) {
                            HStack(spacing: 4) {
                                Text(title).font(.headline)
                                if menu { Image(systemName: "chevron.down").font(.caption.weight(.semibold)) }
                            }
                            if let subtitle { Text(subtitle).font(.caption).foregroundStyle(.secondary) }
                        }
                        .foregroundStyle(.primary)
                    }
                    .disabled(!menu)
                    .accessibilityIdentifier("session-menu")
                }
            }
    }
}
