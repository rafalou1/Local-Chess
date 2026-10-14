use crate::App;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, time::Instant};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub provider: String,
    pub model: String,
    pub local_url: String,
    pub language: String,
    pub automatic: String,
    pub session_limit: u32,
    pub max_tokens: u32,
    pub version: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            provider: "local".into(),
            model: "gemini-3.5-flash-lite".into(),
            local_url: "managed".into(),
            language: "en".into(),
            automatic: "important".into(),
            session_limit: 200,
            max_tokens: 200,
            version: 0,
        }
    }
}
#[derive(Default)]
pub struct Runtime {
    busy: bool,
    requests: u32,
    tokens: u64,
    cache: HashMap<String, Reply>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Reply {
    pub transcript: String,
    pub status: String,
    pub fallback: bool,
    pub elapsed_ms: f64,
    pub requests: u32,
    pub tokens: u64,
}
fn config(a: &App) -> Config {
    let db = a.db.lock().unwrap();
    let s: String = db
        .query_row("SELECT value FROM settings WHERE key='provider'", [], |r| {
            r.get(0)
        })
        .unwrap_or("{}".into());
    serde_json::from_str(&s).unwrap_or_default()
}
fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("org.localchess.coach", "gemini-api-key")
        .map_err(|_| "Windows Credential Manager unavailable".into())
}
fn local(c: &Config) -> Result<String, String> {
    if c.local_url == "managed" {
        return Ok("managed".into());
    }
    let u = reqwest::Url::parse(&c.local_url).map_err(|_| "Invalid local URL")?;
    if u.scheme() != "http"
        || u.host_str() != Some("127.0.0.1")
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err("Local AI must bind to http://127.0.0.1 only".into());
    }
    Ok(c.local_url.trim_end_matches('/').into())
}
#[tauri::command]
pub fn provider_config(a: tauri::State<App>, value: Option<Config>) -> Result<Value, String> {
    let mut c = config(&a);
    if let Some(mut next) = value {
        if !["offline", "local", "gemini"].contains(&next.provider.as_str()) {
            return Err("Unknown provider".into());
        }
        if next.provider == "local" {
            local(&next)?;
        }
        if !next
            .model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return Err("Invalid model ID".into());
        }
        next.version = c.version + 1;
        next.session_limit = next.session_limit.clamp(1, 200);
        next.max_tokens = next.max_tokens.clamp(50, 1000);
        a.db.lock()
            .unwrap()
            .execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES('provider',?1)",
                [serde_json::to_string(&next).unwrap()],
            )
            .map_err(|e| e.to_string())?;
        if next.provider != "local" || next.local_url != "managed" {
            crate::local_ai::stop();
        }
        c = next;
    }
    let rt = a.language.lock().unwrap();
    Ok(
        json!({"config":c,"key_present":entry().and_then(|e|e.get_password().map_err(|_|"No key".into())).is_ok(),"requests":rt.requests,"tokens":rt.tokens}),
    )
}
#[tauri::command]
pub fn provider_key(action: String, key: Option<String>) -> Result<Value, String> {
    let e = entry()?;
    match action.as_str() {
        "save" => {
            let secret = key.ok_or("No key supplied")?;
            if secret.trim().is_empty() {
                return Err("Key is empty".into());
            }
            e.set_password(secret.trim())
                .map_err(|_| "Could not save key in Windows Credential Manager")?;
        }
        "remove" => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err("Could not remove saved credential".into()),
        },
        _ => return Err("Unknown credential action".into()),
    }
    Ok(json!({"saved":action=="save"}))
}
#[tauri::command]
pub async fn provider_test(
    app: tauri::AppHandle,
    a: tauri::State<'_, App>,
) -> Result<String, String> {
    let c = config(&a);
    if c.provider == "offline" {
        return Ok("Offline templates ready; no network required".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| "HTTP initialization failed")?;
    let r = if c.provider == "gemini" {
        let key = entry()?.get_password().map_err(|_| "No Gemini key saved")?;
        client
            .get(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}",
                c.model
            ))
            .header("x-goog-api-key", key)
            .send()
            .await
    } else {
        client
            .get(format!(
                "{}/health",
                if c.local_url == "managed" {
                    crate::local_ai::ensure(&app)?
                } else {
                    local(&c)?
                }
            ))
            .send()
            .await
    }
    .map_err(|_| "Connection failed or timed out; offline templates remain available")?;
    if !r.status().is_success() {
        return Err(format!(
            "Provider returned HTTP {}. Offline templates remain available.",
            r.status().as_u16()
        ));
    }
    Ok(
        "Connection successful. Model access confirmed; explanation quality is not yet validated."
            .into(),
    )
}
pub fn validate(text: &str, facts: &[String]) -> Result<String, String> {
    let v: Value = serde_json::from_str(
        text.trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim(),
    )
    .map_err(|_| "Provider did not return structured evidence references")?;
    let ids = v["verified_fact_ids"]
        .as_array()
        .ok_or("Missing fact references")?;
    if ids.is_empty() || ids.len() > 3 {
        return Err("Select one to three verified facts".into());
    }
    let mut selected = vec![];
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        let i = id.as_u64().ok_or("Invalid fact ID")? as usize;
        if !seen.insert(i) {
            return Err("Duplicate fact reference".into());
        }
        selected.push(facts.get(i).ok_or("Unknown fact ID")?.clone());
    }
    let transcript = selected.join(" ");
    if let Some(summary) = v["summary"].as_str() {
        if summary != transcript {
            return Err("Unverified wording rejected; using offline evidence".into());
        }
    }
    Ok(transcript)
}
#[tauri::command]
pub async fn narrate(
    app: tauri::AppHandle,
    a: tauri::State<'_, App>,
    game_id: String,
    revision: u64,
    node: usize,
    question: String,
) -> Result<Reply, String> {
    let c = config(&a);
    let g = a.game.lock().unwrap().clone();
    if g.id != game_id || g.revision != revision || node >= g.nodes.len() {
        return Err("Stale narration request".into());
    }
    let evidence_node = crate::coach_answers::answer_node(&g, node, &question);
    let evidence = g.nodes[evidence_node].evidence.as_ref();
    let review = question == "Review this move";
    let lang = crate::coach_answers::language(&a);
    let fallback = crate::coach_voice::answer(&g, node, &question, &lang)?;
    // Only conversational, legally verified sentences enter the model bank.
    // Engine SAN/PV details are accessible in Explain, never appended here.
    let facts = crate::coach_voice::wording_choices(&fallback);
    let key = format!(
        "voice-v2:{lang}:{game_id}:{revision}:{node}:{}:{question}",
        serde_json::to_string(&c).unwrap()
    );
    let started = Instant::now();
    {
        let rt = a.language.lock().unwrap();
        if let Some(reply) = rt.cache.get(&key) {
            return Ok(reply.clone());
        }
    }
    let mut reply = Reply {
        transcript: fallback,
        status: "Offline evidence".into(),
        fallback: true,
        elapsed_ms: 0.,
        requests: 0,
        tokens: 0,
    };
    if c.provider == "offline" || evidence.is_none() || (review && c.automatic == "off") {
        return Ok(reply);
    }
    {
        let mut rt = a.language.lock().unwrap();
        if rt.busy || rt.requests >= c.session_limit {
            reply.status = "Provider busy or session limit reached; offline fallback".into();
            return Ok(reply);
        }
        rt.busy = true;
        rt.requests += 1;
    }
    let request=async{
        let prompt=format!("Speak like a friendly chess coach in one short conversational sentence. Choose exactly ONE of the supplied equivalent wording options. Each describes the same verified position in everyday language. Never output moves, coordinates, variations or numeric material counts. Facts are already in the learner's language. Return JSON only: {{\"verified_fact_ids\":[0],\"lead\":\"focus\"}}. Optional lead must be one of none, focus, compare; prefer none. Do not output free-form chess claims or summary. User question: {}. Verified facts: {}.",question.chars().take(300).collect::<String>(),serde_json::to_string(&facts).unwrap());
        let client=reqwest::Client::builder().timeout(std::time::Duration::from_millis(2200)).build().map_err(|_|"HTTP initialization failed".to_string())?;
        let response=if c.provider=="gemini"{
            let secret=entry()?.get_password().map_err(|_|"Missing saved Gemini key".to_string())?;
            client.post(format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",c.model)).header("x-goog-api-key",secret).json(&json!({"contents":[{"parts":[{"text":prompt}]}],"generationConfig":{"maxOutputTokens":100,"temperature":0,"responseMimeType":"application/json"}})).send().await
        }else{
            let url=if c.local_url=="managed"{crate::local_ai::ensure(&app)?}else{local(&c)?};
            let request=client.post(format!("{url}/v1/chat/completions"));
            let request=if c.local_url=="managed"{request.bearer_auth(crate::local_ai::token().ok_or("Local server unavailable")?)}else{request};
            request.json(&json!({"model":"local","messages":[{"role":"user","content":prompt}],"max_tokens":80,"temperature":0,"response_format":{"type":"json_object"}})).send().await
        }.map_err(|_|"Local AI warming, unavailable or exceeded 2.2s; immediate engine explanation retained".to_string())?;
        let status=response.status();if !status.is_success(){return Err(format!("Provider HTTP {}; no automatic retry",status.as_u16()))}
        let v:Value=response.json().await.map_err(|_|"Invalid provider response".to_string())?;
        let text=if c.provider=="gemini"{v["candidates"][0]["content"]["parts"][0]["text"].as_str()}else{v["choices"][0]["message"]["content"].as_str()}.ok_or("No explanation returned")?;
        let transcript=validate(text,&facts)?;
        let parsed:Value=serde_json::from_str(text).map_err(|_|"Invalid JSON")?;
        if !parsed["verified_fact_ids"].as_array().is_some_and(|ids|ids.len()==1){return Err("Use one short conversational message; offline explanation retained".into())}
        let lead=match (parsed["lead"].as_str().unwrap_or("none"),lang.as_str()){("focus","de")=>"Schau genau hin. ",("focus",_)=>"Take a closer look. ",("compare","de")=>"Schauen wir gemeinsam hin. ",("compare",_)=>"Let's look together. ",("none",_)=>"",_=>return Err("Unverified introduction rejected".into())};
        let lead=if ["Careful", "Watch", "Pass auf", "Vorsicht", "Achte auf"].iter().any(|prefix|transcript.starts_with(prefix)){""}else{lead};
        let tokens=v["usageMetadata"]["totalTokenCount"].as_u64().or(v["usage"]["total_tokens"].as_u64()).unwrap_or(0);
        Ok::<_,String>((format!("{lead}{transcript}"),tokens))
    }.await;
    let mut rt = a.language.lock().unwrap();
    rt.busy = false;
    match request {
        Ok((text, tokens)) => {
            rt.tokens += tokens;
            reply.transcript = text;
            reply.status = "Local/provider AI composed verified consequences".into();
            reply.fallback = false
        }
        Err(e) => reply.status = format!("{e}; offline fallback"),
    };
    reply.elapsed_ms = started.elapsed().as_secs_f64() * 1000.;
    reply.requests = rt.requests;
    reply.tokens = rt.tokens;
    drop(rt);
    let now = a.game.lock().unwrap();
    if now.id != game_id
        || now.revision != revision
        || config(&a).version != c.version
        || crate::coach_answers::language(&a) != lang
    {
        return Err("Stale narration discarded".into());
    }
    drop(now);
    let mut rt = a.language.lock().unwrap();
    if rt.cache.len() > 200 {
        rt.cache.clear()
    }
    if !reply.fallback {
        rt.cache.insert(key, reply.clone());
    }
    Ok(reply)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_invented_fact() {
        let facts = vec!["Nf3 develops the knight.".into()];
        assert!(validate(
            r#"{"verified_fact_ids":[0],"summary":"Nf3 wins a queen."}"#,
            &facts
        )
        .is_err());
        assert!(validate(r#"{"verified_fact_ids":[9]}"#, &facts).is_err());
        assert!(validate(
            r#"{"verified_fact_ids":[0],"summary":"Nf3 develops the knight."}"#,
            &facts
        )
        .is_ok())
    }
}
