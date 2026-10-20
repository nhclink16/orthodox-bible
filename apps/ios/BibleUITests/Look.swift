import XCTest

// TEMPORARY: screenshots to look at. Not to be committed.
final class Look: XCTestCase {
    private var app: XCUIApplication!

    func testSepia() throws {
        continueAfterFailure = true
        app = XCUIApplication()
        app.launchArguments = ["--uitest"]
        app.launch()
        XCTAssertTrue(app.buttons["reference"].waitForExistence(timeout: 10))
        app.buttons["settings"].tap()
        app.buttons["Sepia theme"].tap()
        app.buttons["Done"].tap()
        sleep(1)
        app.buttons["today"].tap()
        sleep(1)
        app.buttons["month"].tap()
        sleep(1)
        snap("sepia-month")
        app.navigationBars.buttons.firstMatch.tap()
        for _ in 0..<5 { app.swipeUp(velocity: .slow) }
        sleep(1)
        app.buttons["plans"].tap()
        sleep(1)
        snap("sepia-plans")
        app.navigationBars.buttons.firstMatch.tap()
        sleep(1)
        app.buttons["edit-rule"].tap()
        sleep(1)
        app.buttons["item-morning-prayers"].tap()
        sleep(1)
        snap("sepia-item")
    }

    private func snap(_ name: String) {
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = "look-\(name)"
        shot.lifetime = .keepAlways
        add(shot)
    }
}
