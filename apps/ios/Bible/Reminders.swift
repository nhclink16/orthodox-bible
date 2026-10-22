import Foundation
import UserNotifications

/// The prayer rule's reminders: a notification at the time of each part of the day, set again
/// whenever the app comes to the front or the rule or the switch changes. Tapping one opens
/// Today at that part of the day's first item not done.
@MainActor
final class Reminders: NSObject, UNUserNotificationCenterDelegate {
    private let center = UNUserNotificationCenter.current()
    /// A reminder was tapped: its part of the day's id, if it names one.
    var opened: ((String?) -> Void)?

    /// How many days ahead a part of the day kept only in or out of Lent is reminded of.
    static let days = 14

    override init() {
        super.init()
        center.delegate = self
    }

    /// Whether notifications are allowed, without asking.
    func allowed() async -> Bool {
        let status = await center.notificationSettings().authorizationStatus
        return [.authorized, .provisional, .ephemeral].contains(status)
    }

    /// Whether notifications are allowed, asking the first time.
    func allow() async -> Bool {
        if await center.notificationSettings().authorizationStatus == .notDetermined {
            return (try? await center.requestAuthorization(options: [.alert, .sound])) ?? false
        }
        return await allowed()
    }

    /// Replaces the rule's reminders. A part of the day with something in it every day is
    /// reminded of daily ("rule-<block id>"), naming what's always in it; one kept only in or out
    /// of Lent, on each of `days` (the coming days and their rule) that it's kept, naming what's
    /// in it then.
    func schedule(_ rule: Rule, days: [(Date, RuleDay)], on: Bool) async {
        let old = await center.pendingNotificationRequests().map(\.identifier).filter { $0.hasPrefix("rule-") }
        center.removePendingNotificationRequests(withIdentifiers: old)
        guard on else { return }
        for block in rule.blocks {
            guard let time = RuleTime(block.time) else { continue }
            let seasons = Set(block.items.map(\.season))
            if seasons.contains(.always) || seasons.isSuperset(of: [.greatLent, .outsideLent]) {
                let always = block.items.filter { $0.season == .always }.map(\.title)
                await add(
                    "rule-\(block.id)", block: block.id, "\(block.title) — \(time.label)", always, at: time.components,
                    repeats: true)
                continue
            }
            for (date, day) in days {
                guard let items = day.blocks.first(where: { $0.id == block.id })?.items else { continue }
                var when = Calendar.current.dateComponents([.year, .month, .day], from: date)
                (when.hour, when.minute) = (time.hour, time.minute)
                guard let at = Calendar.current.date(from: when), at > Date() else { continue }
                let id = "rule-\(block.id)-\(CivilDate(date).number)"
                await add(id, block: block.id, "\(block.title) — \(time.label)", items.map(\.title), at: when, repeats: false)
            }
        }
    }

    private func add(
        _ id: String, block: String, _ title: String, _ items: [String], at when: DateComponents, repeats: Bool
    ) async {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = items.joined(separator: " · ")
        content.sound = .default
        content.userInfo = ["block": block]
        let trigger = UNCalendarNotificationTrigger(dateMatching: when, repeats: repeats)
        try? await center.add(UNNotificationRequest(identifier: id, content: content, trigger: trigger))
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let block = response.notification.request.content.userInfo["block"] as? String
        await MainActor.run { opened?(block) }
    }
}

/// A part of the day's time, kept as "HH:MM" in the rule.
struct RuleTime: Equatable {
    var hour: Int
    var minute: Int

    init?(_ text: String?) {
        let parts = (text ?? "").split(separator: ":").compactMap { Int($0) }
        guard parts.count == 2, (0..<24).contains(parts[0]), (0..<60).contains(parts[1]) else { return nil }
        hour = parts[0]
        minute = parts[1]
    }

    init(_ date: Date) {
        let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
        hour = parts.hour ?? 0
        minute = parts.minute ?? 0
    }

    var components: DateComponents { DateComponents(hour: hour, minute: minute) }
    var text: String { String(format: "%02d:%02d", hour, minute) }
    var date: Date { Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: Date()) ?? Date() }
    /// As the phone shows times: "8:30 PM" or "20:30".
    var label: String { date.formatted(date: .omitted, time: .shortened) }
}
