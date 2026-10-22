import XCTest

/// The main flows, driven through the interface. Each test starts the app with no saved data
/// (`--uitest`), so it opens at John 1 in the Septuagint (whose New Testament is the KJV's).
final class BibleUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["--uitest"]
        app.launch()
        XCTAssertTrue(reference.waitForExistence(timeout: 10))
    }

    private var reference: XCUIElement { app.buttons["reference"] }

    private func verse(_ id: String) -> XCUIElement { app.staticTexts[id] }

    /// Waits for the toolbar to name a chapter.
    private func expectChapter(_ title: String, file: StaticString = #filePath, line: UInt = #line) {
        let shown = NSPredicate(format: "label BEGINSWITH %@", title + ".")
        let found = XCTNSPredicateExpectation(predicate: shown, object: reference)
        XCTAssertEqual(XCTWaiter.wait(for: [found], timeout: 5), .completed, "expected \(title), saw \(reference.label)", file: file, line: line)
    }

    func testOpensAtJohnOne() {
        expectChapter("John 1")
        XCTAssertTrue(verse("John 1:1").exists)
        XCTAssertTrue(verse("John 1:1").label.contains("In the beginning was the Word"))
    }

    func testScrollingReadsOnIntoTheNextChapters() {
        for _ in 0..<12 { app.swipeUp(velocity: .fast) }
        let title = reference.label
        XCTAssertFalse(title.hasPrefix("John 1."), "still at \(title)")
        XCTAssertTrue(title.hasPrefix("John"), "left John: \(title)")
        // And back, across the chapter boundary into Luke.
        for _ in 0..<24 { app.swipeDown(velocity: .fast) }
        let shown = NSPredicate(format: "label BEGINSWITH 'Luke'")
        let back = XCTNSPredicateExpectation(predicate: shown, object: reference)
        XCTAssertEqual(XCTWaiter.wait(for: [back], timeout: 5), .completed, "saw \(reference.label)")
    }

    func testJumpsToATypedReference() {
        reference.tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.tap()
        field.typeText("rom 8:28\n")
        expectChapter("Romans 8")
        XCTAssertTrue(verse("Romans 8:28").waitForExistence(timeout: 3))
        XCTAssertTrue(verse("Romans 8:28").isHittable)
    }

    func testRotatingKeepsYourPlace() {
        addTeardownBlock { XCUIDevice.shared.orientation = .portrait }
        reference.tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.tap()
        field.typeText("rom 8:28\n")
        expectChapter("Romans 8")
        XCTAssertTrue(verse("Romans 8:28").waitForExistence(timeout: 3))
        for orientation in [UIDeviceOrientation.landscapeLeft, .portrait] {
            XCUIDevice.shared.orientation = orientation
            sleep(1)
            expectChapter("Romans 8")
            XCTAssertTrue(verse("Romans 8:28").isHittable, "Romans 8:28 left the screen at orientation \(orientation.rawValue)")
        }
    }

    func testPicksABookAndChapter() {
        reference.tap()
        let romans = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Romans'")).firstMatch
        XCTAssertTrue(romans.waitForExistence(timeout: 3))
        romans.tap()
        let twelve = app.buttons["Romans 12"]
        XCTAssertTrue(twelve.waitForExistence(timeout: 3))
        twelve.tap()
        expectChapter("Romans 12")
        XCTAssertTrue(verse("Romans 12:1").label.contains("living sacrifice"))
    }

    func testPsalmsOpenFromAReference() {
        reference.tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.tap()
        // Psalms are numbered as the Septuagint numbers them: the Shepherd psalm is 22.
        field.typeText("ps 22\n")
        expectChapter("Psalm 22")
        XCTAssertTrue(verse("Psalm 22:1").waitForExistence(timeout: 3))
        XCTAssertTrue(verse("Psalm 22:1").label.contains("shepherd"))
    }

    func testSwitchesTranslation() {
        XCTAssertTrue(verse("John 1:5").label.contains("shineth in darkness"))
        app.buttons["translation"].tap()
        let web = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'WEB'")).firstMatch
        XCTAssertTrue(web.waitForExistence(timeout: 3))
        web.tap()
        let switched = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS 'shines in the darkness'"), object: verse("John 1:5"))
        XCTAssertEqual(XCTWaiter.wait(for: [switched], timeout: 5), .completed, verse("John 1:5").label)
        expectChapter("John 1")
    }

    func testReopensWhereYouLeftOff() {
        reference.tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.tap()
        field.typeText("rom 8:28\n")
        expectChapter("Romans 8")
        app.buttons["translation"].tap()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'BSB'")).firstMatch.tap()

        XCUIDevice.shared.press(.home)
        app.terminate()
        app.launchArguments = ["--uitest", "--keep-data"]
        app.launch()
        XCTAssertTrue(reference.waitForExistence(timeout: 10))
        expectChapter("Romans 8")
        XCTAssertTrue(verse("Romans 8:28").isHittable)
        XCTAssertTrue(app.buttons["translation"].label.contains("Berean"), app.buttons["translation"].label)
    }

    func testSearchFindsAndOpensAVerse() {
        app.buttons["search"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.typeText("love one another")
        let hit = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'John 13:34'")).firstMatch
        XCTAssertTrue(hit.waitForExistence(timeout: 5))
        hit.tap()
        expectChapter("John 13")
        XCTAssertTrue(verse("John 13:34").isHittable)
    }

    func testSearchLoadsMoreAsYouScroll() {
        app.buttons["search"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.typeText("lamb\n")
        XCTAssertTrue(row("Genesis 21:28").waitForExistence(timeout: 5))
        // The last of 175 hits, on the fourth page.
        let last = row("Revelation 22:3")
        for _ in 0..<40 where !last.exists { app.swipeUp(velocity: .fast) }
        XCTAssertTrue(last.exists)
    }

    func testMisspelledSearchOffersASuggestion() {
        app.buttons["search"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.typeText("bethsaida pool")
        let suggestion = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Search “bethesda pool”'")).firstMatch
        XCTAssertTrue(suggestion.waitForExistence(timeout: 5))
        suggestion.tap()
        XCTAssertTrue(
            app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'John 5:2'")).firstMatch.waitForExistence(timeout: 5))
    }

    func testHighlightBookmarkAndNoteShowInStudy() {
        let first = verse("John 1:1")
        first.tap()
        app.selectionAction("Highlight Green").tap()

        first.tap()
        app.selectionAction("Bookmark").tap()

        verse("John 1:3").tap()
        app.selectionAction("Note").tap()
        let note = app.textFields["Write a note"].exists ? app.textFields["Write a note"] : app.textViews.firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 3))
        note.typeText("All things, without exception.")
        app.buttons["Save"].tap()

        app.buttons["study"].tap()
        XCTAssertTrue(row("John 1:1").waitForExistence(timeout: 3), "bookmark listed")
        app.buttons["Highlights"].tap()
        XCTAssertTrue(row("John 1:1").waitForExistence(timeout: 3), "highlight listed")
        app.buttons["Notes"].tap()
        XCTAssertTrue(row("John 1:3").waitForExistence(timeout: 3), "note listed")
        XCTAssertTrue(row("John 1:3").label.contains("without exception"))
    }

    func testCopiesAVerseWithItsReference() {
        verse("John 1:4").tap()
        app.selectionAction("Copy").tap()
        XCTAssertTrue(app.staticTexts["Copied John 1:4"].waitForExistence(timeout: 3))
    }

    func testThemeAndTextSizeApplyAtOnce() {
        app.buttons["settings"].tap()
        app.buttons["Dark theme"].tap()
        let slider = app.sliders["Text size"]
        XCTAssertTrue(slider.waitForExistence(timeout: 3))
        slider.adjust(toNormalizedSliderPosition: 0.9)
        app.buttons["Done"].tap()
        XCTAssertTrue(verse("John 1:1").exists)
    }

    func testTodayShowsTheChurchDayAndOpensTheKathisma() {
        app.buttons["today"].tap()
        XCTAssertTrue(app.staticTexts["Fasting"].waitForExistence(timeout: 3) || app.staticTexts["FASTING"].exists)
        let kathisma = scrolledTo(app.buttons.matching(NSPredicate(format: "label CONTAINS 'Kathisma'")).firstMatch)
        kathisma.tap()
        // Its psalms open in Today, which stays open over the reader.
        XCTAssertTrue(passage.waitForExistence(timeout: 5))
        let psalm = app.staticTexts.matching(NSPredicate(format: "identifier BEGINSWITH 'Psalm '")).firstMatch
        XCTAssertTrue(psalm.waitForExistence(timeout: 5))
        XCTAssertFalse(reference.isHittable)
    }

    func testChecksOffTheRuleForTheDay() {
        app.buttons["today"].tap()
        // Under the day's readings.
        XCTAssertTrue(app.buttons["Previous day"].waitForExistence(timeout: 3))
        let prayers = scrolledTo(app.buttons["rule-morning-prayers"])
        XCTAssertEqual(prayers.value as? String, "Not done")
        prayers.tap()
        XCTAssertEqual(prayers.value as? String, "Done")

        // A prayer opens to be read, and Done checks it off; a part of the day with only it goes
        // straight back to Today.
        scrolledTo(app.buttons["open-meal-prayer"]).tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Our Father'")).firstMatch.waitForExistence(timeout: 3))
        scrolledTo(app.buttons["mark-done"]).tap()
        XCTAssertEqual(scrolledTo(app.buttons["rule-meal-prayer"]).value as? String, "Done")

        // The Jesus Prayer counts to its number, and then is done.
        scrolledTo(app.buttons["open-midday-jesus-prayer"]).tap()
        let count = app.buttons["count"]
        XCTAssertTrue(count.waitForExistence(timeout: 3))
        for _ in 0..<33 { count.tap() }
        XCTAssertTrue(count.label.hasPrefix("Glory to God"), count.label)
        back()
        XCTAssertEqual(scrolledTo(app.buttons["rule-midday-jesus-prayer"]).value as? String, "Done")

        // The check marks are that day's, and are kept.
        app.terminate()
        app.launchArguments = ["--uitest", "--keep-data"]
        app.launch()
        XCTAssertTrue(reference.waitForExistence(timeout: 10))
        app.buttons["today"].tap()
        XCTAssertTrue(app.buttons["Previous day"].waitForExistence(timeout: 3))
        app.buttons["Previous day"].tap()
        XCTAssertEqual(scrolledTo(app.buttons["rule-morning-prayers"]).value as? String, "Not done")
        // Today opens again at today, from the top.
        app.buttons["Done"].tap()
        app.buttons["today"].tap()
        XCTAssertTrue(app.buttons["Previous day"].waitForExistence(timeout: 3))
        XCTAssertEqual(scrolledTo(app.buttons["rule-morning-prayers"]).value as? String, "Done")
    }

    func testEditsTheRule() {
        app.buttons["today"].tap()
        scrolledTo(app.buttons["edit-rule"]).tap()

        // Rename Morning, which keeps its items and their check marks.
        let name = app.textFields["name-morning"]
        XCTAssertTrue(name.waitForExistence(timeout: 3))
        name.tap()
        name.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 12) + "Dawn\n")
        XCTAssertEqual(name.value as? String, "Dawn")

        // Add a psalm to Midday.
        scrolledTo(app.buttons["add-midday"]).tap()
        let psalm = app.buttons["A psalm"]
        XCTAssertTrue(psalm.waitForExistence(timeout: 3))
        psalm.tap()
        let title = app.textFields["item-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 3))
        XCTAssertEqual(title.value as? String, "Psalm 50")
        // Its number, on the number pad, which Done puts away.
        let number = app.textFields["psalm"]
        number.tap()
        number.typeText("118")
        app.toolbars.buttons["Done"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3))
        XCTAssertEqual(title.value as? String, "Psalm 118")
        app.navigationBars.buttons["Your rule"].tap()

        // Remove the examination of conscience.
        let reflection = scrolledTo(app.buttons["item-evening-reflection"])
        reflection.swipeLeft()
        app.buttons["Delete"].tap()
        XCTAssertFalse(app.buttons["item-evening-reflection"].exists)
        app.buttons["Save"].tap()

        // Back on Today, up to the renamed part of the day.
        XCTAssertTrue(app.buttons["edit-rule"].waitForExistence(timeout: 3))
        let dawn = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH[c] 'Dawn'")).firstMatch
        for _ in 0..<6 where !(dawn.exists && dawn.isHittable) { app.swipeDown(velocity: .slow) }
        XCTAssertTrue(dawn.isHittable)
        XCTAssertTrue(app.buttons["rule-morning-prayers"].exists)
        XCTAssertTrue(scrolledTo(app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Psalm 118'")).firstMatch).exists)
        scrolledTo(app.buttons["edit-rule"])
        XCTAssertFalse(app.buttons["rule-evening-reflection"].exists)

        // And back to the starter rule.
        app.buttons["edit-rule"].tap()
        scrolledTo(app.buttons["Reset to the starter rule"]).tap()
        app.buttons["Reset"].tap()
        app.buttons["Save"].tap()
        XCTAssertTrue(app.buttons["rule-evening-reflection"].waitForExistence(timeout: 3))
    }

    func testRemovesEveryPartOfTheDay() {
        app.buttons["today"].tap()
        scrolledTo(app.buttons["edit-rule"]).tap()
        for id in ["morning", "meals", "midday", "evening"] {
            scrolledTo(app.buttons["remove-\(id)"]).tap()
            app.buttons["Remove"].tap()
            XCTAssertFalse(app.buttons["remove-\(id)"].waitForExistence(timeout: 1), id)
        }
        app.buttons["Save"].tap()
        XCTAssertTrue(app.staticTexts["Nothing in your rule for this day."].waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["rule-morning-prayers"].exists)
    }

    func testRemindersTurnOnOnceNotificationsAreAllowed() {
        app.buttons["today"].tap()
        scrolledTo(app.buttons["edit-rule"]).tap()
        let reminders = app.switches["reminders"]
        XCTAssertTrue(reminders.waitForExistence(timeout: 3))
        // The switch itself sits at the end of the row.
        let flip = reminders.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5))
        if reminders.value as? String != "1" {
            flip.tap()
            // The first time, iOS asks.
            let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow"]
            if allow.waitForExistence(timeout: 5) { allow.tap() }
        }
        expect(reminders, value: "1")
        flip.tap()
        expect(reminders, value: "0")
        app.buttons["Cancel"].tap()
    }

    func testMonthCalendarMarksTheDayAndOpensAnother() {
        app.buttons["today"].tap()
        XCTAssertTrue(app.buttons["Previous day"].waitForExistence(timeout: 3))
        scrolledTo(app.buttons["rule-morning-prayers"]).tap()
        // Today again, from the top.
        app.buttons["Done"].tap()
        app.buttons["today"].tap()

        app.buttons["month"].tap()
        let today = Calendar.current.component(.day, from: Date())
        let cell = app.buttons["day-\(today)"]
        XCTAssertTrue(cell.waitForExistence(timeout: 3))
        XCTAssertTrue(cell.label.contains("Some of the rule kept"), cell.label)
        XCTAssertTrue(app.staticTexts["Strict fast"].exists, "the key")

        let other = today == 1 ? 2 : 1
        app.buttons["day-\(other)"].tap()
        var parts = Calendar.current.dateComponents([.year, .month], from: Date())
        parts.day = other
        let shown = Calendar.current.date(from: parts)!.formatted(.dateTime.weekday(.wide).day().month(.wide))
        XCTAssertTrue(app.staticTexts[shown].waitForExistence(timeout: 3), shown)
        XCTAssertTrue(app.buttons["Back to Today"].exists)
        XCTAssertEqual(scrolledTo(app.buttons["rule-morning-prayers"]).value as? String, "Not done")
    }

    func testStartsAReadingPlanAndOpensItsReading() {
        app.buttons["today"].tap()
        scrolledTo(app.buttons["plans"]).tap()
        scrolledTo(app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'The Psalter in 20 Days'")).firstMatch).tap()
        app.buttons["start-plan"].tap()
        let first = app.buttons["passage-psalter-20-1-0"]
        XCTAssertTrue(first.waitForExistence(timeout: 3))
        first.tap()
        XCTAssertEqual(first.value as? String, "Done")
        // A passage opens to be read in Today, and isn't checked off by opening it.
        app.buttons["read-psalter-20-1-1"].tap()
        XCTAssertTrue(verse("Psalm 2:1").waitForExistence(timeout: 5))
        back()
        let second = app.buttons["passage-psalter-20-1-1"]
        XCTAssertEqual(second.value as? String, "Not done")
        // Done at the end of it checks it off, and opens the day's next.
        app.buttons["read-psalter-20-1-1"].tap()
        scrolledTo(app.buttons["mark-done"]).tap()
        XCTAssertTrue(verse("Psalm 3:1").waitForExistence(timeout: 5))
        back()
        XCTAssertEqual(second.value as? String, "Done")
        XCTAssertEqual(app.buttons["passage-psalter-20-1-2"].value as? String, "Not done")
    }

    func testPraysThroughAPartOfTheDayInToday() {
        app.buttons["today"].tap()
        let begin = scrolledTo(app.buttons["block-begin-evening"])
        XCTAssertEqual(begin.label, "Begin evening")
        begin.tap()
        // Evening Prayers, from his own book.
        XCTAssertTrue(app.staticTexts["Pray from your prayer book. When you’ve finished, tap Done."].waitForExistence(timeout: 3))
        // Done on each item (Evening Prayers, the kathisma, the Fathers, the examination of
        // conscience, and St Ephrem's prayer in Lent), to the end page.
        for _ in 0..<6 where !app.buttons["end-today"].exists {
            doneInStep()
        }
        XCTAssertTrue(app.staticTexts["Glory to God for all things."].waitForExistence(timeout: 3))
        app.buttons["end-today"].tap()
        // Every evening item is checked off, and Today is still open.
        for id in ["evening-prayers", "evening-fathers", "evening-reflection"] {
            XCTAssertEqual(scrolledTo(app.buttons["rule-\(id)"]).value as? String, "Done", id)
        }
        XCTAssertTrue(app.navigationBars["Today"].exists)
        XCTAssertFalse(reference.isHittable)
    }

    func testSwipingTodayAwayKeepsTheStep() {
        app.buttons["today"].tap()
        scrolledTo(app.buttons["open-midday-psalm"]).tap()
        XCTAssertTrue(verse("Psalm 90:1").waitForExistence(timeout: 5))
        for _ in 0..<2 { app.swipeUp(velocity: .slow) }
        sleep(1)
        let before = topVerse("Psalm 90:")
        XCTAssertNotEqual(before, "Psalm 90:1")
        // Swiped away, the reader shows; the calendar opens the same step at the same verse.
        dismissSheet()
        XCTAssertTrue(reference.waitForExistence(timeout: 3) && reference.isHittable)
        app.buttons["today"].tap()
        XCTAssertTrue(app.navigationBars["Psalm 90"].waitForExistence(timeout: 3))
        sleep(1)
        XCTAssertEqual(topVerse("Psalm 90:"), before)
    }

    func testTheJesusPrayerKeepsItsCountWhenLeft() {
        app.buttons["today"].tap()
        scrolledTo(app.buttons["open-midday-jesus-prayer"]).tap()
        let count = app.buttons["count"]
        XCTAssertTrue(count.waitForExistence(timeout: 3))
        for _ in 0..<5 { count.tap() }
        back()
        scrolledTo(app.buttons["open-midday-jesus-prayer"]).tap()
        XCTAssertTrue(count.waitForExistence(timeout: 3))
        XCTAssertTrue(count.label.hasPrefix("5 of 33"), count.label)
        back()
        XCTAssertEqual(scrolledTo(app.buttons["rule-midday-jesus-prayer"]).value as? String, "Not done")
    }

    func testAReminderOpensItsPartOfTheDay() {
        app.terminate()
        app.launchArguments = ["--uitest", "--open-block", "evening"]
        app.launch()
        XCTAssertTrue(app.navigationBars["Evening Prayers"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Pray from your prayer book. When you’ve finished, tap Done."].exists)
    }

    func testOpenInReaderComesBackToTheStep() {
        app.buttons["today"].tap()
        scrolledTo(app.buttons["open-midday-psalm"]).tap()
        let book = app.buttons["session-reader"]
        XCTAssertTrue(book.waitForExistence(timeout: 5))
        book.tap()
        // The reader, at the psalm, with the way back.
        expectChapter("Psalm 90")
        let back = app.buttons["reader-back-today"]
        XCTAssertTrue(back.waitForExistence(timeout: 3))
        back.tap()
        XCTAssertTrue(app.navigationBars["Psalm 90"].waitForExistence(timeout: 3))
        XCTAssertTrue(verse("Psalm 90:1").exists)
        XCTAssertFalse(back.exists)
    }

    func testFathersAskToBeInstalledFirst() {
        verse("John 1:1").tap()
        app.selectionAction("Fathers").tap()
        XCTAssertTrue(app.staticTexts["Quotes from the Fathers aren't installed yet."].waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["Install from Files…"].exists)
    }

    func testTodayShowsTheDaysReadings() {
        openDay(2026, 10, 20)
        // The Liturgy's Epistle and Gospel first: St Artemius's Epistle with the weekday's Gospel.
        let epistle = scrolledTo(app.buttons["reading-0"])
        XCTAssertTrue(epistle.label.hasPrefix("Epistle, 2 Timothy 2:1–10"), epistle.label)
        XCTAssertTrue(epistle.label.contains("Great Martyr Artemius"), epistle.label)
        let gospel = app.buttons["reading-1"]
        XCTAssertTrue(gospel.label.hasPrefix("Gospel, Luke 9:23–27"), gospel.label)
        XCTAssertFalse(app.buttons["reading-2"].exists)
        // Read in Today, which stays open; nothing either side of the reading shows.
        gospel.tap()
        XCTAssertTrue(verse("Luke 9:23").waitForExistence(timeout: 5))
        XCTAssertTrue(verse("Luke 9:27").exists)
        XCTAssertFalse(verse("Luke 9:22").exists)
        XCTAssertFalse(verse("Luke 9:28").exists)
        back()
        app.buttons["Done"].tap()

        // A saint's whole day names the saint once, under the Epistle.
        openDay(2026, 10, 26)
        let demetrius = scrolledTo(app.buttons["reading-0"])
        XCTAssertTrue(demetrius.label.contains("Great Martyr Demetrius"), demetrius.label)
        XCTAssertFalse(app.buttons["reading-1"].label.contains("Demetrius"), app.buttons["reading-1"].label)
        app.buttons["Done"].tap()

        // An evening Liturgy says which it is.
        openDay(2026, 4, 9)
        let thursday = scrolledTo(app.buttons["reading-0"])
        XCTAssertTrue(thursday.label.hasPrefix("Epistle · Vesperal Liturgy of St Basil, 1 Corinthians 11:23–32"), thursday.label)
        XCTAssertTrue(app.buttons["reading-1"].label.hasPrefix("Gospel, Matthew 26:1–20; John 13:3–17"), app.buttons["reading-1"].label)
        app.buttons["Done"].tap()

        // Readings the parishes differ on say so once, under them.
        openDay(2026, 1, 26)
        scrolledTo(app.staticTexts["Some of the day’s readings vary from parish to parish. Check your parish calendar."])
        app.buttons["Done"].tap()

        // A Lenten weekday has no Liturgy of its own: what there is, at the Hours and Vespers.
        openDay(2026, 3, 4)
        let isaiah = scrolledTo(app.buttons["reading-0"])
        XCTAssertTrue(isaiah.label.hasPrefix("Isaiah 5:16–25, Sixth Hour"), isaiah.label)
        XCTAssertEqual(app.buttons["reading-1"].label, "Genesis 4:16–26, Presanctified Liturgy · Old Testament")
        XCTAssertEqual(app.buttons["reading-2"].label, "Proverbs 5:15–6:3, Old Testament", "the service named once")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Epistle'")).firstMatch.exists)
        app.buttons["Done"].tap()

        // Holy Friday's twenty-nine readings: the first few, then the rest when asked.
        openDay(2026, 4, 10)
        let first = scrolledTo(app.buttons["reading-0"])
        XCTAssertTrue(first.label.hasPrefix("John 13:31–18:1, Matins of the Twelve Passion Gospels · 1st"), first.label)
        let more = scrolledTo(app.buttons["more-readings"])
        XCTAssertEqual(more.label, "26 more readings")
        more.tap()
        let vespers = scrolledTo(app.buttons["reading-24"])
        XCTAssertTrue(vespers.label.hasPrefix("Exodus 33:11–23, Vespers of the Taking-Down from the Cross"), vespers.label)
        XCTAssertEqual(scrolledTo(app.buttons["reading-28"]).label.suffix(8), ", Gospel")
    }

    func testRuleReadingsOpenTheGospelAndTheFathers() {
        openDay(2026, 10, 20)
        let readings = scrolledTo(app.buttons["open-morning-reading"])
        XCTAssertTrue(readings.label.contains("2 Timothy 2:1–10 · Luke 9:23–27"), readings.label)
        readings.tap()
        // The Epistle, then the Gospel, a tap away.
        XCTAssertTrue(verse("2 Timothy 2:1").waitForExistence(timeout: 5))
        app.buttons["go-to-gospel"].tap()
        XCTAssertTrue(verse("Luke 9:23").waitForExistence(timeout: 3))
        expect(verse("Luke 9:23"), hittable: true)
        back()

        // The Fathers on the Gospel, offering to be installed first; Done still checks it off.
        scrolledTo(app.buttons["open-evening-fathers"]).tap()
        XCTAssertTrue(app.staticTexts["Quotes from the Fathers aren't installed yet."].waitForExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label ENDSWITH 'Luke 9:23–27'")).firstMatch.exists)
        scrolledTo(app.buttons["mark-done"]).tap()
        back()
        XCTAssertEqual(scrolledTo(app.buttons["rule-evening-fathers"]).value as? String, "Done")
        app.buttons["Done"].tap()

        // No Gospel on a Lenten weekday: the first reading, and the item says so.
        openDay(2026, 3, 4)
        let lent = scrolledTo(app.buttons["open-morning-reading"])
        XCTAssertTrue(lent.label.contains("No Gospel today · Isaiah 5:16–25 at the Sixth Hour"), lent.label)
        lent.tap()
        XCTAssertTrue(verse("Isaiah 5:16").waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["No Gospel today · Isaiah 5:16–25 at the Sixth Hour"].exists)
    }

    func testTheReadingsFollowTheArchdioceseChosen() {
        // The 2nd Sunday after Pentecost: the Greek Archdiocese reads the week's Epistle.
        openDay(2026, 6, 14)
        let greek = scrolledTo(app.buttons["reading-0"])
        XCTAssertTrue(greek.label.hasPrefix("Epistle, Romans 2:10–16"), greek.label)
        app.buttons["Done"].tap()
        // A tap while the sheet slides away is lost.
        XCTAssertTrue(greek.waitForNonExistence(timeout: 3))

        // Settings, beside the church calendar, with a line under it.
        app.buttons["settings"].tap()
        scrolledTo(app.staticTexts["Which archdiocese’s calendar of readings to follow. Your parish’s bulletin is the best check."])
        let readings = app.buttons["Readings"]
        XCTAssertEqual(readings.value as? String, "Greek Orthodox Archdiocese")
        readings.tap()
        XCTAssertTrue(app.buttons["Antiochian Archdiocese"].waitForExistence(timeout: 3))
        app.buttons["Antiochian Archdiocese"].tap()
        XCTAssertTrue(readings.waitForExistence(timeout: 3))
        XCTAssertEqual(readings.value as? String, "Antiochian Archdiocese")
        app.buttons["Done"].tap()
        XCTAssertTrue(readings.waitForNonExistence(timeout: 3))

        // The Antiochian, All Saints of Antioch's; the rule's Readings item follows.
        openDay(2026, 6, 14)
        let antiochian = scrolledTo(app.buttons["reading-0"])
        XCTAssertTrue(antiochian.label.hasPrefix("Epistle · All Saints of Antioch, Acts 11:19–30"), antiochian.label)
        XCTAssertTrue(app.buttons["reading-1"].label.hasPrefix("Gospel, Matthew 4:18–23"), app.buttons["reading-1"].label)
        let rule = scrolledTo(app.buttons["open-morning-reading"])
        XCTAssertTrue(rule.label.contains("Acts 11:19–30 · Matthew 4:18–23"), rule.label)
    }

    /// Never iCloud Drive: a folder in the test's own temporary folder, which the app reaches on
    /// the simulator; the first device's file waits beside it while the second turns sync on.
    private let syncFolder = FileManager.default.temporaryDirectory.appendingPathComponent("uitest-sync-\(UUID().uuidString)")
    private var syncAside: URL { syncFolder.appendingPathExtension("aside") }

    override func tearDown() {
        try? FileManager.default.removeItem(at: syncFolder)
        try? FileManager.default.removeItem(at: syncAside)
    }

    func testSyncsThroughAFolder() throws {
        app.terminate()
        app.launchEnvironment["BIBLE_SYNC_FOLDER"] = syncFolder.path
        app.launch()
        XCTAssertTrue(reference.waitForExistence(timeout: 10))
        // A note written before sync is turned on goes too.
        writeNote("John 1:3", "Written on the other device.")
        turnOnSync()
        XCTAssertTrue(
            app.staticTexts["Through iCloud Drive: notes, highlights, bookmarks, reading plans, your prayer rule and what you’ve ticked."].exists)
        XCTAssertTrue(lastSynced.label.hasSuffix(" · nothing from your Mac in this folder yet"), lastSynced.label)
        app.buttons["sync-now"].tap()
        XCTAssertTrue(lastSynced.waitForExistence(timeout: 5))
        app.buttons["Done"].tap()

        // A change goes a few seconds later by itself.
        writeNote("John 1:4", "Sent without Sync now.")
        sleep(5)

        // Off forgets the folder, and nothing done meanwhile is written there.
        app.buttons["settings"].tap()
        let sync = scrolledTo(app.switches["sync"])
        sync.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        expect(sync, value: "0")
        XCTAssertFalse(app.buttons["sync-now"].exists)
        app.buttons["Done"].tap()
        verse("John 1:1").tap()
        app.selectionAction("Highlight Green").tap()
        sleep(4)

        // Another device, with nothing of its own, turns sync on while the first one's file is
        // set aside, then gets it with its notes open.
        app.terminate()
        let fm = FileManager.default
        let files = try fm.contentsOfDirectory(atPath: syncFolder.path).filter { $0.hasSuffix(".json") }
        XCTAssertEqual(files.count, 1, "\(files)")
        try fm.moveItem(at: syncFolder, to: syncAside)
        app.launch()
        XCTAssertTrue(reference.waitForExistence(timeout: 10))
        turnOnSync()
        app.buttons["Done"].tap()
        app.buttons["study"].tap()
        app.buttons["Notes"].tap()
        XCTAssertFalse(row("John 1:3").waitForExistence(timeout: 1))
        try fm.moveItem(at: syncAside.appendingPathComponent(files[0]), to: syncFolder.appendingPathComponent(files[0]))
        // It syncs on coming back to the front, and the open list takes the changes in.
        XCUIDevice.shared.press(.home)
        app.activate()
        XCTAssertTrue(row("John 1:3").waitForExistence(timeout: 5))
        XCTAssertTrue(row("John 1:3").label.contains("Written on the other device."), row("John 1:3").label)
        XCTAssertTrue(row("John 1:4").exists, "sent without Sync now")
        app.buttons["Highlights"].tap()
        XCTAssertFalse(row("John 1:1").waitForExistence(timeout: 2), "highlighted while sync was off")
    }

    private var lastSynced: XCUIElement {
        app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Last synced'")).firstMatch
    }

    private func writeNote(_ reference: String, _ text: String) {
        verse(reference).tap()
        app.selectionAction("Note").tap()
        let note = app.textFields["Write a note"].exists ? app.textFields["Write a note"] : app.textViews.firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 3))
        note.typeText(text)
        app.buttons["Save"].tap()
        // The verse stays selected under the note.
        app.selectionAction("Clear selection").tap()
    }

    /// Turns on Settings' sync with the folder the test named, and waits for its first sync.
    private func turnOnSync() {
        app.buttons["settings"].tap()
        let sync = scrolledTo(app.switches["sync"])
        XCTAssertEqual(sync.value as? String, "0")
        // The switch itself sits at the end of the row.
        sync.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        expect(sync, value: "1")
        // In the footer, under Sync now.
        scrolledTo(lastSynced)
    }

    /// Opens Today on a date, picked from the month.
    private func openDay(_ year: Int, _ month: Int, _ day: Int) {
        app.buttons["today"].tap()
        let now = Calendar.current.dateComponents([.year, .month], from: Date())
        let steps = (year - now.year!) * 12 + month - now.month!
        app.buttons["month"].tap()
        let step = app.buttons[steps < 0 ? "Previous month" : "Next month"]
        XCTAssertTrue(step.waitForExistence(timeout: 3))
        for _ in 0..<abs(steps) { step.tap() }
        app.buttons["day-\(day)"].tap()
        let date = Calendar.current.date(from: DateComponents(year: year, month: month, day: day))!
        XCTAssertTrue(app.staticTexts[date.formatted(.dateTime.weekday(.wide).day().month(.wide))].waitForExistence(timeout: 3))
    }

    func testScrollingDoesNotHitch() {
        let options = XCTMeasureOptions()
        options.iterationCount = 5
        measure(metrics: [XCTOSSignpostMetric.scrollDecelerationMetric], options: options) {
            app.swipeUp(velocity: .fast)
        }
    }

    private var passage: XCUIElement { app.descendants(matching: .any)["passage"] }

    /// ‹ Today, or back a page in it.
    private func back() {
        app.navigationBars.buttons["BackButton"].firstMatch.tap()
        usleep(600_000)
    }

    /// Swipes the sheet away by its top bar.
    private func dismissSheet() {
        let top = app.navigationBars.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        top.press(forDuration: 0.05, thenDragTo: app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.98)))
        usleep(800_000)
    }

    /// Done at the end of the step's text, scrolled to however long the reading.
    private func doneInStep() {
        let done = app.buttons["mark-done"]
        XCTAssertTrue(done.waitForExistence(timeout: 3))
        for _ in 0..<40 where !done.isHittable { app.swipeUp(velocity: .fast) }
        usleep(600_000)
        done.tap()
        usleep(600_000)
    }

    /// The verse whose id starts so at the top of the page, just under the bar.
    private func topVerse(_ prefix: String) -> String? {
        let verses = app.staticTexts.matching(NSPredicate(format: "identifier BEGINSWITH %@", prefix))
        let line = app.navigationBars.firstMatch.frame.maxY + 14
        return verses.allElementsBoundByIndex.first { $0.frame.maxY > line && $0.isHittable }?.identifier
    }

    private func expect(_ element: XCUIElement, hittable: Bool, file: StaticString = #filePath, line: UInt = #line) {
        let shown = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == %@", NSNumber(value: hittable)), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [shown], timeout: 5), .completed, "\(element)", file: file, line: line)
    }

    private func expect(_ element: XCUIElement, value: String, file: StaticString = #filePath, line: UInt = #line) {
        let shown = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", value), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [shown], timeout: 5), .completed, "\(element.value ?? "nil")", file: file, line: line)
    }

    /// Scrolls the frontmost list up until `element` is on screen, clear of a bottom toolbar.
    @discardableResult
    private func scrolledTo(_ element: XCUIElement, file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
        let bottom = app.windows.firstMatch.frame.maxY - 100
        var swiped = false
        for _ in 0..<10 where !(element.exists && element.isHittable && element.frame.maxY < bottom) {
            app.swipeUp(velocity: .slow)
            swiped = true
        }
        // A tap while the list still glides only stops it.
        if swiped { usleep(600_000) }
        XCTAssertTrue(element.isHittable, "\(element) not found", file: file, line: line)
        return element
    }

    private func row(_ reference: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", reference)).firstMatch
    }
}

extension XCUIApplication {
    /// A button in the selection bar, once the bar has slid into place (0.28 s): a tap while it
    /// moves lands on the text behind it.
    func selectionAction(_ label: String) -> XCUIElement {
        let button = buttons[label]
        XCTAssertTrue(button.waitForExistence(timeout: 3), label)
        usleep(400_000)
        return button
    }
}
