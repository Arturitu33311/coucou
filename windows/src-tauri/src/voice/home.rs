// What « OK Coucou » can ask Jinx for, as two tools:
//
//   * `casa`: the house (TV, lights, volume…). It goes to the quick Jinx on the server (the G2 bridge,
//     mode "lite"), which holds the Home Assistant key and a closed list of what a voice may ask. Coucou
//     never sees that key; it only holds the bridge's own (secret `jinx-g2-key`).
//   * `jinx`: anything that needs her memory or her real tools. It starts a run exactly like the chat
//     does and returns at once ("passed on"); her answer appears in her pill, not in this conversation.

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::{net, secrets};

pub const BRIDGE_URL_KEY: &str = "jinx-g2-url";
pub const BRIDGE_KEY: &str = "jinx-g2-key";

pub fn home_configured() -> bool {
    secrets::present(BRIDGE_URL_KEY) && secrets::present(BRIDGE_KEY)
}

/// Whether the full Jinx can be reached from here (her gateway, or her Hermes address and key).
pub fn jinx_configured(app: &AppHandle) -> bool {
    let backend = app
        .try_state::<crate::Shared>()
        .map(|s| s.settings.lock().unwrap().jinx_backend.clone())
        .unwrap_or_default();
    crate::jinx_gateway::use_gateway(&backend) || (secrets::present(crate::jinx::URL_KEY) && secrets::present(crate::jinx::API_KEY))
}

pub fn tools(app: &AppHandle) -> Vec<Value> {
    let mut out = Vec::new();
    if home_configured() {
        out.push(json!({
            "name": "casa",
            "description": "Control the user's home through the quick Jinx: turn the TV, lights, plugs, fans or the heating on or off, \
change the volume, play, pause or skip, set a temperature or a scene. Pass the request in plain words, as the user said it \
(say which device if they did). Locks, alarm and garage are refused there: tell the user to do those by hand.",
            "input_schema": {
                "type": "object",
                "properties": { "request": { "type": "string", "description": "What to do at home, in the user's words." } },
                "required": ["request"]
            }
        }));
    }
    if jinx_configured(app) {
        out.push(json!({
            "name": "jinx",
            "description": "Hand a request to the full Jinx (the user's assistant with memory, notes, mail, scheduling and the PC's tools). \
Use it for what needs her memory or those tools, never for something you can do yourself. It returns at once: \
her answer shows up in her pill later, so only say that you passed it on.",
            "input_schema": {
                "type": "object",
                "properties": { "request": { "type": "string", "description": "The request, complete enough to be understood on its own." } },
                "required": ["request"]
            }
        }));
    }
    out
}

/// What the quick Jinx says of a home request, and whether it did something.
pub fn ask_home(request: &str) -> (String, bool) {
    let request = request.trim();
    if request.is_empty() {
        return ("Nothing was asked: ask the user what to do at home.".into(), false);
    }
    let result = tauri::async_runtime::block_on(async {
        let base = crate::jinx::clean_base(&secrets::get(BRIDGE_URL_KEY).ok_or("no bridge address")?)?;
        let key = secrets::get(BRIDGE_KEY).ok_or("no bridge key")?;
        let url = reqwest::Url::parse(&format!("{base}/api/g2/ask")).map_err(|e| e.to_string())?;
        let response = net::client(&url, Duration::from_secs(50))?
            .post(url)
            .bearer_auth(&key)
            .json(&json!({ "text": request, "mode": "lite" }))
            .send()
            .await
            .map_err(|e| format!("cannot reach the house bridge: {e}"))?;
        let status = response.status();
        let bytes = net::read_capped(response, net::MAX_BODY).await?;
        if !status.is_success() {
            return Err(format!("the house bridge answered {status}"));
        }
        serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())
    });
    match result {
        Ok(v) => read_reply(&v),
        Err(err) => {
            crate::log::line(format!("voice: house request failed: {err}"));
            (format!("The house did not answer ({err}). Say so briefly."), false)
        }
    }
}

/// The bridge's reply as a tool result.
pub fn read_reply(v: &Value) -> (String, bool) {
    match v["status"].as_str() {
        Some("done") => (v["text"].as_str().unwrap_or("").to_string(), true),
        Some("pending") => ("Still working on it; say you will tell them when it is done.".into(), false),
        _ => (format!("Failed: {}. Say so briefly.", v["error"].as_str().unwrap_or("unknown")), false),
    }
}

/// Starts a run of the full Jinx and returns at once.
pub fn pass_to_jinx(app: &AppHandle, request: &str) -> (String, bool) {
    let request = request.trim().to_string();
    if request.is_empty() {
        return ("Nothing to pass on: ask the user what to hand over.".into(), false);
    }
    let (backend, shared) = app
        .try_state::<crate::Shared>()
        .map(|s| {
            let s = s.settings.lock().unwrap();
            (s.jinx_backend.clone(), s.jinx_share && !s.server_host.trim().is_empty())
        })
        .unwrap_or_default();
    // Said by voice and rewritten by the model: Jinx treats it as a request to weigh, not a command to obey blindly.
    let request = format!("[Pedido por voz con «OK Coucou», reformulado por el modelo; si algo es delicado o destructivo, confirma antes con el usuario] {request}");
    if crate::jinx_gateway::use_gateway(&backend) {
        crate::jinx_gateway::send(app.clone(), request);
    } else {
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let jinx = app2.state::<crate::jinx::Jinx>();
            if let Err(err) = crate::jinx::send(app2.clone(), &jinx, request, None, shared).await {
                crate::log::line(format!("voice: could not pass to Jinx: {err}"));
            }
        });
    }
    ("Passed on to Jinx; her answer will appear in her pill.".into(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_reply_becomes_a_tool_result() {
        assert_eq!(read_reply(&json!({ "status": "done", "text": "Apagada." })), ("Apagada.".into(), true));
        assert!(!read_reply(&json!({ "status": "pending" })).1);
        let (msg, did) = read_reply(&json!({ "status": "error", "error": "no hay llave" }));
        assert!(msg.contains("no hay llave") && !did);
    }
}
