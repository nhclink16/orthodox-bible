import SwiftUI

/// The prayer rule's editor: its parts of the day (name and reminder time) and their items
/// (added, removed, reordered, each with what it opens, its season and whether it's optional),
/// the reminders, and the starter rule to go back to. Nothing is saved until Save. Items keep
/// their ids when renamed, so past check marks keep their meaning; only new ones get new ids.
struct RuleEditor: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    let theme: Theme
    @State private var rule = Rule(blocks: [])
    /// The switch as drawn: it goes back off when notifications aren't allowed.
    @State private var reminders = false
    @State private var refused = false
    @State private var saved = Rule(blocks: [])
    @State private var path: [ItemRef] = []
    @State private var reordering = false
    @State private var naming = false
    @State private var newName = ""
    @State private var resetting = false
    @State private var removing: RuleBlock?

    var body: some View {
        NavigationStack(path: $path) {
            List {
                Section {
                    Toggle("Reminders", isOn: Binding(
                        get: { reminders },
                        set: { on in
                            reminders = on
                            Task {
                                refused = !(await store.setReminders(on))
                                reminders = store.remindersOn
                            }
                        }
                    ))
                    .accessibilityIdentifier("reminders")
                } footer: {
                    if refused {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Notifications are off for Orthodox Bible.")
                            Button("Turn them on in Settings") {
                                if let url = URL(string: UIApplication.openNotificationSettingsURLString) { openURL(url) }
                            }
                            .font(.footnote.weight(.medium))
                        }
                    } else {
                        Text("A notification at the time set for each part of the day, on the days it has something to do.")
                    }
                }

                // By id, not by place: a part removed while it's drawn must not be looked up.
                ForEach(rule.blocks.map(\.id), id: \.self) { id in
                    let block = binding(block: id)
                    Section {
                        TextField("Name", text: block.title)
                            .font(.headline)
                            .accessibilityLabel("Name of \(block.wrappedValue.title)")
                            .accessibilityIdentifier("name-\(id)")
                        Toggle("At a set time", isOn: Binding(
                            get: { block.wrappedValue.time != nil },
                            set: { block.wrappedValue.time = $0 ? (block.wrappedValue.time ?? "07:00") : nil }
                        ))
                        if let time = RuleTime(block.wrappedValue.time) {
                            DatePicker(
                                "Time",
                                selection: Binding(get: { time.date }, set: { block.wrappedValue.time = RuleTime($0).text }),
                                displayedComponents: .hourAndMinute)
                        }
                        ForEach(block.wrappedValue.items, id: \.id) { item in
                            NavigationLink(value: ItemRef(block: id, item: item.id)) {
                                ItemSummary(item: item, theme: theme)
                            }
                            .accessibilityIdentifier("item-\(item.id)")
                        }
                        .onDelete { block.wrappedValue.items.remove(atOffsets: $0) }
                        .onMove { block.wrappedValue.items.move(fromOffsets: $0, toOffset: $1) }
                        Menu {
                            ForEach(KindChoice.allCases) { choice in
                                Button(choice.label) { add(choice, to: id) }
                            }
                        } label: {
                            // The whole row opens the menu, as a button's would.
                            Label("Add an item", systemImage: "plus")
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                        }
                        .accessibilityIdentifier("add-\(id)")
                        Button("Remove “\(block.wrappedValue.title)”", role: .destructive) { removing = block.wrappedValue }
                            .foregroundStyle(Color(theme.danger))
                            .accessibilityIdentifier("remove-\(id)")
                    }
                }

                Section {
                    Button {
                        newName = ""
                        naming = true
                    } label: {
                        Label("Add a part of the day", systemImage: "plus")
                    }
                    .accessibilityIdentifier("add-part")
                }

                Section {
                    Button("Reset to the starter rule", role: .destructive) { resetting = true }
                        .foregroundStyle(Color(theme.danger))
                }
            }
            .environment(\.editMode, .constant(reordering ? .active : .inactive))
            .navigationTitle("Your rule")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .fontWeight(.semibold)
                }
                ToolbarItem(placement: .bottomBar) {
                    Button(reordering ? "Done" : "Reorder items") {
                        withAnimation { reordering.toggle() }
                    }
                    .accessibilityIdentifier("reorder")
                }
            }
            .navigationDestination(for: ItemRef.self) { ref in
                ItemEditor(item: binding(ref), theme: theme) { remove(ref) }
                    .sheetChrome(theme)
            }
            .alert("New part of the day", isPresented: $naming) {
                TextField("Name", text: $newName)
                Button("Add") { addBlock() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Such as Night or Before work.")
            }
            .confirmationDialog("Reset to the starter rule?", isPresented: $resetting, titleVisibility: .visible) {
                Button("Reset", role: .destructive) {
                    path = []
                    rule = starterRule()
                }
            } message: {
                Text("The starter rule replaces yours when you save. Days already checked off stay as they are.")
            }
            .confirmationDialog(
                "Remove “\(removing?.title ?? "")”?",
                isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
                titleVisibility: .visible, presenting: removing
            ) { block in
                Button("Remove", role: .destructive) { rule.blocks.removeAll { $0.id == block.id } }
            } message: { _ in
                Text("Its items go with it.")
            }
        }
        .interactiveDismissDisabled(rule != saved)
        .onAppear {
            reminders = store.remindersOn
            guard saved.blocks.isEmpty else { return }
            saved = store.bible.rule()
            rule = saved
        }
    }

    private func save() {
        for b in rule.blocks.indices {
            if rule.blocks[b].title.trimmingCharacters(in: .whitespaces).isEmpty { rule.blocks[b].title = "Untitled" }
            for i in rule.blocks[b].items.indices
            where rule.blocks[b].items[i].title.trimmingCharacters(in: .whitespaces).isEmpty {
                rule.blocks[b].items[i].title = "Untitled"
            }
        }
        store.saveRule(rule)
        saved = rule
        dismiss()
    }

    /// A new id, unlike any in the rule.
    private func newID(_ prefix: String) -> String {
        let taken = Set(rule.blocks.flatMap { [$0.id] + $0.items.map(\.id) })
        while true {
            let id = "\(prefix)-\(UUID().uuidString.prefix(6).lowercased())"
            if !taken.contains(id) { return id }
        }
    }

    private func add(_ choice: KindChoice, to block: String) {
        guard let b = rule.blocks.firstIndex(where: { $0.id == block }) else { return }
        let kind = choice.kind
        let item = RuleItem(
            id: newID("item"), title: kind.defaultTitle, kind: kind, note: choice.note, season: .always,
            optional: false)
        rule.blocks[b].items.append(item)
        path.append(ItemRef(block: block, item: item.id))
    }

    private func addBlock() {
        let name = newName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        rule.blocks.append(RuleBlock(id: newID("part"), title: name, time: nil, items: []))
    }

    private func remove(_ ref: ItemRef) {
        guard let b = rule.blocks.firstIndex(where: { $0.id == ref.block }) else { return }
        rule.blocks[b].items.removeAll { $0.id == ref.item }
    }

    /// A part of the day, looked up by id each time (it may be removed while drawn).
    private func binding(block id: String) -> Binding<RuleBlock> {
        Binding(
            get: { rule.blocks.first { $0.id == id } ?? RuleBlock(id: id, title: "", time: nil, items: []) },
            set: { block in
                guard let b = rule.blocks.firstIndex(where: { $0.id == id }) else { return }
                rule.blocks[b] = block
            })
    }

    /// An item of the rule, looked up by id each time (it may be removed while open).
    private func binding(_ ref: ItemRef) -> Binding<RuleItem> {
        Binding(
            get: {
                rule.blocks.first { $0.id == ref.block }?.items.first { $0.id == ref.item }
                    ?? RuleItem(id: ref.item, title: "", kind: .other, note: "", season: .always, optional: false)
            },
            set: { item in
                guard let b = rule.blocks.firstIndex(where: { $0.id == ref.block }),
                    let i = rule.blocks[b].items.firstIndex(where: { $0.id == ref.item })
                else { return }
                rule.blocks[b].items[i] = item
            })
    }
}

private struct ItemRef: Hashable {
    let block: String
    let item: String
}

/// An item in the editor's list: its title, and what it is.
private struct ItemSummary: View {
    let item: RuleItem
    let theme: Theme

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(item.title.isEmpty ? "Untitled" : item.title)
                .foregroundStyle(Color(theme.ink))
            let kind = KindChoice(item.kind).label
            let line = [kind == item.title ? nil : kind, item.season.label, item.optional ? "Optional" : nil].compactMap { $0 }
            if !line.isEmpty {
                Text(line.joined(separator: " · "))
                    .font(.subheadline)
                    .foregroundStyle(Color(theme.ink2))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .padding(.vertical, 2)
    }
}

/// One item: its title and note, what it opens, its season and whether it's optional.
private struct ItemEditor: View {
    @Environment(\.dismiss) private var dismiss
    @Binding var item: RuleItem
    let theme: Theme
    let remove: () -> Void
    @FocusState private var focus: Field?
    /// The psalm's number as typed; the psalm saved once it's a number.
    @State private var psalm = ""

    var body: some View {
        Form {
            Section {
                TextField("Title", text: $item.title)
                    .focused($focus, equals: .title)
                    .accessibilityIdentifier("item-title")
                TextField("Note", text: $item.note, axis: .vertical)
                    .focused($focus, equals: .note)
                    .accessibilityIdentifier("item-note")
            } footer: {
                Text("The note is the line under the title on Today.")
            }

            Section {
                Picker("Kind", selection: Binding(get: { KindChoice(item.kind) }, set: change)) {
                    ForEach(KindChoice.allCases) { Text($0.label).tag($0) }
                }
                details
            } footer: {
                Text(KindChoice(item.kind).footer)
            }

            Section {
                Picker("When", selection: $item.season) {
                    Text("Always").tag(RuleSeason.always)
                    Text("Great Lent only").tag(RuleSeason.greatLent)
                    Text("Outside Lent").tag(RuleSeason.outsideLent)
                }
                Toggle("Optional", isOn: $item.optional)
            } footer: {
                Text("Great Lent includes Holy Week. An optional item doesn’t decide whether the day’s rule was kept.")
            }

            Section {
                Button("Remove item", role: .destructive) {
                    dismiss()
                    remove()
                }
                .foregroundStyle(Color(theme.danger))
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                Spacer()
                Button("Done") { focus = nil }
            }
        }
        .navigationTitle(item.title.isEmpty ? "Item" : item.title)
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { if item.title.isEmpty { focus = .title } }
    }

    /// What the kind needs: the prayer, the psalm, the plan or the count.
    @ViewBuilder
    private var details: some View {
        switch item.kind {
        case .prayers(let prayer, let text):
            let builtIn = prayers()
            Picker("Prayer", selection: Binding(
                get: { prayer ?? (text == nil ? "book" : "own") },
                set: { choose(prayer: $0, keeping: text) }
            )) {
                Text("My prayer book").tag("book")
                ForEach(builtIn, id: \.id) { Text($0.title).tag($0.id) }
                if let prayer, !builtIn.contains(where: { $0.id == prayer }) {
                    Text(item.title).tag(prayer)
                }
                Text("My own words").tag("own")
            }
            if prayer == nil, let text {
                TextField("The prayer", text: Binding(
                    get: { text }, set: { item.kind = .prayers(prayer: nil, text: $0) }
                ), axis: .vertical)
                .lineLimit(4...12)
                .font(.reading(17))
                .focused($focus, equals: .words)
                .accessibilityIdentifier("own-words")
            }
        case .psalm(let lxx):
            LabeledContent("Psalm") {
                // Emptied to type a new number over the one it shows; the number saved once
                // typing is over.
                TextField("\(lxx)", text: $psalm)
                    .keyboardType(.numberPad)
                    .multilineTextAlignment(.trailing)
                    .focused($focus, equals: .psalm)
                    .accessibilityLabel("Psalm number")
                    .accessibilityIdentifier("psalm")
                    .onAppear { psalm = "\(lxx)" }
                    .onChange(of: psalm) {
                        guard let n = Int(psalm), n >= 1 else { return }
                        let new = UInt8(min(n, 151))
                        if item.title == RuleKind.psalm(lxx: lxx).defaultTitle { item.title = RuleKind.psalm(lxx: new).defaultTitle }
                        item.kind = .psalm(lxx: new)
                    }
                    .onChange(of: focus) { psalm = focus == .psalm ? "" : "\(lxx)" }
            }
        case .plan(let plan):
            Picker("Plan", selection: Binding(
                get: { plan },
                set: { new in
                    if item.title == RuleKind.plan(plan: plan).defaultTitle { item.title = RuleKind.plan(plan: new).defaultTitle }
                    item.kind = .plan(plan: new)
                }
            )) {
                ForEach(readingPlans(), id: \.id) { Text($0.title).tag($0.id) }
            }
        case .jesusPrayer(let count):
            Picker("Times", selection: Binding(get: { count }, set: { item.kind = .jesusPrayer(count: $0) })) {
                ForEach(Array(Set(KindChoice.counts + [count])).sorted(), id: \.self) { Text("\($0)").tag($0) }
            }
        default:
            EmptyView()
        }
    }

    private enum Field { case title, note, words, psalm }

    private func change(_ choice: KindChoice) {
        let old = KindChoice(item.kind)
        guard choice != old else { return }
        if item.title.isEmpty || item.title == item.kind.defaultTitle { item.title = choice.kind.defaultTitle }
        if item.note == old.note { item.note = choice.note }
        item.kind = choice.kind
    }

    private func choose(prayer id: String, keeping text: String?) {
        let book = KindChoice.prayers.note
        switch id {
        case "book":
            item.kind = .prayers(prayer: nil, text: nil)
            if item.note.isEmpty { item.note = book }
        case "own":
            item.kind = .prayers(prayer: nil, text: text ?? "")
            if item.note == book { item.note = "" }
        default:
            if let p = prayers().first(where: { $0.id == id }), item.title == KindChoice.prayers.kind.defaultTitle {
                item.title = p.title
            }
            item.kind = .prayers(prayer: id, text: nil)
            if item.note == book { item.note = "" }
        }
    }
}

/// The kinds of item, as the editor offers them.
private enum KindChoice: CaseIterable, Identifiable, Hashable {
    case prayers, kathisma, psalm, readings, plan, fathers, jesusPrayer, reflection, other

    /// Prayer-rope counts.
    static let counts: [UInt16] = [12, 33, 50, 100, 150, 300, 500, 1000]

    var id: Self { self }

    init(_ kind: RuleKind) {
        self =
            switch kind {
            case .prayers: .prayers
            case .kathisma: .kathisma
            case .psalm: .psalm
            case .readings: .readings
            case .plan: .plan
            case .fathers: .fathers
            case .jesusPrayer: .jesusPrayer
            case .reflection: .reflection
            case .other: .other
            }
    }

    var label: String {
        switch self {
        case .prayers: "Prayers"
        case .kathisma: "The day’s kathisma"
        case .psalm: "A psalm"
        case .readings: "The day’s readings"
        case .plan: "A reading plan"
        case .fathers: "The Fathers"
        case .jesusPrayer: "The Jesus Prayer"
        case .reflection: "Examination of conscience"
        case .other: "Something else"
        }
    }

    var kind: RuleKind {
        switch self {
        case .prayers: .prayers(prayer: nil, text: nil)
        case .kathisma: .kathisma
        case .psalm: .psalm(lxx: 50)
        case .readings: .readings
        case .plan: .plan(plan: readingPlans().first?.id ?? "")
        case .fathers: .fathers
        case .jesusPrayer: .jesusPrayer(count: 33)
        case .reflection: .reflection
        case .other: .other
        }
    }

    var note: String {
        switch self {
        case .prayers: "From your prayer book"
        case .kathisma: "The day’s kathisma"
        case .readings: "The day’s Epistle and Gospel"
        case .fathers: "On the day’s Gospel"
        case .reflection: "Before sleep"
        default: ""
        }
    }

    var footer: String {
        switch self {
        case .prayers: "A prayer from the app, your own words, or a line pointing to your prayer book."
        case .kathisma: "Opens the day’s kathisma: at Matins in Morning or a part of the day before noon, at Vespers in the others."
        case .psalm: "Numbered as in the Septuagint: Psalm 50 is “Have mercy on me, O God”."
        case .readings: "The day’s Epistle and Gospel are coming soon."
        case .plan: "Opens the plan’s reading for the day, and is done once that’s read."
        case .fathers: "The Fathers on the day’s Gospel are coming soon."
        case .jesusPrayer: "Opens a quiet counter."
        case .reflection: "Opens a few lines for looking back over the day."
        case .other: "Anything else, described by its title and note."
        }
    }
}

extension RuleKind {
    /// The title a new item of this kind starts with.
    var defaultTitle: String {
        switch self {
        case .prayers: "Prayers"
        case .kathisma: "The Psalter"
        case .psalm(let lxx): "Psalm \(lxx)"
        case .readings: "Scripture"
        case .plan(let plan): readingPlans().first { $0.id == plan }?.title ?? "Reading plan"
        case .fathers: "The Fathers"
        case .jesusPrayer: "The Jesus Prayer"
        case .reflection: "Examination of conscience"
        case .other: ""
        }
    }
}

extension RuleSeason {
    /// How the editor's list names a season; nothing for always.
    var label: String? {
        switch self {
        case .always: nil
        case .greatLent: "Great Lent only"
        case .outsideLent: "Outside Lent"
        }
    }
}
