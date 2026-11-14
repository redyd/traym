//! Custom AI providers: any OpenAI-compatible API (Ollama, LM Studio, OpenRouter, Mistral, Groq…).
//! traym runs the agent loop itself: streamed chat completions, with the traym MCP tools offered
//! as functions and executed in-process (they show up in the chat like Claude's and Codex's).

use crate::{agent::BRIEF, mcp, settings::Provider, Core};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::Emitter;

const MAX_STEPS: usize = 25;

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build().unwrap())
}

fn req(p: &Provider, post: bool, path: &str) -> reqwest::RequestBuilder {
    let url = format!("{}/{path}", p.base_url.trim().trim_end_matches('/'));
    let r = if post { client().post(url) } else { client().get(url) };
    if p.api_key.trim().is_empty() { r } else { r.bearer_auth(p.api_key.trim()) }
}

/// Non-2xx: the API's own error message when it gives one.
async fn failure(r: reqwest::Response) -> String {
    let status = r.status();
    let body = r.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&body).unwrap_or_default();
    let msg = v["error"]["message"].as_str().or(v["error"].as_str()).or(v["message"].as_str()).map(str::to_string);
    format!("{status}: {}", msg.unwrap_or_else(|| body.chars().take(300).collect()))
}

fn net(p: &Provider) -> impl Fn(reqwest::Error) -> String + '_ {
    move |e| format!("{} ({}) unreachable: {e}", p.name, p.base_url)
}

/// Small models are the default pick: names saying so, or a parameter count of 4B or less (`llama3.2:3b`).
fn light(id: &str) -> bool {
    let id = id.to_lowercase();
    ["mini", "small", "lite", "haiku", "flash", "nano", "tiny"].iter().any(|w| id.contains(w))
        || id.split([':', '-', '_', '/']).any(|t| t.strip_suffix('b').and_then(|n| n.parse::<f32>().ok()).is_some_and(|n| n <= 4.0))
}

/// `GET /models` -> [{ id, name, default, light }], the same shape as the CLIs' lists.
pub async fn models(p: Provider) -> Result<Vec<Value>, String> {
    let r = req(&p, false, "models").timeout(Duration::from_secs(15)).send().await.map_err(net(&p))?;
    if !r.status().is_success() {
        return Err(failure(r).await);
    }
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    let mut ids: Vec<&str> = v["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str()).collect();
    ids.sort_unstable();
    Ok(ids.iter().enumerate().map(|(i, id)| json!({ "id": id, "name": id, "default": i == 0, "light": light(id) })).collect())
}

/// One streamed completion, assembled from SSE `data:` lines (they can split anywhere, even inside a UTF-8 char).
#[derive(Default)]
struct Turn {
    buf: Vec<u8>,
    text: String,
    calls: Vec<(String, String, String)>, // id, name, arguments (JSON text, streamed in pieces)
    error: Option<String>,
}

impl Turn {
    /// Feed raw bytes; returns the text deltas they completed.
    fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut out = vec![];
        while let Some(i) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=i).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { continue }; // also skips [DONE]
            if let Some(e) = v["error"]["message"].as_str().or(v["error"].as_str()) {
                self.error = Some(e.to_string());
            }
            let d = &v["choices"][0]["delta"];
            if let Some(t) = d["content"].as_str().filter(|t| !t.is_empty()) {
                self.text.push_str(t);
                out.push(t.to_string());
            }
            for c in d["tool_calls"].as_array().into_iter().flatten() {
                let i = c["index"].as_u64().unwrap_or(self.calls.len().saturating_sub(1) as u64) as usize;
                if self.calls.len() <= i {
                    self.calls.resize(i + 1, Default::default());
                }
                let (id, name, args) = &mut self.calls[i];
                // id and name come once (OpenAI) or repeated (some servers): keep the first
                if id.is_empty() {
                    *id = c["id"].as_str().unwrap_or_default().to_string();
                }
                if name.is_empty() {
                    *name = c["function"]["name"].as_str().unwrap_or_default().to_string();
                }
                args.push_str(c["function"]["arguments"].as_str().unwrap_or_default());
            }
        }
        out
    }

    fn assistant(&self) -> Value {
        let mut m = json!({ "role": "assistant", "content": self.text });
        if !self.calls.is_empty() {
            let calls: Vec<Value> = self
                .calls
                .iter()
                .enumerate()
                .map(|(i, (id, name, args))| {
                    let id = if id.is_empty() { format!("call_{i}") } else { id.clone() };
                    let args = if args.trim().is_empty() { "{}".to_string() } else { args.clone() };
                    json!({ "id": id, "type": "function", "function": { "name": name, "arguments": args } })
                })
                .collect();
            m["tool_calls"] = json!(calls);
        }
        m
    }
}

/// The traym MCP tools, as chat-completions functions.
fn functions() -> Value {
    let tools = mcp::tools();
    let list = tools.as_array().unwrap().iter().map(|t| {
        json!({ "type": "function", "function": { "name": t["name"], "description": t["description"], "parameters": t["inputSchema"] } })
    });
    json!(list.collect::<Vec<_>>())
}

pub async fn send(core: Arc<Core>, id: String, prompt: String, session: Option<String>, model: Option<String>) -> Result<(), String> {
    let p = core.provider(&id)?;
    let model = model.filter(|m| !m.is_empty()).ok_or("choose a model first")?;
    let sid = session.unwrap_or_else(|| crate::settings::token()[..16].to_string());
    let _ = core.app.emit("agent://event", json!({ "type": "session", "id": sid }));
    {
        let mut chats = core.chats.lock().unwrap();
        let h = chats.entry(sid.clone()).or_insert_with(|| vec![json!({ "role": "system", "content": format!("{BRIEF}\n\n{}", mcp::INSTRUCTIONS) })]);
        h.push(json!({ "role": "user", "content": prompt }));
    }
    // a task, so "stop" can abort it mid-stream; the history keeps every completed step
    let work = tokio::spawn(run(core.clone(), p, model, sid));
    *core.task.lock().unwrap() = Some(work.abort_handle());
    let r = work.await;
    core.task.lock().unwrap().take();
    r.unwrap_or(Ok(())) // Err = aborted by the user
}

async fn run(core: Arc<Core>, p: Provider, model: String, sid: String) -> Result<(), String> {
    let emit = |v: Value| {
        let _ = core.app.emit("agent://event", v);
    };
    let tools = functions();
    for _ in 0..MAX_STEPS {
        let messages = core.chats.lock().unwrap().get(&sid).cloned().unwrap_or_default();
        let body = json!({ "model": model, "messages": messages, "tools": tools, "stream": true });
        let mut r = req(&p, true, "chat/completions").json(&body).send().await.map_err(net(&p))?;
        if !r.status().is_success() {
            return Err(failure(r).await);
        }
        let mut turn = Turn::default();
        while let Some(chunk) = r.chunk().await.map_err(net(&p))? {
            for d in turn.feed(&chunk) {
                emit(json!({ "type": "delta", "text": d }));
            }
        }
        turn.feed(b"\n"); // a last line without newline
        if let Some(e) = turn.error.take() {
            return Err(e);
        }
        if !turn.text.is_empty() {
            emit(json!({ "type": "message", "text": turn.text }));
        }
        let mut step = vec![turn.assistant()];
        let done = turn.calls.is_empty();
        for (i, (id, name, args)) in turn.calls.iter().enumerate() {
            emit(json!({ "type": "tool", "name": name }));
            let a: Value = serde_json::from_str(args).unwrap_or(json!({}));
            let out = mcp::run_tool(&core, name, &a).unwrap_or_else(|e| format!("error: {e}"));
            let id = if id.is_empty() { format!("call_{i}") } else { id.clone() };
            step.push(json!({ "role": "tool", "tool_call_id": id, "content": out }));
        }
        core.chats.lock().unwrap().entry(sid.clone()).or_default().extend(step);
        if done {
            return Ok(());
        }
    }
    Err(format!("stopped after {MAX_STEPS} tool steps"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_streamed_text_and_tool_calls() {
        let mut t = Turn::default();
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Je \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lis\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"read_note\",\"arguments\":\"{\\\"pa\"}}]}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"th\\\":\\\"a.md\\\"}\"}}]}}]}\n",
            "data: [DONE]\n",
        )
        .as_bytes();
        // split mid-line and mid-UTF-8 ("é" in a later chunk would still decode): feed in 7-byte pieces
        let deltas: Vec<String> = sse.chunks(7).flat_map(|c| t.feed(c)).collect();
        assert_eq!(deltas, vec!["Je ", "lis"]);
        assert_eq!(t.calls, vec![("c1".into(), "read_note".into(), "{\"path\":\"a.md\"}".into())]);
        let a = t.assistant();
        assert_eq!(a["tool_calls"][0]["function"]["arguments"], "{\"path\":\"a.md\"}");

        let mut u = Turn::default();
        let bytes = "data: {\"choices\":[{\"delta\":{\"content\":\"été\"}}]}\n".as_bytes();
        let (x, y) = bytes.split_at(31); // inside the first "é"
        assert!(u.feed(x).is_empty());
        assert_eq!(u.feed(y), vec!["été"]);

        let mut e = Turn::default();
        e.feed(b"data: {\"error\":{\"message\":\"model not found\"}}\n");
        assert_eq!(e.error.as_deref(), Some("model not found"));
    }

    #[test]
    fn small_models_are_light() {
        assert!(light("llama3.2:3b") && light("gpt-4o-mini") && light("qwen2.5:1.5b") && light("gemini-flash"));
        assert!(!light("llama3.1:70b") && !light("qwen3:8b") && !light("mistral-large"));
    }
}
