/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.os.SystemClock
import android.util.Log
import com.google.ai.edge.litertlm.Backend
import com.google.ai.edge.litertlm.Content
import com.google.ai.edge.litertlm.ConversationConfig
import com.google.ai.edge.litertlm.Contents
import com.google.ai.edge.litertlm.Engine
import com.google.ai.edge.litertlm.EngineConfig
import com.google.ai.edge.litertlm.Message
import com.google.ai.edge.litertlm.SamplerConfig

/**
 * The real [LlmRuntime]: Google's LiteRT-LM engine on the CPU (GPU/NPU are not assumed on these phones).
 * Loading the model takes seconds and about 3 GB of RAM, so [LocalModelBrain] keeps it only while it is in use.
 * Timings go to logcat (`JinxLocal`) so the speed on each phone is measured, not guessed.
 */
class LiteRtRuntime(modelPath: String, cacheDir: String) : LlmRuntime {
    private val engine: Engine

    init {
        val t = SystemClock.elapsedRealtime()
        engine = Engine(
            EngineConfig(
                modelPath = modelPath,
                backend = Backend.CPU(),
                maxNumTokens = MAX_CONTEXT_TOKENS,
                cacheDir = cacheDir,
            ),
        )
        try {
            engine.initialize()
        } catch (e: Throwable) {
            runCatching { engine.close() }
            throw e
        }
        Log.i("JinxLocal", "engine loaded in ${SystemClock.elapsedRealtime() - t} ms")
    }

    override fun generate(system: String, history: List<Pair<Boolean, String>>, user: String, maxTokens: Int): String {
        val t = SystemClock.elapsedRealtime()
        val config = ConversationConfig(
            systemInstruction = Contents.of(system),
            initialMessages = history.map { (jinx, text) -> if (jinx) Message.model(text) else Message.user(text) },
            samplerConfig = SamplerConfig(40, 0.95, 0.8, 0),
            maxOutputToken = maxTokens,
        )
        engine.createConversation(config).use { conversation ->
            val reply = conversation.sendMessage(user)
            val text = reply.contents.contents.filterIsInstance<Content.Text>().joinToString("") { it.text }
            Log.i(
                "JinxLocal",
                "generated ${text.length} chars in ${SystemClock.elapsedRealtime() - t} ms " +
                    "(system ${system.length} chars, history ${history.size}, ${conversation.tokenCountSafe()} tokens)",
            )
            return text
        }
    }

    override fun close() {
        runCatching { engine.close() }
    }

    private fun com.google.ai.edge.litertlm.Conversation.tokenCountSafe(): String =
        runCatching { getTokenCount().toString() }.getOrDefault("?")

    private companion object {
        /** Prompt plus answer: the persona (≤ 3 000 chars), six turns and 400 new tokens fit with room. */
        const val MAX_CONTEXT_TOKENS = 2048
    }
}
