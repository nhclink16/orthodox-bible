import Foundation
import Observation
import UIKit
import os

/// The reader's preferences, saved in the study database (under the "ios" setting) as JSON.
struct ReaderSettings: Codable, Equatable {
    var translation = "lxx"
    var appearance = Appearance.system
    /// Points at the default Dynamic Type size.
    var textSize: Double = 18
    var lineSpacing: Double = 1.6
    var redLetters = true
    var verseNumbers = true
    var versePerLine = false
    var headings = true
    var wholeWords = false
    /// Psalms numbered as the Septuagint numbers them (Orthodox fork).
    var septuagintPsalms = true
    /// Feasts and fasts on the Old (Julian) calendar rather than the New.
    var oldCalendar = false
    /// Whose calendar of readings to follow: the Greek Orthodox Archdiocese's or the Antiochian's.
    var usage = ReadingUsage.greek
    /// Reminders at the rule's set times, once notifications are allowed.
    var ruleReminders = true

    static let textSizes: ClosedRange<Double> = 14...32
    static let lineSpacings: ClosedRange<Double> = 1.3...2.0

    init() {}

    /// Settings saved by an older or newer build decode too: anything missing keeps its default.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = ReaderSettings()
        func value<T: Decodable>(_ key: CodingKeys, _ fallback: T) -> T {
            ((try? c.decodeIfPresent(T.self, forKey: key)) ?? nil) ?? fallback
        }
        translation = value(.translation, d.translation)
        appearance = value(.appearance, d.appearance)
        textSize = value(.textSize, d.textSize).clamped(to: Self.textSizes)
        lineSpacing = value(.lineSpacing, d.lineSpacing).clamped(to: Self.lineSpacings)
        redLetters = value(.redLetters, d.redLetters)
        verseNumbers = value(.verseNumbers, d.verseNumbers)
        versePerLine = value(.versePerLine, d.versePerLine)
        headings = value(.headings, d.headings)
        wholeWords = value(.wholeWords, d.wholeWords)
        septuagintPsalms = value(.septuagintPsalms, d.septuagintPsalms)
        oldCalendar = value(.oldCalendar, d.oldCalendar)
        usage = value(.usage, d.usage)
        ruleReminders = value(.ruleReminders, d.ruleReminders)
    }
}

/// Saved under the names the core gives them: "greek", "antiochian".
extension ReadingUsage: Codable {
    public init(from decoder: Decoder) throws {
        self = try decoder.singleValueContainer().decode(String.self) == "antiochian" ? .antiochian : .greek
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(self == .antiochian ? "antiochian" : "greek")
    }
}

extension Comparable {
    func clamped(to range: ClosedRange<Self>) -> Self { min(max(self, range.lowerBound), range.upperBound) }
}

/// A local date's civil year, month and day, as the core takes them.
struct CivilDate {
    let year: Int32
    let month: UInt8
    let day: UInt8

    init(_ date: Date) {
        let parts = Calendar(identifier: .gregorian).dateComponents([.year, .month, .day], from: date)
        year = Int32(parts.year ?? 2026)
        month = UInt8(parts.month ?? 1)
        day = UInt8(parts.day ?? 1)
    }

    /// Days since 1970-01-01.
    var number: Int64 { civilDay(year: year, month: month, day: day) }
}

/// A chapter (0–1364 in the canonical grid) and verse.
struct Position: Equatable {
    var chapter: Int
    var verse: Int
}

/// A request for the reader to move.
struct JumpRequest: Equatable {
    let id = UUID()
    var chapter: Int
    var verse: Int
    var animated = false
    var flash = false
}

/// A part of the rule being prayed through in Today (Orthodox fork): its day, fixed at Begin, and
/// the item open, or none for the end page.
struct RuleSession: Equatable {
    var date: Date
    var block: String
    var item: String?
    /// Passed over without a tick; only Continue minds them.
    var skipped: Set<String> = []
    /// Back to Today after the last item: kept only so the page shows as it goes.
    var over = false
}

/// Readings opened from Today's Readings, Psalter or a plan, read in Today one after another.
struct ReadingWalk: Equatable {
    var date: Date
    var steps: [WalkStep]
    var index: Int
}

struct WalkStep: Equatable {
    /// "Gospel", "Kathisma 12", "Psalm 2".
    let title: String
    /// Where it's read, for the subtitle and the menu's groups: "Liturgy", "Day 3".
    let service: String
    /// The line over the text: "Luke 9:23–27 · Great Martyr Artemius".
    let intro: String?
    let parts: [RulePart]
    /// A plan's passage, which Done checks off.
    var plan: PlanPassageRef?
}

/// A passage of a plan's day: the plan, the day and its place in the day (from 0).
struct PlanPassageRef: Equatable {
    let id: String
    let day: UInt32
    let passage: Int
}

enum Sheet: Identifiable, Equatable {
    case books, search, settings, study, today
    case note(VerseRef)
    case fathers(VerseRef)

    var id: String {
        switch self {
        case .books: "books"
        case .search: "search"
        case .settings: "settings"
        case .study: "study"
        case .today: "today"
        case .note(let v): "note-\(v.book)-\(v.chapter)-\(v.verse)"
        case .fathers(let v): "fathers-\(v.book)-\(v.chapter)-\(v.verse)"
        }
    }
}

@MainActor
@Observable
final class Store {
    @ObservationIgnored let bible: Bible
    @ObservationIgnored let bookList: [BookInfo]
    private(set) var translations: [TranslationInfo]

    var settings: ReaderSettings {
        didSet {
            guard settings != oldValue else { return }
            if settings.translation != oldValue.translation { translationChanged() }
            if settings.septuagintPsalms != oldValue.septuagintPsalms {
                setSeptuagintNumbering(on: settings.septuagintPsalms)
            }
            scheduleSave()
        }
    }

    /// Where the reader is; the reader view reports it as you scroll.
    private(set) var position: Position
    private(set) var jump: JumpRequest?
    var selection: VerseSelection?
    /// Bumped whenever highlights, notes or bookmarks change.
    private(set) var marksVersion = 0
    var sheet: Sheet? {
        didSet {
            if sheet == .today { returnToToday = false }
            if oldValue == .today, sheet != .today { todayClosed() }
        }
    }
    var barsHidden = false
    /// Bumped when a reminder asks for Today, which then goes back to today's date.
    private(set) var todayRequest = 0
    private(set) var toast: String?
    /// Bumped whenever the prayer rule, a check mark or a reading plan changes.
    private(set) var ruleVersion = 0 {
        didSet { keepSessionInRule() }
    }
    /// Where Today is: the pages pushed on it, and the day shown (from today).
    var todayPath: [TodayRoute] = []
    var todayOffset = 0
    /// The part of the rule under way in Today, kept while Today is closed.
    private(set) var session: RuleSession?
    private(set) var walk: ReadingWalk?
    /// Parts of the day finished today (their end page reached), by day: the Now card moves on.
    private(set) var ended: [Int64: Set<String>] = [:]
    /// The Jesus Prayer counted so far, by day and item.
    var counts: [String: Int] = [:]
    /// Where each reading of a step had got to (by `placeKey`), so it opens there again; and
    /// which of its verses the Fathers step was on.
    @ObservationIgnored var places: [String: PassagePlace] = [:]
    @ObservationIgnored var fathersAt: [String: Int] = [:]
    /// The reader was opened from a step in Today: a capsule leads back to it.
    private(set) var returnToToday = false
    /// The rule's reminders are on and notifications allowed.
    private(set) var remindersOn = false
    /// The folder synced with the Mac, as a bookmark; `nil` while sync is off.
    private(set) var syncBookmark: Data?
    private(set) var syncing = false
    private(set) var lastSynced: Date?
    /// "Orthodox Bible › Sync".
    private(set) var syncFolder: String?
    /// Why the last sync failed.
    private(set) var syncError: String?
    /// The files the last sync couldn't read, each with why.
    private(set) var syncSkipped: [String] = []
    /// The other devices' files the last sync read: none in a folder the Mac doesn't sync.
    private(set) var syncDevices: UInt32 = 0
    /// Files the last sync found still coming down from iCloud.
    private(set) var syncWaiting = 0

    @ObservationIgnored let reminders = Reminders()
    /// A reminder was tapped while a note was being written: Today opens once it's closed.
    @ObservationIgnored private var todayPending = false
    @ObservationIgnored private var saveTask: Task<Void, Never>?
    @ObservationIgnored private var toastTask: Task<Void, Never>?
    @ObservationIgnored private var syncTask: Task<Void, Never>?
    /// Asked for while a sync ran: one more follows it.
    @ObservationIgnored private var syncAgain = false
    /// Called once the syncs asked for are done.
    @ObservationIgnored private var syncDone: [() -> Void] = []
    /// Syncs run again in a row for files still coming down from iCloud.
    @ObservationIgnored private var syncRetries = 0
    /// The rule as it was before the first sync with another device, which may replace it.
    @ObservationIgnored private var ruleBeforeSync: Rule?
    @ObservationIgnored private var backgroundSync = UIBackgroundTaskIdentifier.invalid

    init() {
        let bible = Self.openBible()
        // Before any sync: a copy of the database restored onto another phone syncs under an id
        // of its own.
        if let machine = UIDevice.current.identifierForVendor?.uuidString {
            try? bible.bindMachine(machine: machine)
        }
        let bookList = books()
        let translations = bible.translations()
        var settings = ReaderSettings()
        let john = bookList.first { $0.name == "John" } ?? bookList[0]
        var position = Position(chapter: Int(john.firstChapter), verse: 1)
        if let json = bible.setting(key: "ios"), let saved = try? JSONDecoder().decode(Saved.self, from: Data(json.utf8)) {
            settings = saved.settings ?? settings
            if let chapter = saved.chapter, (0..<ReaderView.chapterCount).contains(chapter) {
                position = Position(chapter: chapter, verse: max(saved.verse ?? 1, 1))
            }
        }
        if !translations.contains(where: { $0.id == settings.translation }) {
            settings.translation = translations.first?.id ?? "lxx"
        }
        self.bible = bible
        self.bookList = bookList
        self.translations = translations
        self.settings = settings
        self.position = position
        syncBookmark = bible.setting(key: Self.syncKey).flatMap { $0.isEmpty ? nil : Data(base64Encoded: $0) }
        setSeptuagintNumbering(on: settings.septuagintPsalms)
        jump = JumpRequest(chapter: position.chapter, verse: position.verse)
        reminders.opened = { [weak self] block in self?.openToday(block: block) }
    }

    /// The study database and imported texts live in Application Support. If that can't be
    /// opened the reader still works, keeping study data in a temporary folder.
    private static func openBible() -> Bible {
        let fm = FileManager.default
        let support = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        var dir = support.appendingPathComponent("bible-rs", isDirectory: true)
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("--uitest") {
            // UI tests start from nothing (unless relaunching) and leave your notes alone.
            dir = fm.temporaryDirectory.appendingPathComponent("bible-rs-uitest", isDirectory: true)
            if !arguments.contains("--keep-data") { try? fm.removeItem(at: dir) }
        }
        try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        do {
            return try Bible(dataDir: dir.path)
        } catch {
            Log.app.error("Couldn't open \(dir.path, privacy: .public): \(error.localizedDescription, privacy: .public)")
            let tmp = fm.temporaryDirectory.appendingPathComponent("bible-rs", isDirectory: true)
            try? fm.createDirectory(at: tmp, withIntermediateDirectories: true)
            return try! Bible(dataDir: tmp.path)
        }
    }

    // MARK: Text

    var translation: TranslationInfo? { translations.first { $0.id == settings.translation } }

    var contentOptions: ContentOptions {
        ContentOptions(
            versePerLine: settings.versePerLine, headings: settings.headings, verseNumbers: settings.verseNumbers)
    }

    /// Decodes the translation and builds its search text off the main thread.
    func warm() {
        let bible = bible
        let ids = [settings.translation]
        Task.detached(priority: .utility) { bible.warm(translations: ids) }
    }

    private func translationChanged() {
        selection = nil
        warm()
    }

    // MARK: Books and chapters

    func book(of chapter: Int) -> BookInfo {
        bookList.last { Int($0.firstChapter) <= chapter } ?? bookList[0]
    }

    func ref(chapter: Int, verse: Int) -> VerseRef {
        let book = book(of: chapter)
        return VerseRef(book: book.index, chapter: UInt8(chapter - Int(book.firstChapter) + 1), verse: UInt8(verse))
    }

    var title: String { chapterLabel(index: UInt16(position.chapter)) }

    // MARK: Moving

    func go(to chapter: Int, verse: Int = 1, flash: Bool = false, animated: Bool = false) {
        selection = nil
        barsHidden = false
        position = Position(chapter: chapter, verse: verse)
        jump = JumpRequest(chapter: chapter, verse: verse, animated: animated, flash: flash)
        scheduleSave()
    }

    func go(to reference: Reference) {
        go(to: Int(reference.chapterIndex), verse: Int(reference.verse ?? 1), flash: reference.verse != nil)
    }

    func reached(_ chapter: Int, verse: Int) {
        let next = Position(chapter: chapter, verse: verse)
        guard next != position else { return }
        position = next
        scheduleSave()
    }

    // MARK: Selection

    func tap(verse: Int, chapter: Int) {
        barsHidden = false
        if var s = selection, s.chapter == chapter {
            if s.verses.contains(verse) { s.verses.remove(verse) } else { s.verses.insert(verse) }
            selection = s.verses.isEmpty ? nil : s
        } else {
            selection = VerseSelection(chapter: chapter, verses: [verse])
        }
    }

    func tapPage() {
        if selection != nil {
            selection = nil
        } else {
            barsHidden.toggle()
        }
    }

    var selectedRefs: [VerseRef] {
        guard let s = selection else { return [] }
        return s.verses.sorted().map { ref(chapter: s.chapter, verse: $0) }
    }

    /// "John 3:16", "John 3:16–18" or "John 3:16, 18".
    var selectionLabel: String {
        let refs = selectedRefs
        guard let first = refs.first, let last = refs.last else { return "" }
        let contiguous = Int(last.verse) - Int(first.verse) == refs.count - 1
        if contiguous { return rangeLabel(from: first, to: last) }
        return rangeLabel(from: first, to: first) + ", " + refs.dropFirst().map { "\($0.verse)" }.joined(separator: ", ")
    }

    var selectionMarks: [VerseMarks] {
        guard let s = selection else { return [] }
        return bible.marks(translation: settings.translation, chapter: UInt16(s.chapter))
            .filter { s.verses.contains(Int($0.verse)) }
    }

    func highlightSelection(_ color: HighlightColor?) {
        study { for ref in selectedRefs { try bible.setHighlight(verse: ref, color: color) } }
        selection = nil
    }

    func toggleBookmarkOnSelection() {
        let refs = selectedRefs
        let marked = Set(selectionMarks.filter(\.bookmark).map(\.verse))
        let on = refs.contains { !marked.contains($0.verse) }
        study { for ref in refs { try bible.setBookmark(verse: ref, on: on) } }
        show(on ? (refs.count == 1 ? "Bookmarked" : "Bookmarked \(refs.count) verses") : "Bookmark removed")
        selection = nil
    }

    /// The Church Fathers on the first selected verse.
    func fathersOnSelection() {
        guard let first = selectedRefs.first else { return }
        selection = nil
        sheet = .fathers(first)
    }

    func noteOnSelection() {
        guard let first = selectedRefs.first else { return }
        sheet = .note(first)
    }

    /// The selected verses as text with their reference, as the Mac copies them.
    var selectionText: String {
        (try? bible.copyText(translation: settings.translation, verses: selectedRefs)) ?? ""
    }

    func copySelection() {
        UIPasteboard.general.string = selectionText
        show("Copied \(selectionLabel)")
        selection = nil
    }

    // MARK: Study

    func note(_ ref: VerseRef) -> String { (try? bible.note(verse: ref)) ?? "" }

    @discardableResult
    func setNote(_ ref: VerseRef, _ body: String) -> Bool {
        study { try bible.setNote(verse: ref, body: body) }
    }

    /// Adds `text` to the verse's note, after a blank line if there is one already.
    func appendToNote(_ ref: VerseRef, _ text: String) {
        let existing = note(ref).trimmingCharacters(in: .whitespacesAndNewlines)
        if setNote(ref, existing.isEmpty ? text : existing + "\n\n" + text) {
            show("Added to your note on \(rangeLabel(from: ref, to: ref))")
        }
    }

    func setHighlight(_ ref: VerseRef, _ color: HighlightColor?) {
        study { try bible.setHighlight(verse: ref, color: color) }
    }

    func setBookmark(_ ref: VerseRef, _ on: Bool) {
        study { try bible.setBookmark(verse: ref, on: on) }
    }

    func items(_ kind: StudyKind) -> [StudyItem] {
        bible.studyItems(kind: kind, translation: settings.translation)
    }

    func export(_ format: ExportFormat) -> String? {
        try? bible.export(format: format, translation: settings.translation)
    }

    @discardableResult
    private func study(_ change: () throws -> Void) -> Bool {
        defer {
            marksVersion += 1
            syncSoon()
        }
        do {
            try change()
            return true
        } catch {
            show("Couldn’t save: \(error.localizedDescription)")
            return false
        }
    }

    // MARK: Prayer rule and plans

    /// The rule on a day, with what was checked off; given the time it is `now` (on that day), it
    /// also says which part of the day the Now card offers.
    func ruleDay(_ date: Date, now: Date? = nil, ending block: String? = nil) -> RuleDay {
        _ = ruleVersion
        let d = CivilDate(date)
        let minute = now.map { Calendar.current.dateComponents([.hour, .minute], from: $0) }
            .map { UInt16(($0.hour ?? 0) * 60 + ($0.minute ?? 0)) }
        let ended = (ended[d.number] ?? []).union(block.map { [$0] } ?? [])
        return bible.ruleDay(
            year: d.year, month: d.month, day: d.day, oldCalendar: settings.oldCalendar, usage: settings.usage,
            midnight: Self.midnight(date), minute: minute, ended: now == nil ? [] : Array(ended))
    }

    func month(year: Int32, month: UInt8) -> [MonthDay] {
        _ = ruleVersion
        let first = Calendar.current.date(from: DateComponents(year: Int(year), month: Int(month), day: 1)) ?? Date()
        return bible.churchMonth(
            year: year, month: month, oldCalendar: settings.oldCalendar, midnight: Self.midnight(first))
    }

    /// Checks an item off on `day`, or clears it. A plan's item checks off that day's passages
    /// instead, which say whether it's done, as on the Mac.
    func setDone(_ item: RuleDayItem, day: Int64, _ done: Bool) {
        changing {
            var ticked = done
            if case .plan(let plan, let planDay) = item.open {
                for i in (bible.planDay(plan: plan, day: planDay)?.passages ?? []).indices {
                    try bible.setPlanRead(plan: plan, day: planDay, passage: UInt8(i), read: done)
                }
                ticked = false
            }
            try bible.setRuleDone(day: day, item: item.id, done: ticked)
        }
    }

    func saveRule(_ rule: Rule) {
        changing { try bible.setRule(rule: rule) }
        if ruleBeforeSync != nil { ruleBeforeSync = rule }
        Task { await refreshReminders() }
    }

    /// Plans under way, as of a day.
    func activePlans(_ date: Date = Date()) -> [ActivePlan] {
        _ = ruleVersion
        return bible.activePlans(today: Self.midnight(date))
    }

    func planDay(_ plan: String, _ day: UInt32) -> PlanDay? {
        _ = ruleVersion
        return bible.planDay(plan: plan, day: day)
    }

    func planDays(_ plan: String) -> [PlanDay] {
        _ = ruleVersion
        return bible.planDays(plan: plan)
    }

    func startPlan(_ plan: String) {
        changing { try bible.startPlan(plan: plan, today: Self.midnight(Date())) }
    }

    func stopPlan(_ plan: String) {
        changing { try bible.stopPlan(plan: plan) }
    }

    func setRead(_ plan: String, day: UInt32, passage: Int, _ read: Bool) {
        changing { try bible.setPlanRead(plan: plan, day: day, passage: UInt8(passage), read: read) }
    }

    /// Shows Today at today's date, after the note being written, if one is. A reminder names its
    /// part of the day, which opens at its first item not done (Today itself once it's all done);
    /// one set before reminders named it opens Today.
    func openToday(block: String? = nil) {
        todayRequest += 1
        todayOffset = 0
        if let block, let item = startItem(block, on: Date()) {
            open(item, block: block, on: Date())
        } else {
            todayPath = []
        }
        if case .note = sheet { todayPending = true } else { sheet = .today }
    }

    /// A sheet has gone: Today, if a reminder asked for it meanwhile.
    func sheetClosed() {
        guard todayPending, sheet == nil else { return }
        todayPending = false
        sheet = .today
    }

    // MARK: Praying through a part of the day

    /// A part of the day on `date`, with its items then.
    func block(_ id: String, on date: Date) -> RuleDayBlock? {
        ruleDay(date).blocks.first { $0.id == id }
    }

    /// What an item of the rule is (the day's items say what they open, not what they are).
    func kind(of item: String) -> RuleKind? {
        _ = ruleVersion
        return bible.rule().blocks.lazy.flatMap(\.items).first { $0.id == item }?.kind
    }

    /// The item of part of the day `block` on `date` after `after` (from the top when `nil`) that
    /// isn't done and has something to do then.
    func ruleNext(_ block: String, after: String?, on date: Date) -> String? {
        let d = CivilDate(date)
        return bible.ruleNext(
            year: d.year, month: d.month, day: d.day, oldCalendar: settings.oldCalendar,
            midnight: Self.midnight(date), block: block, after: after)
    }

    /// What Begin or Continue opens in part of the day `block` on `date`: the step paused there,
    /// or once it's done the next not done after it (passing over those skipped), or else the
    /// first not done. `nil` when there's nothing left to do.
    func startItem(_ block: String, on date: Date) -> String? {
        guard let s = session, !s.over, s.block == block, Self.sameDay(s.date, date), let paused = s.item else {
            return ruleNext(block, after: nil, on: date)
        }
        if self.block(block, on: date)?.items.first(where: { $0.id == paused })?.done == false { return paused }
        var next = ruleNext(block, after: paused, on: date)
        while let n = next, s.skipped.contains(n) { next = ruleNext(block, after: n, on: date) }
        return next ?? ruleNext(block, after: nil, on: date)
    }

    /// Whether Begin reads Continue: something in the part of the day is done, or paused.
    func begun(_ block: RuleDayBlock, on date: Date) -> Bool {
        block.items.contains(where: \.done) || paused(in: block.id, on: date) != nil
    }

    /// The item paused in part of the day `block` on `date`, if one is.
    func paused(in block: String, on date: Date) -> String? {
        guard let s = session, !s.over, s.block == block, Self.sameDay(s.date, date) else { return nil }
        return s.item
    }

    /// Begin or Continue (a reminder too): the part of the day opens at `startItem` in Today.
    func begin(_ block: String, on date: Date) {
        guard let item = startItem(block, on: date) else { return }
        open(item, block: block, on: date)
    }

    /// Opens an item as a step of its part of the day, from its row or the title menu. Another
    /// part of the day paused meanwhile is forgotten; its ticks stay.
    func open(_ item: String, block: String, on date: Date) {
        if session.map({ $0.over || $0.block != block || !Self.sameDay($0.date, date) }) ?? true {
            session = RuleSession(date: Calendar.current.startOfDay(for: date), block: block)
        }
        session?.item = item
        if todayPath.last != .session { todayPath = [.session] }
    }

    /// The step's item, as the day has it now.
    var sessionItem: RuleDayItem? {
        guard let s = session, let item = s.item else { return nil }
        return block(s.block, on: s.date)?.items.first { $0.id == item }
    }

    /// Done: ticks the item (on a day still to come, nothing) and opens the next. A plan's item
    /// checks off the passage shown; while the day has another, that one opens in its place.
    func done() {
        guard let s = session, let item = sessionItem else { return }
        let day = CivilDate(s.date).number
        if !Self.ahead(s.date) {
            if case .plan(let plan, let planDay) = item.open, let i = unreadPassage(plan, day: planDay) {
                setRead(plan, day: planDay, passage: i, true)
                places[Self.placeKey(item.id, on: s.date)] = nil
                if unreadPassage(plan, day: planDay) != nil { return }
            } else if !item.done {
                setDone(item, day: day, true)
            }
        }
        advance(after: item.id)
    }

    /// Skip: on to the next without a tick.
    func skip() {
        guard let item = session?.item else { return }
        session?.skipped.insert(item)
        advance(after: item)
    }

    /// The next item not done below `item`; after the last, the end page, or Today straight away
    /// for a part of the day with one item.
    private func advance(after item: String) {
        guard let s = session else { return }
        if let next = ruleNext(s.block, after: item, on: s.date) {
            session?.item = next
        } else if (block(s.block, on: s.date)?.items.count ?? 0) > 1 {
            session?.item = nil
        } else {
            finish()
        }
    }

    /// Back to Today from the end page (or after a part of the day's only item): it's over for
    /// today, and the Now card moves on.
    func finish() {
        guard let s = session, !s.over else { return }
        ended[CivilDate(s.date).number, default: []].insert(s.block)
        session?.over = true
        todayPath = []
    }

    /// The end page's last line: the part of the day the Now card offers next (none for another
    /// day than today), or tomorrow's first.
    func nextInRule(after block: String, on date: Date) -> String? {
        guard Self.sameDay(date, Date()) else { return nil }
        let today = ruleDay(date, now: Date(), ending: block)
        let at = { (b: RuleDayBlock) in RuleTime(b.time).map { "\(b.title) at \($0.label)" } }
        if let id = today.now?.block, let next = today.blocks.first(where: { $0.id == id }).flatMap(at) {
            return "Next in your rule: \(next)"
        }
        let tomorrow = Calendar.current.date(byAdding: .day, value: 1, to: date) ?? date
        let first = ruleDay(tomorrow).blocks.filter { RuleTime($0.time) != nil }
            .min { RuleTime($0.time)!.text < RuleTime($1.time)!.text }
        return first.flatMap(at).map { "Tomorrow: \($0)" }
    }

    /// Where a step's reading is kept in `places`: by day and item.
    static func placeKey(_ item: String, on date: Date) -> String { "\(CivilDate(date).number)-\(item)" }

    /// The first passage not read of a plan's day.
    func unreadPassage(_ plan: String, day: UInt32) -> Int? {
        planDay(plan, day)?.passages.firstIndex { !$0.read }
    }

    /// The rule changed (here or on the Mac): an open item that has gone gives way to the next.
    private func keepSessionInRule() {
        guard let s = session, !s.over else { return }
        guard let block = block(s.block, on: s.date) else {
            // The part of the day has gone from the rule.
            session = nil
            todayPath.removeAll { $0 == .session }
            return
        }
        guard let item = s.item, !block.items.contains(where: { $0.id == item }) else { return }
        // From the top: where the item stood is gone with it.
        session?.item = ruleNext(s.block, after: item, on: s.date)
    }

    /// Opens readings from Today's Readings, Psalter or plans, at `index`.
    func read(_ steps: [WalkStep], at index: Int, on date: Date) {
        guard steps.indices.contains(index) else { return }
        walk = ReadingWalk(date: Calendar.current.startOfDay(for: date), steps: steps, index: index)
        todayPath.append(.walk)
    }

    /// The walk's next reading, or another picked from its menu.
    func walk(to index: Int) {
        guard walk?.steps.indices.contains(index) == true else { return }
        walk?.index = index
    }

    /// Done on a plan's passage: checks it off, and opens the day's next not read, or goes back.
    func walkDone() {
        guard let w = walk else { return }
        if let plan = w.steps[w.index].plan, !Self.ahead(w.date) {
            setRead(plan.id, day: plan.day, passage: plan.passage, true)
        }
        let unread = { (i: Int) in
            w.steps[i].plan.map { self.planDay($0.id, $0.day)?.passages.dropFirst($0.passage).first?.read == false }
                ?? true
        }
        if let next = w.steps.indices.first(where: { $0 > w.index && unread($0) }) {
            walk?.index = next
        } else if todayPath.last == .walk {
            todayPath.removeLast()
        }
    }

    /// The book button: the reader at the verse in view, with a way back to the step.
    func openInReader(chapter: Int, verse: Int) {
        go(to: chapter, verse: verse, flash: true)
        sheet = nil
        returnToToday = true
    }

    /// The reader's Back to Today: Today again, at the step it left.
    func backToToday() {
        selection = nil
        sheet = .today
    }

    /// Today has closed. Unless a step is open in it, it opens next time at today, from the top.
    private func todayClosed() {
        if session?.item == nil { finish() }
        guard !todayPath.contains(where: { $0 == .session || $0 == .walk }) else { return }
        todayPath = []
        todayOffset = 0
    }

    /// The chapter (0–1364) a verse is in.
    func chapterIndex(of ref: VerseRef) -> Int {
        Int(bookList.first { $0.index == ref.book }?.firstChapter ?? 0) + Int(ref.chapter) - 1
    }

    static func sameDay(_ a: Date, _ b: Date) -> Bool { Calendar.current.isDate(a, inSameDayAs: b) }

    /// A day still to come, when nothing is checked off.
    static func ahead(_ date: Date) -> Bool { CivilDate(date).number > CivilDate(Date()).number }

    /// Sets the reminders to the saved rule, if they're on and allowed.
    func refreshReminders() async {
        let allowed = await reminders.allowed()
        remindersOn = settings.ruleReminders && allowed
        let today = Calendar.current.startOfDay(for: Date())
        let coming = remindersOn ? (0..<Reminders.days).compactMap { Calendar.current.date(byAdding: .day, value: $0, to: today) } : []
        await reminders.schedule(bible.rule(), days: coming.map { ($0, ruleDay($0)) }, on: remindersOn)
    }

    /// Turns reminders on (asking to send notifications the first time) or off; false when
    /// notifications aren't allowed.
    @discardableResult
    func setReminders(_ on: Bool) async -> Bool {
        if on, !(await reminders.allow()) { return false }
        settings.ruleReminders = on
        await refreshReminders()
        return true
    }

    /// Unix seconds of the local midnight that starts a day, as plans count days.
    static func midnight(_ date: Date) -> Int64 {
        Int64(Calendar.current.startOfDay(for: date).timeIntervalSince1970)
    }

    private func changing(_ change: () throws -> Void) {
        defer {
            ruleVersion += 1
            syncSoon()
        }
        do {
            try change()
        } catch {
            show("Couldn’t save: \(error.localizedDescription)")
        }
    }

    // MARK: Sync with the Mac

    /// The bookmark of the synced folder, in the study database; this device's own, never synced.
    private static let syncKey = "ios-sync"

    var syncOn: Bool { syncBookmark != nil }

    /// UI tests sync a folder of their own, at the path in BIBLE_SYNC_FOLDER (the test's own
    /// temporary folder, which it fills and clears), instead of one picked in iCloud Drive.
    static var testSyncFolder: URL? {
        guard ProcessInfo.processInfo.arguments.contains("--uitest"),
            let path = ProcessInfo.processInfo.environment["BIBLE_SYNC_FOLDER"]
        else { return nil }
        return URL(fileURLWithPath: path, isDirectory: true)
    }

    /// Turns sync on with a picked folder, and syncs.
    func startSync(folder url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            saveSyncBookmark(try FolderSync.bookmark(url))
        } catch {
            show("Couldn’t use that folder: \(error.localizedDescription)")
            return
        }
        forgetSyncs()
        syncFolder = FolderSync.name(url)
        ruleBeforeSync = bible.rule()
        syncNow()
    }

    func stopSync() {
        saveSyncBookmark(nil)
        forgetSyncs()
    }

    /// What the last folder's syncs said.
    private func forgetSyncs() {
        syncTask?.cancel()
        lastSynced = nil
        syncFolder = nil
        syncError = nil
        syncSkipped = []
        syncDevices = 0
        syncWaiting = 0
        syncRetries = 0
        ruleBeforeSync = nil
    }

    /// Syncs about three seconds after the last of a run of changes.
    private func syncSoon(after delay: Duration = .seconds(3)) {
        guard syncOn else { return }
        syncTask?.cancel()
        syncTask = Task {
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            syncNow()
        }
    }

    /// Syncs now, off the main thread; `done` once it (and any asked for meanwhile) is over.
    func syncNow(then done: (() -> Void)? = nil) {
        syncTask?.cancel()
        if let done { syncDone.append(done) }
        guard let bookmark = syncBookmark else { return finishSyncing() }
        guard !syncing else {
            syncAgain = true
            return
        }
        syncing = true
        let bible = bible
        Task {
            let outcome = await Task.detached(priority: .utility) { FolderSync.run(bible, bookmark: bookmark) }.value
            synced(outcome, bookmark: bookmark)
        }
    }

    /// Syncs as the app goes to the background, which iOS gives it time for.
    func syncInBackground() {
        guard syncOn else { return }
        if backgroundSync == .invalid {
            backgroundSync = UIApplication.shared.beginBackgroundTask(withName: "Sync") { [weak self] in
                MainActor.assumeIsolated { self?.endBackgroundSync() }
            }
        }
        syncNow { [weak self] in self?.endBackgroundSync() }
    }

    private func endBackgroundSync() {
        guard backgroundSync != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundSync)
        backgroundSync = .invalid
    }

    private func synced(_ outcome: FolderSync.Outcome, bookmark: Data) {
        syncing = false
        // Unless sync was turned off, or onto another folder, meanwhile.
        if syncBookmark == bookmark {
            if let fresh = outcome.bookmark { saveSyncBookmark(fresh) }
            syncFolder = outcome.folder ?? syncFolder
            syncError = outcome.error
            syncWaiting = outcome.downloading
            if outcome.downloading == 0 { syncRetries = 0 }
            if let report = outcome.report {
                lastSynced = Date()
                syncSkipped = report.skipped
                syncDevices = report.devices
                if report.applied > 0 {
                    // Everything shown is read again: the reader's marks, Today, the month, plans.
                    marksVersion += 1
                    ruleVersion += 1
                    Task { await refreshReminders() }
                }
                // The rule is one record: on the first sync with the Mac, the one changed last
                // is kept on both.
                if let before = ruleBeforeSync, report.devices > 0 {
                    ruleBeforeSync = nil
                    if bible.rule() != before { show("Your prayer rule now matches your Mac’s.") }
                }
            }
        }
        if syncAgain {
            syncAgain = false
            return syncNow()
        }
        // Files asked for from iCloud are usually there in a moment: looked for again a few
        // times, further apart each time (5 s, 10 s… about 5 minutes in all).
        if syncBookmark == bookmark, outcome.downloading > 0, syncRetries < 6 {
            syncSoon(after: .seconds(5 << syncRetries))
            syncRetries += 1
        }
        finishSyncing()
    }

    private func finishSyncing() {
        let done = syncDone
        syncDone = []
        for f in done { f() }
    }

    private func saveSyncBookmark(_ data: Data?) {
        syncBookmark = data
        do {
            try bible.setSetting(key: Self.syncKey, value: data?.base64EncodedString() ?? "")
        } catch {
            show("Couldn’t save: \(error.localizedDescription)")
        }
    }

    // MARK: Toasts

    func show(_ message: String) {
        toast = message
        toastTask?.cancel()
        toastTask = Task {
            try? await Task.sleep(for: .seconds(1.6))
            guard !Task.isCancelled else { return }
            toast = nil
        }
    }

    // MARK: Saving

    private struct Saved: Codable {
        var settings: ReaderSettings?
        var chapter: Int?
        var verse: Int?
    }

    private func scheduleSave() {
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(for: .seconds(1.5))
            guard !Task.isCancelled else { return }
            save()
        }
    }

    func save() {
        saveTask?.cancel()
        let saved = Saved(settings: settings, chapter: position.chapter, verse: position.verse)
        guard let data = try? JSONEncoder().encode(saved), let json = String(data: data, encoding: .utf8) else { return }
        try? bible.setSetting(key: "ios", value: json)
    }
}

enum Log {
    static let app = Logger(subsystem: "dev.bible-rs.ios", category: "app")
}

/// Time since the process started, for launch measurements.
enum LaunchClock {
    static let processStart: Date = {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var mib: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        guard sysctl(&mib, 4, &info, &size, nil, 0) == 0 else { return Date() }
        let t = info.kp_proc.p_starttime
        return Date(timeIntervalSince1970: Double(t.tv_sec) + Double(t.tv_usec) / 1_000_000)
    }()

    private static var logged = Set<String>()

    static func mark(_ event: String) {
        guard !logged.contains(event) else { return }
        logged.insert(event)
        let ms = Date().timeIntervalSince(processStart) * 1000
        Log.app.notice("\(event, privacy: .public): \(ms, format: .fixed(precision: 0), privacy: .public) ms after launch")
    }
}
