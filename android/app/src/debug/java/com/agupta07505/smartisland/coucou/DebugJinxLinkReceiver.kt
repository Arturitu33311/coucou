package com.agupta07505.smartisland.coucou

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import com.agupta07505.smartisland.coucou.agents.AgentUi
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import com.agupta07505.smartisland.coucou.phone.PhoneKey
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.followClaude
import com.agupta07505.smartisland.coucou.phone.SshProfile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * Debug builds only, and only on an emulator: drives the real links (laptop and Jinx's server) from `adb`, so the
 * whole path (sshj, AgentRemote, the merged view, the routing of an answer) can be tested without touching a phone.
 *
 *   adb shell am broadcast -n <pkg>/.coucou.DebugJinxLinkReceiver -a SETUP --es host 10.0.2.2 --ei port 22 --es user alan --es hostKey SHA256:… [--ez laptop true]
 *   … -a DUMP | -a ANSWER --es id <id> --es decision allow|deny | -a DECLINE --es id <id> | -a RESET
 * Result: files/debug-jinx-link.json (read it with `adb shell run-as <pkg> cat files/debug-jinx-link.json`).
 */
class DebugJinxLinkReceiver : BroadcastReceiver() {
    private fun isEmulator() =
        Build.HARDWARE.contains("ranchu") || Build.HARDWARE.contains("goldfish") || Build.FINGERPRINT.startsWith("generic") || Build.MODEL.contains("sdk", true)

    override fun onReceive(context: Context, intent: Intent) {
        if (!isEmulator()) return
        val app = context.applicationContext
        var last: JSONObject? = null
        when (intent.action) {
            "SETUP" -> {
                val p = SshProfile(
                    host = intent.getStringExtra("host") ?: "10.0.2.2", port = intent.getIntExtra("port", 22),
                    user = intent.getStringExtra("user") ?: "alan", hostKey = intent.getStringExtra("hostKey") ?: "",
                )
                PhoneSettings.save(app, PhoneSettings.SERVER, p)
                // The laptop profile is set only when asked; otherwise it is cleared (an emulator can keep an old one).
                PhoneSettings.save(app, PhoneSettings.LAPTOP, if (intent.getBooleanExtra("laptop", false)) p else SshProfile("", 22, "", ""))
                val pub = PhoneKey.publicLine(app) ?: PhoneKey.create(app)
                File(app.filesDir, "debug-pubkey.txt").writeText(pub)
                app.followClaude = true
                CoucouRuntime.apply(app)
            }
            "ANSWER" -> {
                val id = intent.getStringExtra("id").orEmpty()
                last = JSONObject().put("op", "answer").put("id", id).put("sent", CoucouRuntime.sendForRequest(id, RemoteProtocol.answer(id, intent.getStringExtra("decision").orEmpty())))
            }
            "DECLINE" -> {
                val id = intent.getStringExtra("id").orEmpty()
                last = JSONObject().put("op", "decline").put("id", id).put("sent", CoucouRuntime.sendForRequest(id, RemoteProtocol.decline(id)))
            }
            "RESET" -> {
                app.followClaude = false
                CoucouRuntime.apply(app)
            }
            "DUMP" -> Unit
        }
        dump(app, last)
    }

    private fun dump(app: Context, last: JSONObject?) {
        val ui: AgentUi = CoucouRuntime.agentUi.value
        val o = JSONObject()
            .put("at", System.currentTimeMillis())
            .put("status", ui.status.name)
            .put("detail", ui.detail)
            .put("pending", JSONArray(ui.pending.map {
                JSONObject().put("id", it.id).put("agent", it.agent).put("project", it.project).put("tool", it.tool)
                    .put("detail", it.detail).put("timeoutS", it.timeoutS).put("question", it.isQuestion)
            }))
            .put("sessions", ui.sessions.size)
        if (last != null) o.put("last", last)
        File(app.filesDir, "debug-jinx-link.json").writeText(o.toString())
    }
}
