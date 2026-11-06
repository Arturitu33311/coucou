// OpenRouter as the language model behind « OK Coucou » and Flow's clean-up, for whoever has an
// OpenRouter key and no Anthropic one. Only the words (never audio) go there, with that key.
//
// Chat-completions format with tools: the caller keeps the messages in this format, the helpers
// here only build requests and read replies, so they can be tested without the network.

use std::time::Duration;

use serde_json::{json, Value};

use crate::{net, secrets};

pub const KEY: &str = "openrouter-api-key";
/// Fast and good at tool calls; the same model the server's quick Jinx uses. `COUCOU_VOICE_MODEL` replaces it.
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-flash";
const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

pub fn present() -> bool {
    secrets::present(KEY)
}

pub fn model() -> String {
    std::env::var("COUCOU_VOICE_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| DEFAULT_MODEL.into())
}

async fn post(body: &Value, timeout: Duration) -> Result<Value, String> {
    let key = secrets::get(KEY).ok_or("OpenRouter API key missing")?;
    let url = reqwest::Url::parse(ENDPOINT).map_err(|e| e.to_string())?;
    let response = net::client(&url, timeout)?
        .post(url)
        .bearer_auth(&key)
        .header("X-Title", "Coucou")
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = net::read_capped(response, net::MAX_ERROR_BODY).await.unwrap_or_default();
        return Err(format!("OpenRouter {status}: {}", net::error_detail(&body)));
    }
    let bytes = net::read_capped(response, net::MAX_BODY).await?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Bad API response: {e}"))
}

/// One question, one answer: Flow's clean-up and the like.
pub async fn complete(system: &str, user: &str, max_tokens: u32, timeout: Duration) -> Result<String, String> {
    let body = json!({
        "model": model(),
        "max_tokens": max_tokens,
        "messages": [ { "role": "system", "content": system }, { "role": "user", "content": user } ],
    });
    let reply = post(&body, timeout).await?;
    Ok(parse_turn(&reply)?.text)
}

/// One turn of the voice conversation: the system prompt, the messages so far, the tools.
pub async fn chat_turn(system: &str, messages: &[Value], tools: &[Value]) -> Result<Turn, String> {
    let mut all = vec![json!({ "role": "system", "content": system })];
    all.extend(messages.iter().cloned());
    let mut body = json!({ "model": model(), "max_tokens": 1024, "messages": all });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }
    parse_turn(&post(&body, Duration::from_secs(45)).await?)
}

const LOCAL_ENDPOINT: &str = "http://127.0.0.1:11435/v1/chat/completions";
const LOCAL_MODEL: &str = "gemma4:e2b";
/// Short on purpose: on this laptop's CPU every prompt token costs time, and a turn must come back in seconds.
pub const LOCAL_SYSTEM: &str = "Eres Jinx, la asistente de Alan: directa, mexicana, casual. Hablas en voz alta desde su computadora: \
responde en español con una o dos frases cortas, sin markdown ni emojis. Actúa con las herramientas (pills, music, casa, open_app) en vez de describir, \
y di lo que hiciste en pocas palabras. Si te pide algo que no puedes hacer ahora (correo, agenda, notas, memoria), dilo y que lo intente en un rato. No inventes datos.";

/// OpenRouter did not answer (no credits, offline): one turn from the small local model on this laptop (Ollama on :11435), with the few essential tools.
/// `COUCOU_LOCAL_MODEL` replaces the model. Same message format as `chat_turn`.
pub async fn local_chat_turn(system: &str, messages: &[Value], tools: &[Value]) -> Result<Turn, String> {
    let model = std::env::var("COUCOU_LOCAL_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| LOCAL_MODEL.into());
    let mut all = vec![json!({ "role": "system", "content": system })];
    all.extend(messages.iter().cloned());
    let mut body = json!({ "model": model, "max_tokens": 120, "stream": false, "reasoning_effort": "none", "messages": all });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }
    let url = reqwest::Url::parse(LOCAL_ENDPOINT).map_err(|e| e.to_string())?;
    let response = net::client(&url, Duration::from_secs(120))?
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("local model {status}"));
    }
    let bytes = net::read_capped(response, net::MAX_BODY).await?;
    let reply: Value = serde_json::from_slice(&bytes).map_err(|e| format!("Bad API response: {e}"))?;
    parse_turn(&reply)
}

/// What the model answered.
#[derive(Debug, PartialEq)]
pub struct Turn {
    pub text: String,
    pub calls: Vec<Call>,
    /// The assistant message exactly as it came, to put back in the history.
    pub message: Value,
}

#[derive(Debug, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub input: Value,
}

pub fn parse_turn(reply: &Value) -> Result<Turn, String> {
    if let Some(err) = reply.get("error") {
        return Err(format!("OpenRouter: {}", err["message"].as_str().unwrap_or("error")));
    }
    let message = reply["choices"][0]["message"].clone();
    if !message.is_object() {
        return Err("OpenRouter sent no answer".into());
    }
    let text = match &message["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    };
    let calls = message["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .filter_map(|c| {
                    let name = c["function"]["name"].as_str()?.to_string();
                    // The arguments arrive as a JSON *string*; a broken one is an empty call, not a crash.
                    let input = match &c["function"]["arguments"] {
                        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
                        other if other.is_object() => other.clone(),
                        _ => json!({}),
                    };
                    Some(Call { id: c["id"].as_str().unwrap_or("call").to_string(), name, input })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Turn { text, calls, message })
}

/// Anthropic-style tool declarations (`input_schema`) as chat-completions functions. Server-side
/// tools of Anthropic (web search) have no schema and are left out.
pub fn function_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter(|t| t.get("input_schema").is_some())
        .map(|t| json!({
            "type": "function",
            "function": { "name": t["name"], "description": t["description"], "parameters": t["input_schema"] }
        }))
        .collect()
}

/// Keeps the last `max` messages, starting at a user's own message: never a tool result without its call.
pub fn trim(messages: &mut Vec<Value>, max: usize) {
    if messages.len() <= max {
        return;
    }
    let mut cut = messages.len() - max;
    while cut < messages.len() && !(messages[cut]["role"] == "user") {
        cut += 1;
    }
    messages.drain(..cut.min(messages.len() - 1));
}

/// A history ending on a tool call nobody answered is refused by the API: that call is dropped.
pub fn repair(messages: &mut Vec<Value>) {
    while let Some(last) = messages.last() {
        let dangling = last["role"] == "assistant" && last["tool_calls"].as_array().is_some_and(|c| !c.is_empty());
        let orphan = last["role"] == "tool";
        if dangling || orphan {
            messages.pop();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_arguments_come_as_a_string() {
        let reply = json!({ "choices": [ { "message": { "role": "assistant", "content": null, "tool_calls": [
            { "id": "c1", "type": "function", "function": { "name": "music", "arguments": "{\"action\":\"next\"}" } },
            { "id": "c2", "type": "function", "function": { "name": "pills", "arguments": "not json" } }
        ] } } ] });
        let turn = parse_turn(&reply).unwrap();
        assert_eq!(turn.calls.len(), 2);
        assert_eq!(turn.calls[0], Call { id: "c1".into(), name: "music".into(), input: json!({ "action": "next" }) });
        assert_eq!(turn.calls[1].input, json!({}));
        assert_eq!(turn.text, "");
    }

    #[test]
    fn a_plain_answer_and_an_error_are_read() {
        let ok = json!({ "choices": [ { "message": { "role": "assistant", "content": "Listo." } } ] });
        assert_eq!(parse_turn(&ok).unwrap().text, "Listo.");
        let err = json!({ "error": { "message": "No credits" } });
        assert_eq!(parse_turn(&err).unwrap_err(), "OpenRouter: No credits");
        assert!(parse_turn(&json!({})).is_err());
    }

    #[test]
    fn anthropic_tools_become_functions_and_server_tools_are_dropped() {
        let tools = vec![
            json!({ "name": "music", "description": "d", "input_schema": { "type": "object" } }),
            json!({ "type": "web_search_20250305", "name": "web_search", "max_uses": 3 }),
        ];
        let f = function_tools(&tools);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0]["function"]["name"], "music");
        assert_eq!(f[0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn trim_never_leaves_a_tool_result_without_its_call() {
        let mut m = vec![
            json!({ "role": "user", "content": "a" }),
            json!({ "role": "assistant", "tool_calls": [ { "id": "1" } ] }),
            json!({ "role": "tool", "tool_call_id": "1", "content": "r" }),
            json!({ "role": "assistant", "content": "ok" }),
            json!({ "role": "user", "content": "b" }),
        ];
        trim(&mut m, 3);
        assert_eq!(m[0]["role"], "user");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0]["content"], "b");
    }

    #[test]
    fn repair_drops_an_unanswered_call() {
        let mut m = vec![
            json!({ "role": "user", "content": "a" }),
            json!({ "role": "assistant", "tool_calls": [ { "id": "1" } ] }),
        ];
        repair(&mut m);
        assert_eq!(m.len(), 1);
        let mut done = vec![json!({ "role": "user", "content": "a" }), json!({ "role": "assistant", "content": "x" })];
        repair(&mut done);
        assert_eq!(done.len(), 2);
    }
}
