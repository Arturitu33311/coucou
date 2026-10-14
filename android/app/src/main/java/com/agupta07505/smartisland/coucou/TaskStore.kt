/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * The tasks, kept in one small JSON file in the app's private folder and written through a
 * temporary file, so a crash never leaves half of it. One change at a time.
 */
class TaskStore(private val file: File) {

    /** Everything on disk, tombstones included (what the sync and the merge work on). */
    @Synchronized
    fun loadAll(): List<CoucouTask> {
        if (!file.exists()) return emptyList()
        return runCatching { decode(file.readText()) }.getOrDefault(emptyList())
    }

    /** What the screens see: the list without the tombstones. */
    fun load(): List<CoucouTask> = TaskLogic.visible(loadAll())

    /**
     * Reads the whole list (tombstones included), lets [change] produce the new one, saves it and returns what
     * the screens see of it.
     */
    @Synchronized
    fun update(change: (List<CoucouTask>) -> List<CoucouTask>): List<CoucouTask> {
        val next = TaskLogic.purgeTombstones(change(loadAll()), System.currentTimeMillis() / 1000)
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(encode(next))
        check(tmp.renameTo(file)) { "Could not save the tasks" }
        return TaskLogic.visible(next)
    }

    companion object {
        fun of(context: Context) = TaskStore(File(context.applicationContext.filesDir, "coucou-tasks.json"))

        fun encode(list: List<CoucouTask>): String = JSONArray().also { arr ->
            for (t in list) arr.put(
                JSONObject()
                    .put("id", t.id)
                    .put("title", t.title)
                    .put("done", t.done)
                    .put("remindAt", t.remindAt ?: JSONObject.NULL)
                    .put("notified", t.notified)
                    .put("created", t.created)
                    .put("completedAt", t.completedAt ?: JSONObject.NULL)
                    .put("jinx", t.jinx)
                    .put("jinxId", t.jinxId ?: JSONObject.NULL)
                    .put("origin", t.origin ?: JSONObject.NULL)
                    .put("updatedAt", t.updatedAt)
                    .put("deleted", t.deleted)
                    .put("jinxStatus", t.jinxStatus ?: JSONObject.NULL)
                    .put("jinxSynced", t.jinxSynced),
            )
        }.toString()

        fun decode(json: String): List<CoucouTask> {
            val arr = JSONArray(json)
            return (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                val id = o.getString("id")
                val created = o.optLong("created")
                val completedAt = if (o.isNull("completedAt")) null else o.getLong("completedAt")
                var updatedAt = o.optLong("updatedAt", 0L)
                // A file from before the stamps: what was made here counts from when it was made or finished;
                // what was taken in from Jinx stays at 0, so any real change beats it.
                if (updatedAt == 0L && !id.startsWith("j-")) updatedAt = maxOf(completedAt ?: created, created)
                CoucouTask(
                    id = id,
                    title = o.getString("title"),
                    done = o.optBoolean("done"),
                    remindAt = if (o.isNull("remindAt")) null else o.getLong("remindAt"),
                    notified = o.optBoolean("notified"),
                    created = created,
                    completedAt = completedAt,
                    jinx = o.optBoolean("jinx"),
                    jinxId = if (o.isNull("jinxId")) null else o.getString("jinxId"),
                    origin = if (o.isNull("origin")) null else o.optString("origin").takeIf { it.isNotEmpty() },
                    updatedAt = updatedAt,
                    deleted = o.optBoolean("deleted"),
                    jinxStatus = if (o.isNull("jinxStatus")) null else o.optString("jinxStatus").takeIf { it.isNotEmpty() },
                    jinxSynced = o.optLong("jinxSynced", 0L),
                )
            }
        }
    }
}

/**
 * Her pendientes to dismiss because the task was deleted here. A deleted task leaves no trace in the
 * list, so what is owed to her is kept apart and emptied the next time she can be reached.
 */
class DismissQueue(private val file: File) {
    @Synchronized
    fun ids(): List<String> {
        if (!file.exists()) return emptyList()
        return runCatching { JSONArray(file.readText()).let { a -> (0 until a.length()).map { a.getString(it) } } }.getOrDefault(emptyList())
    }

    @Synchronized
    private fun save(list: List<String>) {
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(JSONArray(list).toString())
        check(tmp.renameTo(file)) { "Could not save the queue" }
    }

    @Synchronized
    fun add(id: String) = save((ids() + id).distinct())

    @Synchronized
    fun remove(id: String) = save(ids() - id)

    companion object {
        fun of(context: Context) = DismissQueue(File(context.applicationContext.filesDir, "coucou-jinx-dismiss.json"))
    }
}
