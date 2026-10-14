use super::*;

fn capture(
    exe: &Path,
    args: &[String],
    deadline: Instant,
    cancel: &AtomicBool,
    isolated: bool,
) -> Result<(String, bool), String> {
    let envelope = args
        .windows(2)
        .any(|a| a[0] == "--output-format" && a[1] == "json");
    capture_inner(exe, args, deadline, cancel, isolated, envelope)
}
fn capture_inner(
    exe: &Path,
    args: &[String],
    deadline: Instant,
    cancel: &AtomicBool,
    isolated: bool,
    envelope: bool,
) -> Result<(String, bool), String> {
    let mut p = Process::spawn_profile(exe, args, isolated)?;
    p.input.take();
    let mut out = String::new();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Operation cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("Provider operation timed out".into());
        }
        match p.rx.recv_timeout(Duration::from_millis(40)) {
            Ok(line) => {
                out.push_str(&line?);
                if out.len() > 1048576 {
                    return Err("Provider response too large".into());
                }
                if envelope {
                    if let Ok(v) = serde_json::from_str::<Value>(&out) {
                        if let Some(status) = v["status"].as_str() {
                            // A JSON result is terminal even if CLI shutdown is
                            // still waiting for its background migration worker.
                            return Ok((out, status == "SUCCESS"));
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(status) = p.child.try_wait().map_err(|_| "Provider wait failed")? {
                    return Ok((out, status.success()));
                }
                // Do not replace the bounded deadline with an unbounded wait().
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(_) => {}
        }
    }
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}
fn base_status(id: &str, name: &str, exe: &str, source: &str) -> Status {
    Status {
        id: id.into(),
        name: name.into(),
        installed: find_exe(exe).is_some(),
        available: false,
        authenticated: false,
        connection_error: false,
        billing: "Account usage · no API fallback".into(),
        version: String::new(),
        detail: format!("Install {name} to get started."),
        source: source.into(),
        models: vec![],
    }
}
fn claude_auth(v: &Value) -> bool {
    v["loggedIn"] == true && v["authMethod"] == "claude.ai" && v["apiProvider"] == "firstParty"
}
fn claude_args() -> Vec<String> {
    strings(&["--print","--input-format","stream-json","--output-format","stream-json","--verbose","--tools","","--disallowedTools","*","--strict-mcp-config","--mcp-config","{\"mcpServers\":{}}","--setting-sources","","--settings","{\"disableAllHooks\":true,\"autoMemoryEnabled\":false,\"forceLoginMethod\":\"claudeai\"}","--disable-slash-commands","--no-session-persistence","--system-prompt","You are a chess opponent. Return exactly one listed legal UCI move. No tools or explanations."])
}
fn claude_models(v: &Value) -> Result<Vec<Model>, String> {
    let rows = v.as_array().ok_or("Claude model catalog unavailable")?;
    let mut out = vec![];
    for m in rows {
        let id = m["value"].as_str().ok_or("Missing Claude model id")?;
        let resolved = m["resolvedModel"].as_str().unwrap_or(id);
        let efforts = if m["supportsEffort"] == true {
            m["supportedEffortLevels"]
                .as_array()
                .ok_or("Missing Claude effort catalog")?
                .iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let name = m["displayName"].as_str().unwrap_or(id);
        let version = resolved
            .strip_prefix("claude-")
            .unwrap_or(resolved)
            .rsplit('-')
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(".");
        out.push(Model {
            id: id.into(),
            name: format!("{name} {version}"),
            default_effort: if efforts.iter().any(|e| e == "low") {
                "low".into()
            } else {
                efforts.first().cloned().unwrap_or_default()
            },
            efforts,
        });
    }
    Ok(out)
}
fn claude_init(p: &mut Process, deadline: Instant, cancel: &AtomicBool) -> Result<Value, String> {
    p.send(&json!({"type":"control_request","request_id":"catalog","request":{"subtype":"initialize"}}))?;
    for _ in 0..100 {
        let v: Value = serde_json::from_str(&p.line(deadline, cancel)?)
            .map_err(|_| "Invalid Claude catalog event")?;
        if v["type"] == "control_response" && v["response"]["request_id"] == "catalog" {
            if v["response"]["subtype"] != "success" {
                return Err("Claude catalog initialization failed".into());
            }
            return Ok(v["response"]["response"].clone());
        }
    }
    Err("Claude catalog event limit".into())
}
pub(super) fn claude_status(cancel: &AtomicBool, deadline: Instant) -> Status {
    let mut s = base_status(
        "claude",
        "Claude Code",
        "claude",
        "https://code.claude.com/docs/en/quickstart",
    );
    let Some(exe) = find_exe("claude") else {
        return s;
    };
    let r = (|| {
        s.version = capture(&exe, &strings(&["--version"]), deadline, cancel, false)?
            .0
            .trim()
            .into();
        let help = capture(&exe, &strings(&["--help"]), deadline, cancel, false)?.0;
        if ![
            "--tools",
            "--no-session-persistence",
            "--setting-sources",
            "--strict-mcp-config",
        ]
        .iter()
        .all(|k| help.contains(k))
        {
            return Err("Update Claude Code for isolated chess mode.".into());
        }
        let (text, _) = capture(&exe, &strings(&["auth", "status"]), deadline, cancel, false)?;
        let auth: Value =
            serde_json::from_str(&text).map_err(|_| "Claude authentication status unavailable")?;
        s.authenticated = claude_auth(&auth);
        s.billing = if s.authenticated {
            "Claude subscription · account limits apply".into()
        } else if auth["loggedIn"] == true {
            "API / cloud credentials detected · blocked".into()
        } else {
            "Sign in with your Claude subscription".into()
        };
        let mut p = Process::spawn(&exe, &claude_args())?;
        let catalog = claude_init(&mut p, deadline, cancel)?;
        s.models = claude_models(&catalog["models"])?;
        s.available = s.authenticated && !s.models.is_empty();
        s.detail = if s.available {
            "Ready".into()
        } else if auth["loggedIn"] == true {
            "Use Claude subscription sign-in; API and cloud routes are disabled.".into()
        } else {
            "Sign in with Claude to play.".into()
        };
        Ok::<(), String>(())
    })();
    if let Err(e) = r {
        s.detail = e;
        s.connection_error = true;
    }
    s
}

fn agy_command(
    exe: &Path,
    name: &str,
    model: Option<&str>,
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    let mut args = strings(&[
        "-p",
        name,
        "--output-format",
        "json",
        "--print-timeout",
        "12s",
    ]);
    if let Some(m) = model {
        args.extend(strings(&["--model", m]));
    }
    let (text, _) = capture(exe, &args, deadline, cancel, true)?;
    let v: Value = serde_json::from_str(text.trim())
        .map_err(|_| "Antigravity returned an invalid metadata envelope")?;
    if v["status"] != "SUCCESS" {
        return Err("Antigravity could not check the connection. Retry, or use Sign in in Options if needed.".into());
    }
    Ok(v)
}
fn agy_model_rows(text: &str) -> Vec<Model> {
    text.lines()
        .filter_map(|l| {
            let (id, label) = l.split_once('\t')?;
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
            {
                return None;
            }
            Some(Model {
                id: id.into(),
                name: label.trim().into(),
                efforts: vec![],
                default_effort: String::new(),
            })
        })
        .collect()
}
// Effort variants are first-class, pinned IDs in the native model catalog.
// Avoid /effort: this version can wait indefinitely in its settings command.
fn agy_catalog(text: &str, help: &str) -> Vec<Model> {
    let mut models = agy_model_rows(text);
    let levels = help
        .lines()
        .find(|l| l.contains("--effort"))
        .and_then(|l| l.split_once('('))
        .and_then(|(_, tail)| tail.split_once(')'))
        .map(|(values, _)| {
            values
                .split('|')
                .map(str::trim)
                .filter(|e| !e.is_empty() && e.bytes().all(|b| b.is_ascii_alphanumeric()))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let ids = models.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
    for model in &mut models {
        if let Some((base, current)) = model.id.rsplit_once('-') {
            if !levels.iter().any(|e| e == current) {
                continue;
            }
            let options = levels
                .iter()
                .filter(|effort| ids.iter().any(|id| id == &format!("{base}-{effort}")))
                .cloned()
                .collect::<Vec<_>>();
            // One fixed variant does not establish an adjustable effort setting.
            if options.len() > 1 {
                model.default_effort = current.into();
                model.efforts = options;
            }
        }
    }
    models
}
fn agy_pinned_model(selection: &Selection) -> String {
    if !selection.effort.is_empty() {
        if let Some((base, _)) = selection.model.rsplit_once('-') {
            // ensure_selection has verified that this exact sibling exists.
            return format!("{base}-{}", selection.effort);
        }
    }
    selection.model.clone()
}
pub(super) fn agy_status(cancel: &AtomicBool, deadline: Instant) -> Status {
    let mut s = base_status(
        "antigravity",
        "Antigravity",
        "agy",
        "https://antigravity.google/docs/cli/install/",
    );
    let Some(exe) = find_exe("agy") else { return s };
    let r = (|| {
        s.version = capture(&exe, &strings(&["--version"]), deadline, cancel, true)?
            .0
            .trim()
            .into();
        let help = capture(&exe, &strings(&["--help"]), deadline, cancel, true)?.0;
        if ![
            "--input-format",
            "--sandbox",
            "--agent",
            "--disable-slash-commands",
        ]
        .iter()
        .all(|k| help.contains(k))
        {
            return Err("Update Antigravity CLI for isolated chess mode.".into());
        }
        // The profile is created by us with no API route and stripped API env.
        // /config can hang in this CLI even in print mode; it is unnecessary
        // for authentication and must not block play or imply a lost sign-in.
        let attempt_deadline = deadline.min(Instant::now() + Duration::from_secs(8));
        let usage = match agy_command(&exe, "/usage", None, attempt_deadline, cancel) {
            Ok(v) => v,
            Err(e)
                if !cancel.load(Ordering::SeqCst)
                    && e.contains("timed out")
                    && Instant::now() < deadline =>
            {
                agy_command(&exe, "/usage", None, deadline, cancel)?
            }
            Err(e) => return Err(e),
        };
        s.authenticated = usage["command"]["name"] == "usage"
            && usage["command"]["data"]["groups"]
                .as_array()
                .is_some_and(|g| !g.is_empty());
        let (text, success) = capture(&exe, &strings(&["models"]), deadline, cancel, true)?;
        if !success {
            return Err("Antigravity model discovery failed. Sign in or refresh.".into());
        }
        s.models = agy_catalog(&text, &help);
        s.available = s.authenticated && !s.models.is_empty();
        s.billing = if s.authenticated {
            "Google account allowance · extra credits disabled".into()
        } else {
            "Sign in with your Google account".into()
        };
        s.detail = if s.available {
            "Ready".into()
        } else {
            "Sign in with Google to play.".into()
        };
        Ok::<(), String>(())
    })();
    if let Err(e) = r {
        s.detail = e;
        s.connection_error = true;
    }
    s
}

fn claude_response(v: &Value) -> Result<Option<String>, String> {
    match v["type"].as_str() {
        Some("control_request") => Err(
            "Claude requested an action; no tools or configuration changes were allowed.".into(),
        ),
        Some("assistant") => {
            if v["message"]["content"]
                .as_array()
                .is_some_and(|a| a.iter().any(|i| i["type"] == "tool_use"))
            {
                Err("Claude attempted a tool action; turn cancelled.".into())
            } else {
                Ok(None)
            }
        }
        Some("system") if v["subtype"] == "init" => {
            let tools = v["tools"]
                .as_array()
                .ok_or("Claude tool isolation could not be verified")?;
            if tools.iter().any(|t| t.as_str() != Some("EndConversation")) {
                Err("Claude exposed a tool in isolated chess mode; turn stopped.".into())
            } else {
                Ok(None)
            }
        }
        Some("result") => {
            if v["is_error"] == true || v["subtype"] != "success" {
                Err("Claude turn failed. Check sign-in, model access and account limits.".into())
            } else {
                Ok(Some(
                    v["result"]
                        .as_str()
                        .ok_or("Claude returned no move text")?
                        .into(),
                ))
            }
        }
        _ => Ok(None),
    }
}
fn agy_turn_error(result: &Value) -> String {
    let error = result["error"].as_str().unwrap_or("").to_ascii_lowercase();
    if error.contains("unknown agent") {
        "Antigravity could not load the chess profile. Restart the app and retry."
    } else if error.contains("quota") || error.contains("resource_exhausted") || error.contains("rate limit") {
        "Antigravity's usage limit was reached. Try another model, wait for the limit to reset, or switch to local play."
    } else if error.contains("unauthenticated") || error.contains("not authenticated") || error.contains("sign in") {
        "Antigravity needs Google sign-in. Use the official sign-in window, then refresh."
    } else if error.contains("not recognized") || error.contains("model is not available") {
        "This Antigravity model is unavailable. Refresh models or select another one."
    } else {
        "Antigravity could not finish the turn. Retry or check the provider's connection and account limits."
    }.into()
}
pub(super) fn move_text(
    s: &Selection,
    prompt: &str,
    deadline: Instant,
    cancel: &AtomicBool,
    raw: &mut RawOutput,
) -> Result<String, String> {
    if s.provider == "claude" {
        let exe = find_exe("claude").ok_or("Install Claude Code first")?;
        let (auth_text, _) = capture(&exe, &strings(&["auth", "status"]), deadline, cancel, false)?;
        let auth: Value =
            serde_json::from_str(&auth_text).map_err(|_| "Claude sign-in check failed")?;
        if !claude_auth(&auth) {
            return Err(
                "Sign in with your Claude subscription. API/cloud credentials are blocked.".into(),
            );
        }
        let mut args = claude_args();
        args.extend(strings(&["--model", &s.model]));
        if !s.effort.is_empty() {
            args.extend(strings(&["--effort", &s.effort]));
        }
        let mut p = Process::spawn(&exe, &args)?;
        claude_init(&mut p, deadline, cancel)?;
        p.send(&json!({"type":"user","message":{"role":"user","content":prompt},"parent_tool_use_id":null,"session_id":"chess"}))?;
        p.input.take();
        for _ in 0..2000 {
            let line = p.line(deadline, cancel)?;
            raw.append(&line);
            let v: Value = serde_json::from_str(&line)
                .map_err(|_| "Invalid Claude move event")?;
            if let Some(text) = claude_response(&v)? {
                if text.len() > 2048 {
                    return Err("Claude response too large".into());
                }
                return Ok(text);
            }
        }
        return Err("Claude event limit reached".into());
    }
    let exe = find_exe("agy").ok_or("Install Antigravity first")?;
    let pinned_model = agy_pinned_model(s);
    let args = strings(&[
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--agent",
        "local-chess",
        "--mode",
        "plan",
        "--sandbox",
        "--disable-slash-commands",
        "--model",
        &pinned_model,
        "--print-timeout",
        &format!("{}s", s.timeout_seconds),
    ]);
    let mut p = Process::spawn_profile(&exe, &args, true)?;
    p.send(&json!({"event":"user","message":{"content":prompt}}))?;
    p.input.take();
    for _ in 0..2000 {
        let line = p.line(deadline, cancel)?;
        raw.append(&line);
        let v: Value = serde_json::from_str(&line)
            .map_err(|_| "Invalid Antigravity move event")?;
        if v["event"] == "init" && v["init"]["permission_mode"] != "strict" {
            return Err("Antigravity did not honor the isolated permission profile".into());
        }
        if v["event"] == "step_update" && v["step_update"]["step_type"] == "tool" {
            return Err("Antigravity attempted a tool action; turn stopped.".into());
        }
        if v["event"] == "result" {
            let r = &v["result"];
            if r["status"] != "SUCCESS" {
                return Err(agy_turn_error(r));
            }
            let text = r["response"]
                .as_str()
                .ok_or("Antigravity returned no move text")?;
            if text.len() > 2048 {
                return Err("Antigravity response too large".into());
            }
            return Ok(text.into());
        }
    }
    Err("Antigravity event limit reached".into())
}

pub(super) fn prepare_agy(dir: &Path) -> Result<(), String> {
    let profile = dir.join(".gemini/antigravity-cli");
    let agents = dir.join(".agents/agents");
    std::fs::create_dir_all(&profile).map_err(|_| "Cannot create isolated provider settings")?;
    std::fs::create_dir_all(&agents).map_err(|_| "Cannot create isolated chess agent")?;
    std::fs::write(profile.join("settings.json"),json!({"toolPermission":"strict","allowNonWorkspaceAccess":false,"enableTerminalSandbox":true,"useG1Credits":false,"modelProvider":"","permissions":{"deny":["command(*)","read_file(*)","write_file(*)","mcp(*)","read_url(*)","execute_url(*)"]}}).to_string()).map_err(|_|"Cannot save isolated provider settings")?;
    std::fs::write(agents.join("local-chess.md"),"---\nname: local-chess\ndescription: Choose one legal chess move only.\nmainAgent: true\nsubagent: false\ntools: [finish]\nmcpServers: []\nskills: []\nplugins: []\ninheritCustomizations: false\ninheritMcp: false\ncommandExecutionPolicy: off\n---\nReturn exactly one supplied legal UCI move. No tools, files, shell, web, explanations or resignation.\n").map_err(|_|"Cannot save isolated chess agent")?;
    Ok(())
}

pub(super) fn install(
    app: &tauri::AppHandle,
    id: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let script = match id {
        "claude" => "Install-Claude.ps1",
        "antigravity" => "Install-Antigravity.ps1",
        _ => return Err("Unknown provider".into()),
    };
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|_| "Missing application resources")?
        .join("resources/providers")
        .join(script);
    let path = if bundled.is_file() {
        bundled
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/providers")
            .join(script)
    };
    if !path.is_file() {
        return Err("Provider installer resource missing. Reinstall Local Chess Coach.".into());
    }
    let ps = PathBuf::from(std::env::var_os("WINDIR").ok_or("Windows directory missing")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut args = strings(&[
        "-NoLogo",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]);
    args.push(path.to_string_lossy().into());
    if id == "antigravity" {
        args.push("--skip-aliases".into())
    }
    let _ = app.emit(
        "provider-setup",
        json!({"provider":id,"stage":"Downloading and installing the official CLI…"}),
    );
    let (_, ok) = capture(
        &ps,
        &args,
        Instant::now() + Duration::from_secs(240),
        cancel,
        false,
    )?;
    if !ok {
        return Err(
            "Official installer failed. Check your connection and the installation details.".into(),
        );
    }
    let exe = if id == "claude" { "claude" } else { "agy" };
    if find_exe(exe).is_none() {
        return Err(
            "Installer finished but the CLI was not found. Restart the app and retry.".into(),
        );
    }
    Ok("Installed. You can sign in now.".into())
}

struct LoginWindow {
    child: Child,
}
impl Drop for LoginWindow {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
static WINDOWS: OnceLock<Mutex<Vec<LoginWindow>>> = OnceLock::new();
pub(super) fn close_login_windows() {
    if let Some(w) = WINDOWS.get() {
        w.lock().unwrap().clear()
    }
}
pub(super) fn login(id: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let exe = find_exe(if id == "claude" { "claude" } else { "agy" })
        .ok_or("Install this provider first")?;
    let mut cmd = Command::new(exe);
    if id == "claude" {
        cmd.args(["auth", "login", "--claudeai"]);
    }
    cmd.creation_flags(0x00000010);
    for k in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
    ] {
        cmd.env_remove(k);
    }
    let child = cmd
        .spawn()
        .map_err(|_| "Could not open the official sign-in window")?;
    let mut windows = WINDOWS.get_or_init(|| Mutex::new(vec![])).lock().unwrap();
    windows.retain_mut(|w| w.child.try_wait().ok().flatten().is_none());
    windows.push(LoginWindow { child });
    Ok("Finish in the official window, then press Refresh.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_metadata_does_not_wait_for_slow_shutdown() {
        use base64::Engine as _;
        let exe = PathBuf::from(std::env::var_os("WINDIR").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script = "Write-Output '{\"status\":\"SUCCESS\"}'; Start-Sleep -Seconds 6";
        let bytes = script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let start = Instant::now();
        let (out, ok) = capture_inner(
            &exe,
            &strings(&["-NoProfile", "-EncodedCommand", &encoded]),
            Instant::now() + Duration::from_secs(3),
            &AtomicBool::new(false),
            false,
            true,
        )
        .unwrap();
        assert!(ok);
        assert_eq!(
            serde_json::from_str::<Value>(&out).unwrap()["status"],
            "SUCCESS"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn agy_effort_variants_are_catalog_bound() {
        let models = agy_catalog("gemini-3.8-flash-high\tGemini High\ngemini-3.8-flash-low\tGemini Low\ngpt-oss-120b-medium\tGPT Medium\n", "  --effort Reasoning effort (low|medium|high|xhigh|max)");
        assert_eq!(models[0].efforts, vec!["low", "high"]);
        assert_eq!(models[0].default_effort, "high");
        assert!(models[2].efforts.is_empty());
        assert!(!models[0].efforts.iter().any(|e| e == "medium"));
        assert!(agy_catalog(
            "sample-high\tSample\nsample-low\tSample Low\n",
            "--effort changed interface"
        )[0]
        .efforts
        .is_empty());
        let s = Selection {
            provider: "antigravity".into(),
            model: models[0].id.clone(),
            display_name: models[0].name.clone(),
            effort: "low".into(),
            timeout_seconds: 30,
        };
        assert_eq!(agy_pinned_model(&s), "gemini-3.8-flash-low");
    }
    #[test]
    fn claude_auth_is_subscription_only() {
        assert!(claude_auth(
            &json!({"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty"})
        ));
        for method in [
            "none",
            "api_key",
            "api_key_helper",
            "oauth_token",
            "third_party",
        ] {
            assert!(!claude_auth(
                &json!({"loggedIn":true,"authMethod":method,"apiProvider":"firstParty"})
            ))
        }
    }
    #[test]
    fn dynamic_claude_models_and_effort() {
        let v = json!([{"value":"sonnet","resolvedModel":"claude-sonnet-5-5","displayName":"Sonnet","supportsEffort":true,"supportedEffortLevels":["low","medium"]},{"value":"small","displayName":"Small","supportsEffort":false}]);
        let m = claude_models(&v).unwrap();
        assert_eq!(m[0].name, "Sonnet 5.5");
        assert_eq!(m[0].efforts, vec!["low", "medium"]);
        assert!(m[1].efforts.is_empty())
    }
    #[test]
    fn agy_model_table_is_not_a_terminal_scrape() {
        let m = agy_model_rows(
            "Fetching available models...\ngemini-3.8-flash-low\tGemini X (Low)\nclaude-y\tClaude Y\n",
        );
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].id, "gemini-3.8-flash-low")
    }
    #[test]
    fn claude_structured_result_and_tools() {
        assert_eq!(
            claude_response(
                &json!({"type":"result","subtype":"success","is_error":false,"result":"e2e4"})
            )
            .unwrap(),
            Some("e2e4".into())
        );
        assert!(
            claude_response(&json!({"type":"system","subtype":"init","tools":["Bash"]})).is_err()
        );
        assert!(claude_response(
            &json!({"type":"assistant","message":{"content":[{"type":"tool_use"}]}})
        )
        .is_err());
        assert!(claude_response(&json!({"type":"result","is_error":true})).is_err())
    }
}
