import XCTest

// TEMPORARY: screenshots to look at. Not to be committed.
final class Look: XCTestCase {
    private var app: XCUIApplication!
    private let dir = "/private/tmp/claude-501/-Users-mba-Library-Application-Support-Claude-scratch-workspaces-d1316a05-ce42-4eec-a723-d723b573d9a4-259a079e-1077-48e6-908f-6ec7d23e83a5-scratch-2026-10-20-5c13c6/65ee66da-ac69-4a13-879c-7b8936820c9e/scratchpad/look"

    private func launch(large: Bool = false) {
        continueAfterFailure = true
        app = XCUIApplication()
        app.launchArguments = ["--uitest"]
        if large { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch()
        XCTAssertTrue(app.buttons["reference"].waitForExistence(timeout: 10))
    }

    func testCounterAndMonthLarge() {
        launch(large: true)
        app.buttons["today"].tap()
        sleep(1)
        up(to: app.buttons["open-midday-jesus-prayer"]).tap()
        sleep(1)
        snap("large-counter")
        let count = app.buttons["count"]
        for _ in 0..<3 { count.tap() }
        snap("large-counter-3")
        app.swipeUp()
        snap("large-counter-scrolled")
        app.navigationBars.buttons.firstMatch.tap()
        sleep(1)
        app.buttons["month"].tap()
        sleep(1)
        snap("large-month")
        app.swipeUp(velocity: .slow)
        sleep(1)
        snap("large-month-key")
    }

    func testSundayAndPlanInTheRule() {
        launch()
        app.buttons["today"].tap()
        sleep(1)
        app.buttons["month"].tap()
        sleep(1)
        app.buttons["day-25"].tap()
        sleep(1)
        app.buttons["month"].tap()
        sleep(1)
        app.buttons["day-20"].tap()
        sleep(1)

        // A plan in the rule, started from Today.
        up(to: app.buttons["edit-rule"]).tap()
        sleep(1)
        up(to: app.buttons["add-evening"]).tap()
        app.buttons["A reading plan"].tap()
        sleep(1)
        snap("editor-plan-item")
        app.navigationBars.buttons["Your rule"].tap()
        app.buttons["Save"].tap()
        sleep(1)
        let item = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'open-item-'")).firstMatch
        up(to: item).tap()
        sleep(1)
        app.buttons["Start today"].tap()
        sleep(1)
        snap("plan-in-rule-started")
        let box = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'rule-item-'")).firstMatch
        box.tap()
        sleep(1)
        snap("plan-in-rule-ticked")
        app.swipeUp(velocity: .slow)
        sleep(1)
        snap("plan-in-rule-ticked-2")
    }

    func testPsalmFieldAndEmptyRule() {
        launch()
        app.buttons["today"].tap()
        sleep(1)
        up(to: app.buttons["edit-rule"]).tap()
        sleep(1)
        up(to: app.buttons["add-midday"]).tap()
        app.buttons["A psalm"].tap()
        sleep(1)
        app.textFields["psalm"].tap()
        sleep(1)
        snap("psalm-typing")
        app.textFields["psalm"].typeText("200")
        snap("psalm-200")
        app.toolbars.buttons["Done"].tap()
        sleep(1)
        snap("psalm-done")
        app.navigationBars.buttons["Your rule"].tap()
        app.buttons["Save"].tap()
        sleep(1)
        app.buttons["edit-rule"].tap()
        sleep(1)
        for id in ["morning", "meals", "midday", "evening"] {
            up(to: app.buttons["remove-\(id)"]).tap()
            app.buttons["Remove"].tap()
            sleep(1)
        }
        snap("editor-empty")
        app.buttons["Save"].tap()
        sleep(1)
        up(to: app.buttons["edit-rule"])
        snap("today-empty")
    }

    func skipRefusedNotifications() {
        launch()
        app.buttons["today"].tap()
        sleep(1)
        up(to: app.buttons["edit-rule"]).tap()
        let reminders = app.switches["reminders"]
        XCTAssertTrue(reminders.waitForExistence(timeout: 3))
        reminders.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        let deny = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Don’t Allow"]
        if deny.waitForExistence(timeout: 5) { deny.tap() }
        sleep(2)
        snap("reminders-refused")
        print("LOOK switch value: \(reminders.value ?? "nil")")
    }

    @discardableResult
    private func up(to element: XCUIElement) -> XCUIElement {
        for _ in 0..<14 where !(element.exists && element.isHittable && element.frame.maxY < app.windows.firstMatch.frame.maxY - 100) {
            app.swipeUp(velocity: .slow)
        }
        usleep(600_000)
        return element
    }

    private func snap(_ name: String) {
        let shot = XCUIScreen.main.screenshot()
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        try? shot.pngRepresentation.write(to: URL(fileURLWithPath: "\(dir)/\(name).png"))
        let a = XCTAttachment(screenshot: shot)
        a.name = "look-\(name)"
        a.lifetime = .keepAlways
        add(a)
    }
}
