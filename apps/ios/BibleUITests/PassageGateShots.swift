import XCTest

// GATE (temporary): the passage view beside the reader at the same verses. Skipped unless
// BIBLE_GATE=1.
final class PassageGateShots: XCTestCase {
    private var app: XCUIApplication!

    func testGate() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["BIBLE_GATE"] == "1")
        continueAfterFailure = false
        for size in ["UICTContentSizeCategoryL", "UICTContentSizeCategoryXXXL"] {
            let tag = size.hasSuffix("XXXL") ? "xxxl" : "l"
            for perLine in [false, true] {
                launch(size, keep: false)
                if perLine { toggle("One verse per line") }
                go("lk 9:44")
                snap("reader-luke-\(perLine ? "lines" : "para")-\(tag)")
                relaunch(size, gate: "morning-reading")
                snap("passage-luke-\(perLine ? "lines" : "para")-\(tag)")
            }
            for lxx in [true, false] {
                launch(size, keep: false)
                if !lxx { toggle("Septuagint psalm numbers") }
                go(lxx ? "ps 85" : "ps 86")
                snap("reader-psalm-\(lxx ? "lxx" : "hebrew")-\(tag)")
                relaunch(size, gate: "evening-kathisma")
                snap("passage-psalm-\(lxx ? "lxx" : "hebrew")-\(tag)")
            }
        }
    }

    private func launch(_ size: String, keep: Bool, gate: String? = nil) {
        app = XCUIApplication()
        app.launchArguments = ["--uitest", "-UIPreferredContentSizeCategoryName", size]
        if keep { app.launchArguments.append("--keep-data") }
        if let gate { app.launchArguments += ["--passage-gate", gate] }
        app.launch()
        XCTAssertTrue(app.buttons["reference"].waitForExistence(timeout: 10))
    }

    private func relaunch(_ size: String, gate: String) {
        XCUIDevice.shared.press(.home)
        app.terminate()
        launch(size, keep: true, gate: gate)
        XCTAssertTrue(app.descendants(matching: .any)["passage"].waitForExistence(timeout: 5))
    }

    private func toggle(_ name: String) {
        app.buttons["settings"].tap()
        let toggle = app.switches[name]
        for _ in 0..<8 where !toggle.isHittable { app.swipeUp(velocity: .slow) }
        toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        app.buttons["Done"].tap()
        sleep(1)
    }

    private func go(_ reference: String) {
        app.buttons["reference"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.tap()
        field.typeText(reference + "\n")
        sleep(2)
    }

    private func snap(_ name: String) {
        sleep(1)
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = "gate-\(name)"
        shot.lifetime = .keepAlways
        add(shot)
    }
}
