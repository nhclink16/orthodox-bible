import Foundation
import UserNotifications

/// The prayer rule's reminders: a notification every day at each part of the day that has a
/// time ("rule-<block id>"), set again whenever the rule or the switch changes. Tapping one opens
/// Today.
@MainActor
final class Reminders: NSObject, UNUserNotificationCenterDelegate {
    private let center = UNUserNotificationCenter.current()
    var opened: (() -> Void)?

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

    /// Replaces the rule's reminders with one per timed part of the day, or none.
    func schedule(_ rule: Rule, on: Bool) async {
        let old = await center.pendingNotificationRequests().map(\.identifier).filter { $0.hasPrefix("rule-") }
        center.removePendingNotificationRequests(withIdentifiers: old)
        guard on else { return }
        for block in rule.blocks {
            guard let time = RuleTime(block.time) else { continue }
            let content = UNMutableNotificationContent()
            content.title = "\(block.title) — \(time.label)"
            content.body = block.items.filter { $0.season == .always }.map(\.title).joined(separator: " · ")
            content.sound = .default
            let trigger = UNCalendarNotificationTrigger(dateMatching: time.components, repeats: true)
            try? await center.add(UNNotificationRequest(identifier: "rule-\(block.id)", content: content, trigger: trigger))
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        await MainActor.run { opened?() }
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
