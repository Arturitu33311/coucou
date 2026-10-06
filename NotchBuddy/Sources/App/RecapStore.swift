import Foundation

// MARK: - Persisted data models

struct RecapTurn: Codable {
    var pillId: String
    var project: String
    var start: Date
    var end: Date
    var filesChanged: Int
    var linesAdded: Int
    var linesRemoved: Int
    var commandsRun: Int
    var questions: Int
}

struct RecapDecision: Codable {
    var pillId: String
    var date: Date
    var decision: String     // "allow", "always", "deny", "ask"
}

private struct RecapData: Codable {
    var turns: [RecapTurn] = []
    var decisions: [RecapDecision] = []
    var schemaVersion: Int = 1
}

// MARK: - Weekly summary

struct WeeklySummary {
    var weekStart: Date
    var weekEnd: Date
    var totalMinutes: Int
    var sessionCount: Int
    var filesChanged: Int
    var linesAdded: Int
    var linesRemoved: Int
    var commandsRun: Int
    var questionsAnswered: Int
    var permissionsAllowed: Int
    var permissionsDenied: Int
    var topAgent: String?    // pill display name
    var topProject: String?
    var busiestDay: String?
    var longestSessionMinutes: Int
}

// MARK: - In-progress turn draft

private struct TurnDraft {
    var pillId: String
    var project: String
    var start: Date
    var changedPaths: Set<String> = []
    var linesAdded: Int = 0
    var linesRemoved: Int = 0
    var commandsRun: Int = 0
    var questions: Int = 0
}

// MARK: - Store

@MainActor
final class RecapStore {
    static let shared = RecapStore()

    private var data = RecapData()
    private var drafts: [String: TurnDraft] = [:]

    private static let storageURL: URL = {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("NotchBuddy/recap.json")
    }()

    private init() { load() }

    // MARK: - Event recording

    func userPromptSubmit(pillId: String, project: String) {
        guard isEnabled else { return }
        if drafts[pillId] == nil {
            drafts[pillId] = TurnDraft(pillId: pillId, project: project, start: .now)
        }
    }

    func preToolUse(pillId: String, tool: String) {
        guard isEnabled, var draft = drafts[pillId] else { return }
        switch tool {
        case "Bash", "Execute", "mcp__ide__executeCode":
            draft.commandsRun += 1
        case "AskUserQuestion":
            draft.questions += 1
        default:
            break
        }
        drafts[pillId] = draft
    }

    /// Called after a file diff is computed in PostToolUse.
    func recordFileDiff(pillId: String, path: String, added: Int, removed: Int) {
        guard isEnabled, var draft = drafts[pillId] else { return }
        draft.changedPaths.insert(path)
        draft.linesAdded += added
        draft.linesRemoved += removed
        drafts[pillId] = draft
    }

    func stop(pillId: String) {
        guard isEnabled, let draft = drafts.removeValue(forKey: pillId) else { return }
        let turn = RecapTurn(
            pillId: pillId,
            project: draft.project,
            start: draft.start,
            end: .now,
            filesChanged: draft.changedPaths.count,
            linesAdded: draft.linesAdded,
            linesRemoved: draft.linesRemoved,
            commandsRun: draft.commandsRun,
            questions: draft.questions
        )
        data.turns.append(turn)
        prune()
        save()
    }

    func sessionEnd(pillId: String) {
        drafts.removeValue(forKey: pillId)
    }

    func recordDecision(pillId: String, decision: String) {
        guard isEnabled else { return }
        data.decisions.append(RecapDecision(pillId: pillId, date: .now, decision: decision))
        prune()
        save()
    }

    // MARK: - Query

    /// Returns a summary for the last completed week (Mon–Sun).
    /// Pass a custom `weekStart` (Monday 00:00 local) to query a different week.
    func weeklySummary(for weekStart: Date? = nil) -> WeeklySummary? {
        let cal = Calendar.current
        let start: Date
        if let ws = weekStart {
            start = ws
        } else {
            // Previous Monday 00:00 local
            var comps = cal.dateComponents([.yearForWeekOfYear, .weekOfYear], from: Date())
            comps.weekday = 2
            let thisMonday = cal.date(from: comps)!
            start = cal.date(byAdding: .weekOfYear, value: -1, to: thisMonday)!
        }
        let end = cal.date(byAdding: .day, value: 7, to: start)!

        let turns = data.turns.filter { $0.start >= start && $0.start < end }
        let decisions = data.decisions.filter { $0.date >= start && $0.date < end }
        guard !turns.isEmpty else { return nil }

        let totalSecs = turns.reduce(0.0) { $0 + $1.end.timeIntervalSince($1.start) }
        let allowed = decisions.filter { $0.decision == "allow" || $0.decision == "always" }.count
        let denied  = decisions.filter { $0.decision == "deny" }.count

        // Top agent by session count
        var countByAgent: [String: Int] = [:]
        for t in turns { countByAgent[t.pillId, default: 0] += 1 }
        let topAgentId = countByAgent.max { $0.value < $1.value }?.key
        let topAgent = topAgentId.flatMap { PillCatalog.definition(for: $0)?.name } ?? topAgentId

        // Top project by session count
        var countByProject: [String: Int] = [:]
        for t in turns where !t.project.isEmpty { countByProject[t.project, default: 0] += 1 }
        let topProject = countByProject.max { $0.value < $1.value }?.key

        // Busiest day
        var countByWeekday: [Int: Int] = [:]
        for t in turns { countByWeekday[cal.component(.weekday, from: t.start), default: 0] += 1 }
        let dayNames = [1: "Sunday", 2: "Monday", 3: "Tuesday", 4: "Wednesday",
                        5: "Thursday", 6: "Friday", 7: "Saturday"]
        let busiestDay = countByWeekday.max { $0.value < $1.value }.flatMap { dayNames[$0.key] }

        let longestSecs = turns.map { $0.end.timeIntervalSince($0.start) }.max() ?? 0

        return WeeklySummary(
            weekStart: start,
            weekEnd: cal.date(byAdding: .second, value: -1, to: end)!,
            totalMinutes:          Int(totalSecs / 60),
            sessionCount:          turns.count,
            filesChanged:          turns.reduce(0) { $0 + $1.filesChanged },
            linesAdded:            turns.reduce(0) { $0 + $1.linesAdded },
            linesRemoved:          turns.reduce(0) { $0 + $1.linesRemoved },
            commandsRun:           turns.reduce(0) { $0 + $1.commandsRun },
            questionsAnswered:     turns.reduce(0) { $0 + $1.questions },
            permissionsAllowed:    allowed,
            permissionsDenied:     denied,
            topAgent:              topAgent,
            topProject:            topProject,
            busiestDay:            busiestDay,
            longestSessionMinutes: Int(longestSecs / 60)
        )
    }

    // MARK: - Persistence

    var isEnabled: Bool {
        UserDefaults.standard.object(forKey: "recapEnabled") as? Bool ?? true
    }

    func clearHistory() {
        data = RecapData()
        drafts = [:]
        try? FileManager.default.removeItem(at: Self.storageURL)
    }

    private func load() {
        guard let raw = try? Data(contentsOf: Self.storageURL),
              let decoded = try? JSONDecoder().decode(RecapData.self, from: raw) else { return }
        data = decoded
        prune()
    }

    private func save() {
        let dir = Self.storageURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        if let encoded = try? JSONEncoder().encode(data) {
            try? encoded.write(to: Self.storageURL, options: .atomic)
        }
    }

    private func prune() {
        guard let cutoff = Calendar.current.date(byAdding: .weekOfYear, value: -12, to: .now) else { return }
        data.turns     = data.turns.filter     { $0.start >= cutoff }
        data.decisions = data.decisions.filter { $0.date  >= cutoff }
    }
}
