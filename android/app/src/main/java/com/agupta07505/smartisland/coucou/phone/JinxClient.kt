/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.phone

import android.content.Context
import com.agupta07505.smartisland.coucou.CoucouTask
import com.agupta07505.smartisland.coucou.DismissQueue
import com.agupta07505.smartisland.coucou.JinxRow
import com.agupta07505.smartisland.coucou.TaskLogic
import com.agupta07505.smartisland.coucou.TaskStore
import org.json.JSONObject

class JinxException(message: String) : Exception(message)

/** What can be asked of Jinx's list. (An interface, so the sync can be tried without a network.) */
interface PendientesApi {
    fun list(): List<JinxRow>

    /** Hands a task over (asking twice makes one pendiente). Returns her id for it. */
    fun add(task: CoucouTask): String

    /** "hecho" | "descartado" | "reabrir", by her id. */
    fun resolve(id: String, action: String)
}

/** The requests the server's gateway (scripts/phone/pendientes-gateway.py) understands. */
object JinxProtocol {
    fun list(): String = JSONObject().put("op", "list").toString()

    fun resolve(id: String, action: String): String =
        JSONObject().put("op", "resolve").put("id", id).put("accion", action).toString()

    /** Her due date is a plain day, and the time of day goes in the note, as the laptop's Coucou does. */
    fun dueAndNote(remindAt: Long?, offsetSec: Long): Pair<String?, String> {
        if (remindAt == null) return null to "Coucou"
        val local = remindAt + offsetSec
        val secs = Math.floorMod(local, 86_400L)
        return TaskLogic.civilDate(Math.floorDiv(local, 86_400L)) to "Coucou · %02d:%02d".format(secs / 3600, secs % 3600 / 60)
    }

    fun add(task: CoucouTask, offsetSec: Long): String {
        val (due, nota) = dueAndNote(task.remindAt, offsetSec)
        return JSONObject()
            .put("op", "add")
            .put("titulo", task.title)
            .put("due", due ?: JSONObject.NULL)
            .put("nota", nota)
            .put("source_id", task.id)
            .toString()
    }

    private fun checked(json: String): JSONObject {
        val o = runCatching { JSONObject(json.trim().lines().lastOrNull { it.isNotBlank() } ?: "") }
            .getOrElse { throw JinxException("Jinx's answer was not understood") }
        if (!o.optBoolean("ok")) throw JinxException(o.optString("error", "refused"))
        return o
    }

    fun parseList(json: String): List<JinxRow> {
        val items = checked(json).getJSONArray("items")
        return (0 until items.length()).mapNotNull { i ->
            val r = items.getJSONObject(i)
            val id = r.optString("id").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
            val titulo = r.optString("titulo").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
            JinxRow(
                id = id,
                titulo = titulo,
                due = r.optString("due").takeIf { it.isNotEmpty() && it != "null" },
                categoria = r.optString("categoria").takeIf { it.isNotEmpty() && it != "null" },
                source = r.optString("source").takeIf { it.isNotEmpty() && it != "null" },
                status = r.optString("status", "pendiente"),
                updatedAt = r.optString("updated_at").takeIf { it.isNotEmpty() && it != "null" }?.let { TaskLogic.isoToUnix(it) },
            )
        }
    }

    fun parseId(json: String): String = checked(json).getString("id")
    fun parseOk(json: String) { checked(json) }
}

/** The real thing: one ssh command per request, over the server profile. */
class SshPendientes(private val context: Context, private val profile: SshProfile) : PendientesApi {
    private val offset: Long get() = java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000L

    private fun <T> withConnection(block: (Ssh.Connection) -> T): T {
        val conn = Ssh.open(context, profile)
        try {
            return block(conn)
        } finally {
            conn.close()
        }
    }

    private fun ask(request: String) = withConnection { it.exec("coucou-pendientes", request) }

    override fun list() = JinxProtocol.parseList(ask(JinxProtocol.list()))
    override fun add(task: CoucouTask) = JinxProtocol.parseId(ask(JinxProtocol.add(task, offset)))
    override fun resolve(id: String, action: String) = JinxProtocol.parseOk(ask(JinxProtocol.resolve(id, action)))
}

/** The result of one round with Jinx. */
data class SyncOutcome(
    val tasks: List<CoucouTask>,
    /** Her open pendientes that are not ours (what Alan told her). */
    val fromJinx: List<JinxRow>,
    /** Why she could not be reached or refused something; the local list is right anyway. */
    val error: String? = null,
)

object JinxSync {
    /**
     * One round: read her list, bring ours in step in both directions (see [TaskLogic.reconcile]), hand
     * over what is hers to have, close what was closed here, and return what is left of hers. What
     * cannot be done now is asked again at the next round, because the difference is still there.
     */
    fun run(
        api: PendientesApi, store: TaskStore, now: Long, queue: DismissQueue? = null,
        offset: (Long) -> Long = { t -> java.util.TimeZone.getDefault().getOffset(t * 1000) / 1000L },
    ): SyncOutcome {
        var rows = try {
            api.list()
        } catch (e: Exception) {
            return SyncOutcome(store.load(), emptyList(), e.message ?: "Jinx is not reachable")
        }
        var plan = com.agupta07505.smartisland.coucou.SyncPlan()
        // One list on every device: every task is shared.
        store.update { l -> l.map { if (it.jinx) it else it.copy(jinx = true) } }
        store.update { current ->
            val (next, p) = TaskLogic.reconcile(current, rows, now)
            plan = p
            next
        }
        var error: String? = null
        var wrote = false
        // Tasks deleted here whose pendiente is still open there.
        for (jid in queue?.ids().orEmpty()) {
            try {
                if (rows.any { it.id == jid && it.open }) api.resolve(jid, "descartado").also { wrote = true }
                queue?.remove(jid)
            } catch (e: Exception) {
                error = e.message
            }
        }
        for (id in plan.toAdd) {
            val task = store.load().firstOrNull { it.id == id } ?: continue
            try {
                val jinxId = api.add(task)
                store.update { l -> l.map { if (it.id == id) it.copy(jinxId = jinxId) else it } }
                wrote = true
            } catch (e: Exception) {
                error = e.message
            }
        }
        for ((jid, action) in plan.toResolve) {
            try {
                api.resolve(jid, action)
                wrote = true
            } catch (e: Exception) {
                error = e.message
            }
        }
        // What was just changed is read again, so a closed pendiente does not flash back.
        if (wrote) rows = runCatching { api.list() }.getOrDefault(rows)
        // Hers become ours: the same kind of task, with the same actions, marked with who made them.
        store.update { TaskLogic.importUnlinked(it, rows, now, offset).first }
        val list = store.load()
        return SyncOutcome(list, TaskLogic.unlinkedOpen(list, rows), error)
    }
}
