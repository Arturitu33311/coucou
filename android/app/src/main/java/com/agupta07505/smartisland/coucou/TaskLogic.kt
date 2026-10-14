/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 * The logic is a port of Coucou's tasks.rs (MIT), for the same behaviour on the phone and the PC.
 */

package com.agupta07505.smartisland.coucou

/** One to-do, with an optional reminder; [jinx] means it is also one of Jinx's pendientes. */
data class CoucouTask(
    val id: String,
    val title: String,
    val done: Boolean = false,
    /** Unix seconds the reminder is due (null: no reminder). */
    val remindAt: Long? = null,
    /** The reminder was announced: it is not announced again after a restart. */
    val notified: Boolean = false,
    val created: Long,
    val completedAt: Long? = null,
    /** Shared with Jinx: it is (or will be) one of her pendientes too. */
    val jinx: Boolean = false,
    /** Her id for it, once she has it. */
    val jinxId: String? = null,
    /** Who made it when it was not made here (her source: "hook", "teams", "portal"…): the list marks it. */
    val origin: String? = null,
    /**
     * When someone last changed it (unix seconds). Each device works alone; when they meet, the newest change
     * wins (see [TaskLogic.merge] and [TaskLogic.reconcile]). 0: never changed by a person (taken in from Jinx).
     */
    val updatedAt: Long = 0,
    /** Deleted: kept as a tombstone so the deletion travels to the other devices (the screens never see it). */
    val deleted: Boolean = false,
    /** Her status for it when we last agreed ("pendiente", "hecho", "descartado"): what a change on her side is told from. */
    val jinxStatus: String? = null,
    /** The [updatedAt] of the version she has (or that we took from her): a later one is not hers yet. */
    val jinxSynced: Long = 0,
)

/** One row of Jinx's pendientes, as far as the phone needs it. */
data class JinxRow(
    val id: String,
    val titulo: String,
    val due: String? = null,
    val categoria: String? = null,
    val source: String? = null,
    val status: String = "pendiente",
    /** When her store last changed the row (unix seconds), if her gateway says; without it a conflict is not told by time. */
    val updatedAt: Long? = null,
) {
    val open: Boolean get() = status == "pendiente"
}

/** What a sync has to do on her side once the local list is in step. */
data class SyncPlan(
    /** Local tasks (ids) to hand to her. */
    val toAdd: List<String> = emptyList(),
    /** (her id, "hecho" | "descartado" | "reabrir") to apply there. */
    val toResolve: List<Pair<String, String>> = emptyList(),
    /** The local list changed and must be saved. */
    val changed: Boolean = false,
)

object TaskLogic {
    /** A to-do list, not a project manager. */
    const val MAX_TASKS = 500
    const val MAX_TITLE = 200

    fun add(list: List<CoucouTask>, title: String, remindAt: Long?, now: Long, id: String, jinx: Boolean = false): List<CoucouTask> {
        val t = title.trim().take(MAX_TITLE)
        require(t.isNotEmpty()) { "A task needs a title" }
        require(list.size < MAX_TASKS) { "Too many tasks: finish or clear some first" }
        return list + CoucouTask(id = id, title = t, remindAt = remindAt, created = now, jinx = jinx, updatedAt = now)
    }

    /** How long a deletion is remembered, so a device that was away for a while still learns of it. */
    const val TOMBSTONE_SECS = 30L * 86_400

    /** A change stamped further ahead than this (a clock that is wrong) is taken as made now. */
    const val SKEW_SECS = 300L

    private fun nowSec() = System.currentTimeMillis() / 1000

    /** What the screens see: the list without the tombstones. */
    fun visible(list: List<CoucouTask>) = list.filter { !it.deleted }

    /** Forgets deletions older than [TOMBSTONE_SECS]. */
    fun purgeTombstones(list: List<CoucouTask>, now: Long) = list.filter { !(it.deleted && it.updatedAt < now - TOMBSTONE_SECS) }

    private fun change(list: List<CoucouTask>, id: String, f: (CoucouTask) -> CoucouTask): List<CoucouTask> {
        require(list.any { it.id == id }) { "That task is gone" }
        return list.map { if (it.id == id) f(it) else it }
    }

    fun setDone(list: List<CoucouTask>, id: String, done: Boolean, now: Long) =
        change(list, id) { it.copy(done = done, completedAt = if (done) now else null, updatedAt = now) }

    /** Moves the reminder to [until] and makes it announce again (and reopens the task). */
    fun snooze(list: List<CoucouTask>, id: String, until: Long, now: Long = nowSec()) =
        change(list, id) { it.copy(remindAt = until, notified = false, done = false, completedAt = null, updatedAt = now) }

    /**
     * New wording, and a new reminder if the sentence carried a time (otherwise the old one stays). It is
     * the same task for the other devices, stamped as a change so the newest one wins.
     */
    fun edit(list: List<CoucouTask>, id: String, title: String, remindAt: Long?, now: Long = nowSec()): List<CoucouTask> {
        val t = title.trim().take(MAX_TITLE)
        require(t.isNotEmpty()) { "A task needs a title" }
        return change(list, id) {
            val moved = remindAt != null && remindAt != it.remindAt
            it.copy(title = t, remindAt = remindAt ?: it.remindAt, notified = if (moved) false else it.notified, updatedAt = now)
        }
    }

    fun markNotified(list: List<CoucouTask>, id: String) = change(list, id) { it.copy(notified = true) }

    fun setJinx(list: List<CoucouTask>, id: String, on: Boolean) = change(list, id) { it.copy(jinx = on) }

    /** A deletion is a change like any other: stamped, and kept as a tombstone so the other devices learn of it. */
    fun delete(list: List<CoucouTask>, id: String, now: Long = nowSec()): List<CoucouTask> {
        require(list.any { it.id == id && !it.deleted }) { "That task is gone" }
        return list.map { if (it.id == id) it.copy(deleted = true, updatedAt = now) else it }
    }

    fun clearDone(list: List<CoucouTask>, now: Long = nowSec()) =
        list.map { if (it.done && !it.deleted) it.copy(deleted = true, updatedAt = now) else it }

    /** The next reminder still to announce (the earliest). */
    fun nextReminder(list: List<CoucouTask>): CoucouTask? =
        list.filter { !it.done && !it.deleted && !it.notified && it.remindAt != null }.minByOrNull { it.remindAt!! }

    /**
     * Brings the local tasks in step with Jinx's list; the newest change wins.
     *
     * Each side keeps working alone, so by the time they meet both may have changed. What each side's change is
     * told from is the status she had when we last agreed ([CoucouTask.jinxStatus]) and the stamp that was then
     * on both sides ([CoucouTask.jinxSynced]): she changed if her row differs from the first, we changed if ours
     * is stamped after the second. Only one changed: it is copied to the other. Both: the later stamp wins; her
     * row has a stamp only when her gateway reports one, and without it she wins, except for a deletion here.
     * What cannot be done now (she is unreachable) is asked again at the next sync, because the difference is
     * still there. The same rules as Coucou's tasks.rs.
     */
    fun reconcile(list: List<CoucouTask>, jinx: List<JinxRow>, now: Long): Pair<List<CoucouTask>, SyncPlan> {
        val toAdd = mutableListOf<String>()
        val toResolve = mutableListOf<Pair<String, String>>()
        var changed = false
        val out = list.map { t ->
            val jid = t.jinxId
            if (jid == null) {
                if (t.jinx && !t.done && !t.deleted) toAdd += t.id
                return@map t
            }
            val row = jinx.firstOrNull { it.id == jid }
            if (row == null) {
                // Not in her list any more: the link is dead.
                changed = true
                return@map t.copy(jinxId = null, jinxStatus = null)
            }
            if (!t.jinx) {
                if (row.open) toResolve += jid to "descartado"
                changed = true
                return@map t.copy(jinxId = null, jinxStatus = null)
            }
            val localOpen = !t.done && !t.deleted
            if (row.open == localOpen) {
                // Agreement: remember it, so the next change on either side is told from this.
                if (t.jinxStatus != row.status || t.jinxSynced != t.updatedAt) {
                    changed = true
                    return@map t.copy(jinxStatus = row.status, jinxSynced = t.updatedAt)
                }
                return@map t
            }
            val known = t.jinxStatus != null
            val sheChanged = !known || t.jinxStatus != row.status
            val weChanged = !known || t.updatedAt > t.jinxSynced
            val takeHers = when {
                !known -> !row.open // an old link with no memory: she is followed when she closed it, and told when we did
                sheChanged && !weChanged -> true
                !sheChanged -> false
                else -> row.updatedAt?.let { it > t.updatedAt } ?: !t.deleted
            }
            if (takeHers) {
                changed = true
                val stamp = row.updatedAt ?: maxOf(now, t.updatedAt)
                val base = if (row.open) t.copy(done = false, deleted = false, completedAt = null) else t.copy(done = true, completedAt = now)
                base.copy(updatedAt = stamp, jinxStatus = row.status, jinxSynced = stamp)
            } else {
                toResolve += jid to (if (t.deleted) "descartado" else if (t.done) "hecho" else "reabrir")
                t
            }
        }
        return out to SyncPlan(toAdd, toResolve, changed)
    }

    // ── Meeting another device ──────────────────────────────────────────────────

    private fun compareUtf8(a: String, b: String): Int {
        val x = a.toByteArray(Charsets.UTF_8)
        val y = b.toByteArray(Charsets.UTF_8)
        for (i in 0 until minOf(x.size, y.size)) {
            val c = (x[i].toInt() and 0xFF) - (y[i].toInt() and 0xFF)
            if (c != 0) return c
        }
        return x.size - y.size
    }

    /** The newest of two versions: the later change; on a tie a fixed order, so both devices choose the same. */
    private fun newer(a: CoucouTask, b: CoucouTask): Boolean {
        if (a.updatedAt != b.updatedAt) return a.updatedAt > b.updatedAt
        if (a.deleted != b.deleted) return a.deleted
        if (a.done != b.done) return a.done
        val c = compareUtf8(a.title, b.title)
        if (c != 0) return c > 0
        val x = a.remindAt
        val y = b.remindAt
        return when {
            x == y -> false
            x == null -> false
            y == null -> true
            else -> x > y
        }
    }

    /** The same id, or the same pendiente of Jinx's: one made here and taken in from her elsewhere is one task. */
    private fun sameTask(local: CoucouTask, remote: CoucouTask) =
        local.id == remote.id || (local.jinxId != null && local.jinxId == remote.jinxId)

    /**
     * Merges another device's list into ours: per task the newest change wins, a deletion is a change, and what
     * the other has and we do not is added. Meeting twice, or in either order, gives the same list.
     * Returns the new list and whether it changed.
     */
    fun merge(local: List<CoucouTask>, remote: List<CoucouTask>, now: Long): Pair<List<CoucouTask>, Boolean> {
        val out = local.toMutableList()
        var changed = false
        for (r0 in remote) {
            // A clock that is ahead must not win for ever.
            val r = r0.copy(updatedAt = minOf(r0.updatedAt, now + SKEW_SECS))
            val i = out.indexOfFirst { sameTask(it, r) }
            if (i < 0) {
                if (out.size < MAX_TASKS * 2 && !(r.deleted && r.updatedAt < now - TOMBSTONE_SECS)) {
                    // A reminder still to come rings here too; one already past does not ring late.
                    out += r.copy(notified = r.remindAt != null && r.remindAt <= now)
                    changed = true
                }
                continue
            }
            val l = out[i]
            if (newer(r, l)) {
                val notified = if (l.remindAt == r.remindAt) l.notified || r.notified else r.notified
                out[i] = r.copy(
                    id = l.id, jinxId = l.jinxId ?: r.jinxId, jinx = l.jinx || r.jinx, origin = l.origin ?: r.origin,
                    notified = notified, jinxStatus = l.jinxStatus, jinxSynced = l.jinxSynced,
                )
                changed = true
            } else if (l.jinxId == null && r.jinxId != null) {
                out[i] = l.copy(jinxId = r.jinxId)
                changed = true
            }
        }
        return out to changed
    }

    /** A task from another device is data, not trust: its text and its links are bounded before it is merged. */
    fun sanitizeRemote(t: CoucouTask): CoucouTask? {
        if (t.id.isEmpty() || t.id.length > 64 || t.id.any { it.isISOControl() }) return null
        val title = t.title.filter { !it.isISOControl() }.take(MAX_TITLE).trim()
        if (title.isEmpty() && !t.deleted) return null
        val hex = t.jinxId?.let { it.isNotEmpty() && it.length <= 16 && it.all { c -> c in '0'..'9' || c in 'a'..'f' || c in 'A'..'F' } } ?: true
        val origin = t.origin?.takeIf { it.length <= 24 && it.all { c -> c.isLetterOrDigit() || c == '-' } }
        // Bookkeeping about the other device's own link with her is not ours to take.
        return t.copy(title = title, jinxId = if (hex) t.jinxId else null, origin = origin, jinxStatus = null, jinxSynced = 0)
    }

    /** Her open pendientes that are not one of our shared tasks. */
    fun unlinkedOpen(list: List<CoucouTask>, jinx: List<JinxRow>): List<JinxRow> =
        jinx.filter { r -> r.open && list.none { it.jinxId == r.id } }

    /** The day number (from 1970-01-01) of a calendar date; the inverse of [civilDate]. */
    fun daysFromCivil(year: Long, month: Long, day: Long): Long {
        val y = if (month <= 2) year - 1 else year
        val era = Math.floorDiv(y, 400L)
        val yoe = Math.floorMod(y, 400L)
        val mp = if (month > 2) month - 3 else month + 9
        val doy = (153 * mp + 2) / 5 + day - 1
        val doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }

    private val DAY = Regex("""(\d{4})-(\d{2})-(\d{2})""")

    /** When a pendiente with only a day gets its reminder: 09:00 that day, in this phone's time zone. */
    fun dueReminder(due: String, offset: (Long) -> Long): Long? {
        val m = DAY.matchEntire(due) ?: return null
        val (y, mo, d) = m.destructured
        if (mo.toInt() !in 1..12 || d.toInt() !in 1..31) return null
        val nine = daysFromCivil(y.toLong(), mo.toLong(), d.toLong()) * 86_400L + 9 * 3600
        return nine - offset(nine)
    }

    /**
     * Her open pendientes that are not any task of ours become tasks of ours: the same kind, with the same
     * actions, marked with who made them. One with a day gets a reminder at 09:00 that day, unless that time
     * has passed (it then shows as late, without ringing for something old).
     */
    fun importUnlinked(list: List<CoucouTask>, jinx: List<JinxRow>, now: Long, offset: (Long) -> Long): Pair<List<CoucouTask>, Int> {
        var out = list
        var taken = 0
        for (r in jinx.filter { it.open }) {
            if (out.size >= MAX_TASKS || out.any { it.jinxId == r.id }) continue
            val title = r.titulo.trim().take(MAX_TITLE)
            if (title.isEmpty()) continue
            val remindAt = r.due?.let { dueReminder(it, offset) }
            out = out + CoucouTask(
                id = "j-${r.id}", title = title, remindAt = remindAt,
                // Only a reminder still to come may ring.
                notified = remindAt != null && remindAt <= now,
                created = now, jinx = true, jinxId = r.id,
                origin = r.source?.takeIf { it != "coucou" },
                // Not a person's change: what a person does with it later beats this.
                updatedAt = r.updatedAt ?: 0, jinxStatus = r.status, jinxSynced = r.updatedAt ?: 0,
            )
            taken++
        }
        return out to taken
    }

    /** "2026-10-06T19:30:42.27+00:00" (or with Z, or without a zone: UTC) to unix seconds; null if it is not a date. */
    fun isoToUnix(s: String): Long? {
        val m = Regex("""(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(Z|[+-]\d{2}:?\d{2})?""").matchEntire(s.trim()) ?: return null
        val (y, mo, d, h, mi, se, zone) = m.destructured
        var t = daysFromCivil(y.toLong(), mo.toLong(), d.toLong()) * 86_400L + h.toLong() * 3600 + mi.toLong() * 60 + se.toLong()
        if (zone.isNotEmpty() && zone != "Z") {
            val sign = if (zone[0] == '+') 1 else -1
            val digits = zone.drop(1).replace(":", "")
            t -= sign * (digits.take(2).toLong() * 3600 + digits.drop(2).toLong() * 60)
        }
        return t
    }

    /** "2026-10-06" for a day number counted from 1970-01-01 (proleptic Gregorian). */
    fun civilDate(days: Long): String {
        val z = days + 719_468
        val era = Math.floorDiv(z, 146_097L)
        val doe = Math.floorMod(z, 146_097L)
        val yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365
        val y = yoe + era * 400
        val doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        val mp = (5 * doy + 2) / 153
        val d = doy - (153 * mp + 2) / 5 + 1
        val m = if (mp < 10) mp + 3 else mp - 9
        val year = if (m <= 2) y + 1 else y
        return "%04d-%02d-%02d".format(year, m, d)
    }
}
