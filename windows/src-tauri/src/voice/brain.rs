// « OK Coucou »'s brain: Claude with the user's key and Coucou's tools —
// pills, music, the user's own data, opening an app, preparing an email (the
// mail app opens with it filled in; the user sends it) and Anthropic's web
// search. The conversation lives in memory only and is forgotten 90 s after
// the last turn. Port of ClaudeVoiceBrain.swift.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const FAST_MODEL: &str = "claude-haiku-4-5";
const FORGET_AFTER: Duration = Duration::from_secs(90);

pub struct Brain {
    /// The conversation in Anthropic's format (used with a Claude key).
    messages: Vec<Value>,
    /// The same conversation in chat-completions format (used with an OpenRouter key).
    or_messages: Vec<Value>,
    last: Instant,
}

impl Default for Brain {
    fn default() -> Self {
        Self { messages: Vec::new(), or_messages: Vec::new(), last: Instant::now() }
    }
}

pub struct Reply {
    pub text: String,
    pub acted: bool,
}

fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "pills",
            "description": "Add, remove, set as main or list the pills shown in the island at the top of the screen. At most 4 pills besides the main one: remove one first if needed.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["add", "remove", "set_main", "list"] },
                    "names": { "type": "array", "items": { "type": "string" }, "description": "Pill names as listed (e.g. Stripe, GitHub, Cursor). Empty for list." }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "music",
            "description": "Control Spotify: play or pause, next, previous, set the volume, or search a song, an artist or a playlist in Spotify.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["play_pause", "next", "previous", "set_volume", "search"] },
                    "query": { "type": "string", "description": "What to search, for search." },
                    "volume": { "type": "integer", "description": "0-100 for set_volume." }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "service_info",
            "description": "Read the user's real data that Coucou already has, as JSON: the active pills, the agent sessions (Claude Code, Codex…) and what they are doing, Claude or Codex plan usage, Spotify now playing, and the services' latest data (Stripe, GitHub, Vercel, Resend, n8n, Notion, Cal.com).",
            "input_schema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "open_app",
            "description": "Open an app on this computer by name (Firefox, VS Code, Notion, Spotify…).",
            "input_schema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }
        }),
        json!({
            "name": "prepare_email",
            "description": "Open the user's mail app with an email filled in, for them to check and send (it is never sent automatically). The recipient must be an email address.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "recipient": { "type": "string", "description": "An email address." },
                    "subject": { "type": "string" },
                    "body": { "type": "string", "description": "The full email text, written for the user." }
                },
                "required": ["recipient", "subject", "body"]
            }
        }),
        json!({ "type": "web_search_20250305", "name": "web_search", "max_uses": 3 }),
    ]
}

fn system_prompt(pills: &str, web_search: bool) -> String {
    let t = crate::platform::local_time();
    let os = if cfg!(windows) { "Windows" } else { "Linux" };
    let web = if web_search { ", and web_search for anything current (weather, news, scores, prices, facts you are not sure of)" } else { "" };
    format!(
        "You are Jinx, the user's assistant, speaking from the top of the screen of their {os} computer: they call you by saying \
« OK Coucou ». Your character: direct, warm, a little cheeky, never like a manual. It is {:04}-{:02}-{:02} {:02}:{:02}.\n\
Everything you write is read aloud: answer in the language the user spoke, in one or two short sentences, \
no markdown, no lists, no emojis, no links.\n\
You are the quick version of Jinx: you have no long memory. Act with the tools instead of describing: pills, music, \
service_info for the user's own data, open_app, prepare_email{web}. Use casa for anything at home (TV, lights, volume, heating) \
and say what you did in a few words; use jinx to hand over what needs her memory, notes, mail, scheduling or the \
computer's real tools, and then only say that you passed it on. Never invent the user's data; if you do not know, say so.\n\
Email: gather the recipient's address, the subject and what to say (write the text yourself from what the user \
wants to say, in the language they used), asking for what is missing one short question at a time, then call \
prepare_email. The email is never sent by you: the user checks it in their mail app and sends it.\n\
When you need an answer from the user, end with one short question. Otherwise never end with a question.\n\
Pills: {pills}",
        t.year, t.month, t.day, t.hour, t.minute
    )
}

impl Brain {
    pub fn respond(&mut self, app: &AppHandle, transcript: &str) -> Option<Reply> {
        if self.last.elapsed() > FORGET_AFTER {
            self.messages.clear();
            self.or_messages.clear();
        }
        self.last = Instant::now();
        // OpenRouter first when its key is set; Claude otherwise.
        if crate::or_brain::present() {
            return self.respond_openrouter(app, transcript);
        }
        self.messages.push(json!({ "role": "user", "content": transcript }));
        self.trim();
        let pills = super::ask_island(app, "pills", json!({ "action": "list" })).unwrap_or_default();
        let system = system_prompt(&pills, true);
        let mut tools = tools();
        tools.extend(super::home::tools(app));
        let chat_model = app
            .try_state::<crate::Shared>()
            .map(|s| s.settings.lock().unwrap().model.clone())
            .unwrap_or_default();
        let mut acted = false;
        let mut spoken = String::new();
        for _ in 0..5 {
            let turn = tauri::async_runtime::block_on(async {
                match crate::claude::voice_turn(FAST_MODEL, &system, &self.messages, &tools).await {
                    Ok(v) => Ok(v),
                    Err(err) if !chat_model.is_empty() => {
                        crate::log::line(format!("voice: {err}; the chat's model instead"));
                        crate::claude::voice_turn(&chat_model, &system, &self.messages, &tools).await
                    }
                    Err(err) => Err(err),
                }
            });
            let turn = match turn {
                Ok(t) => t,
                Err(err) => {
                    crate::log::line(format!("voice: Claude turn failed: {err}"));
                    if self.messages.last().is_some_and(|m| m["content"].is_string()) {
                        self.messages.pop();
                    }
                    return acted.then(|| Reply { text: String::new(), acted });
                }
            };
            let content = turn["content"].as_array().cloned().unwrap_or_default();
            self.messages.push(json!({ "role": "assistant", "content": content.clone() }));
            let text: String = content.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(" ");
            if !text.trim().is_empty() {
                spoken = text;
            }
            let stop = turn["stop_reason"].as_str().unwrap_or("end_turn");
            if stop == "pause_turn" {
                continue;
            }
            let uses: Vec<&Value> = content.iter().filter(|b| b["type"] == "tool_use").collect();
            if stop != "tool_use" || uses.is_empty() {
                break;
            }
            let mut results = Vec::new();
            for u in uses {
                let (result, did) = super::run_tool(app, u["name"].as_str().unwrap_or(""), &u["input"]);
                acted |= did;
                results.push(json!({ "type": "tool_result", "tool_use_id": u["id"], "content": result }));
            }
            self.messages.push(json!({ "role": "user", "content": results }));
            spoken.clear();
        }
        self.repair();
        Some(Reply { text: spoken_text(&spoken), acted })
    }

    /// The same conversation through OpenRouter (chat-completions format, function tools).
    fn respond_openrouter(&mut self, app: &AppHandle, transcript: &str) -> Option<Reply> {
        self.or_messages.push(json!({ "role": "user", "content": transcript }));
        crate::or_brain::trim(&mut self.or_messages, 24);
        let pills = super::ask_island(app, "pills", json!({ "action": "list" })).unwrap_or_default();
        let system = system_prompt(&pills, false);
        let mut declared = tools();
        declared.extend(super::home::tools(app));
        let tools = crate::or_brain::function_tools(&declared);
        let mut acted = false;
        let mut spoken = String::new();
        for _ in 0..5 {
            let turn = tauri::async_runtime::block_on(crate::or_brain::chat_turn(&system, &self.or_messages, &tools));
            let turn = match turn {
                Ok(t) => t,
                Err(err) => {
                    crate::log::line(format!("voice: OpenRouter turn failed: {err}"));
                    if self.or_messages.last().is_some_and(|m| m["role"] == "user") {
                        self.or_messages.pop();
                    }
                    crate::or_brain::repair(&mut self.or_messages);
                    return acted.then(|| Reply { text: String::new(), acted });
                }
            };
            // Only the fields the API needs go back into the history (providers add others).
            let mut assistant = json!({ "role": "assistant", "content": turn.text });
            if !turn.calls.is_empty() {
                assistant["tool_calls"] = turn.message["tool_calls"].clone();
            }
            self.or_messages.push(assistant);
            if !turn.text.trim().is_empty() {
                spoken = turn.text.clone();
            }
            if turn.calls.is_empty() {
                break;
            }
            for c in &turn.calls {
                let (result, did) = super::run_tool(app, &c.name, &c.input);
                acted |= did;
                self.or_messages.push(json!({ "role": "tool", "tool_call_id": c.id, "content": result }));
            }
            spoken.clear();
        }
        crate::or_brain::repair(&mut self.or_messages);
        Some(Reply { text: spoken_text(&spoken), acted })
    }

    /// Keeps the last exchanges, never cutting between a tool call and its result.
    fn trim(&mut self) {
        if self.messages.len() <= 24 {
            return;
        }
        let mut cut = self.messages.len() - 24;
        while cut < self.messages.len() && !(self.messages[cut]["role"] == "user" && self.messages[cut]["content"].is_string()) {
            cut += 1;
        }
        self.messages.drain(..cut.min(self.messages.len() - 1));
    }

    /// The API refuses a history ending on a tool call with no result after it.
    fn repair(&mut self) {
        let Some(last) = self.messages.last() else { return };
        if last["role"] != "assistant" {
            return;
        }
        let calls = last["content"].as_array().is_some_and(|c| c.iter().any(|b| b["type"] == "tool_use"));
        if calls {
            self.messages.pop();
        }
    }
}

/// What Claude wrote, ready to be said: citations and markdown left out.
pub fn spoken_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut skip = 0;
    for c in text.chars() {
        match c {
            '[' => skip += 1,
            ']' if skip > 0 => skip -= 1,
            '*' | '#' | '`' | '_' if skip == 0 => {}
            _ if skip == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_and_citations_are_not_read_aloud() {
        assert_eq!(spoken_text("**Done** — Stripe [1] is in the island."), "Done — Stripe is in the island.");
        assert_eq!(spoken_text("  Il fait  20 °C à Bordeaux. "), "Il fait 20 °C à Bordeaux.");
    }

    #[test]
    fn every_tool_has_a_schema() {
        for tool in tools() {
            assert!(tool["name"].is_string());
            assert!(tool.get("input_schema").is_some() || tool["type"] == "web_search_20250305");
        }
    }
}
