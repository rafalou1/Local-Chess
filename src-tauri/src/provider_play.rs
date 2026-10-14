mod personal;
// Official local Codex adapter. No token files, browser cookies or API calls.
// Discovery: app-server JSON-RPC. Moves: isolated `exec --json` with user config ignored.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Selection {
    pub provider: String,
    pub model: String,
    pub display_name: String,
    pub effort: String,
    pub timeout_seconds: u64,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
}
#[derive(Clone, Serialize)]
pub struct Status {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub available: bool,
    pub authenticated: bool,
    pub connection_error: bool,
    pub billing: String,
    pub version: String,
    pub detail: String,
    pub source: String,
    pub models: Vec<Model>,
}
#[derive(Clone, Serialize, Default)]
pub struct Diagnostic {
    pub game_id: String,
    pub model: String,
    pub elapsed_ms: u64,
    pub attempts: usize,
    pub responses: Vec<String>,
    pub prompts: Vec<String>,
    pub raw_outputs: Vec<RawOutput>,
    pub outcome: String,
}
#[derive(Clone, Serialize, Default)]
pub struct RawOutput {
    pub text: String,
    pub truncated: bool,
}
impl RawOutput {
    fn append(&mut self, line: &str) {
        if self.truncated { return; }
        let available = 262144usize.saturating_sub(self.text.len());
        let mut end = line.len().min(available);
        while !line.is_char_boundary(end) { end -= 1; }
        self.text.push_str(&line[..end]);
        self.truncated |= end < line.len();
    }
}
static TASKS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
static LOG: OnceLock<Mutex<Vec<Diagnostic>>> = OnceLock::new();
fn tasks() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}
struct Task {
    key: String,
    cancel: Arc<AtomicBool>,
}
impl Task {
    fn begin(key: String) -> Result<Self, String> {
        let mut t = tasks().lock().unwrap();
        if t.contains_key(&key) {
            return Err("Provider operation already running. Cancel it or wait.".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        t.insert(key.clone(), cancel.clone());
        Ok(Self { key, cancel })
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        tasks().lock().unwrap().remove(&self.key);
    }
}
pub fn stop() {
    personal::close_login_windows();
    if let Some(home) = AGY_HOME.get() {
        home.lock().unwrap().take();
    }
    for t in tasks().lock().unwrap().values() {
        t.store(true, Ordering::SeqCst)
    }
}
#[tauri::command]
pub fn provider_cancel(key: String) {
    if let Some(t) = tasks().lock().unwrap().get(&key) {
        t.store(true, Ordering::SeqCst)
    }
}
#[tauri::command]
pub fn provider_diagnostics(game_id: String) -> Vec<Diagnostic> {
    LOG.get_or_init(|| Mutex::new(vec![]))
        .lock()
        .unwrap()
        .iter()
        .filter(|d| d.game_id == game_id)
        .cloned()
        .collect()
}
fn record(d: Diagnostic) {
    let mut l = LOG.get_or_init(|| Mutex::new(vec![])).lock().unwrap();
    l.push(d);
    if l.len() > 40 {
        l.remove(0);
    }
}
fn working_dir() -> Result<PathBuf, String> {
    let p = std::env::temp_dir()
        .join("local-chess-provider")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&p).map_err(|_| "Cannot create isolated provider working directory")?;
    Ok(p)
}
// Keep the CLI's private initialization/cache state for this app session.
// Recreating USERPROFILE on every metadata command triggers repeated migrations.
struct AgyHome {
    dir: PathBuf,
}
impl Drop for AgyHome {
    fn drop(&mut self) {
        let base = std::env::temp_dir().join("local-chess-provider");
        if self
            .dir
            .canonicalize()
            .ok()
            .zip(base.canonicalize().ok())
            .is_some_and(|(p, b)| p.starts_with(b))
        {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
static AGY_HOME: OnceLock<Mutex<Option<Arc<AgyHome>>>> = OnceLock::new();
fn agy_home() -> Result<Arc<AgyHome>, String> {
    let mut home = AGY_HOME.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if let Some(h) = home.as_ref() {
        return Ok(h.clone());
    }
    let h = Arc::new(AgyHome {
        dir: working_dir()?,
    });
    personal::prepare_agy(&h.dir)?;
    *home = Some(h.clone());
    Ok(h)
}
struct Process {
    child: Child,
    input: Option<ChildStdin>,
    rx: Receiver<Result<String, String>>,
    dir: PathBuf,
    _job: Option<std::os::windows::io::OwnedHandle>,
    _home: Option<Arc<AgyHome>>,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        drop(self._job.take());
        let base = std::env::temp_dir().join("local-chess-provider");
        if self
            .dir
            .canonicalize()
            .ok()
            .zip(base.canonicalize().ok())
            .is_some_and(|(p, b)| p.starts_with(b))
        {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
impl Process {
    fn spawn(exe: &Path, args: &[String]) -> Result<Self, String> {
        Self::spawn_profile(exe, args, false)
    }
    fn spawn_profile(exe: &Path, args: &[String], isolated_home: bool) -> Result<Self, String> {
        use std::os::windows::process::CommandExt;
        let dir = working_dir()?;
        let home = if isolated_home {
            // Custom agents are resolved from the invocation workspace, not
            // the CLI cache home. Each workspace needs the restricted agent.
            personal::prepare_agy(&dir)?;
            Some(agy_home()?)
        } else {
            None
        };
        let mut cmd = Command::new(exe);
        cmd.args(args)
            .current_dir(&dir)
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(home) = &home {
            cmd.env("USERPROFILE", &home.dir);
            // The fresh chess profile has no updater debounce marker. Do not launch
            // a background updater (and its console) on every headless invocation.
            cmd.env("AGY_CLI_DISABLE_AUTO_UPDATE", "true");
        }
        for e in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "ANTHROPIC_PROFILE",
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GOOGLE_GEMINI_BASE_URL",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_USE_FOUNDRY",
            "OPENAI_API_KEY",
            "CODEX_API_KEY",
            "OPENAI_BASE_URL",
            "AZURE_OPENAI_API_KEY",
        ] {
            cmd.env_remove(e);
        }
        if args.iter().any(|arg| arg == "--help") {
            cmd.stderr(Stdio::piped());
        }
        let mut child = cmd.spawn().map_err(|_| {
            "Could not start official provider executable. Refresh detection or reinstall."
        })?;
        let job = match crate::engine::supervise(&child) {
            Ok(j) => j,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let input = child.stdin.take();
        let output = child.stdout.take().ok_or("Provider stdout missing")?;
        let (tx, rx) = mpsc::sync_channel(64);
        if let Some(error_output) = child.stderr.take() {
            let help_tx = tx.clone();
            std::thread::spawn(move || {
                let mut text = String::new();
                if error_output.take(262144).read_to_string(&mut text).is_ok() {
                    for line in text.lines() {
                        if help_tx.send(Ok(line.to_string() + "\n")).is_err() {
                            break;
                        }
                    }
                }
            });
        }
        std::thread::spawn(move || {
            let mut r = BufReader::new(output);
            loop {
                let mut bytes = vec![];
                match r.by_ref().take(262145).read_until(b'\n', &mut bytes) {
                    Ok(0) => break,
                    Ok(_) => {
                        if bytes.len() > 262144 {
                            let _ = tx.send(Err("Provider output exceeded safety limit".into()));
                            break;
                        }
                        let s = String::from_utf8_lossy(&bytes).into_owned();
                        if tx.send(Ok(s)).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(Err("Provider output pipe failed".into()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            rx,
            dir,
            _job: Some(job),
            _home: home,
        })
    }
    fn line(&mut self, deadline: Instant, cancel: &AtomicBool) -> Result<String, String> {
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("Turn cancelled. Position unchanged.".into());
            }
            if Instant::now() >= deadline {
                return Err(
                    "Provider timed out. Position unchanged; retry or switch to local play.".into(),
                );
            }
            match self.rx.recv_timeout(Duration::from_millis(40)){Ok(r)=>return r,Err(mpsc::RecvTimeoutError::Disconnected)=>return Err("Provider process exited before a response. Check sign-in, limits and connection, then retry.".into()),Err(_)=>{}}
        }
    }
    fn send(&mut self, v: &Value) -> Result<(), String> {
        writeln!(self.input.as_mut().ok_or("Provider input closed")?, "{v}")
            .map_err(|_| "Provider input pipe failed".into())
    }
    fn rpc(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        self.send(&json!({"id":id,"method":method,"params":params}))?;
        for _ in 0..2000 {
            let v: Value = serde_json::from_str(&self.line(deadline, cancel)?)
                .map_err(|_| "Provider emitted invalid JSON")?;
            if v.get("id") == Some(&json!(id)) {
                if v.get("error").is_some() {
                    return Err(format!("Official CLI rejected {method}. Update Codex and refresh; no billing fallback was attempted."));
                }
                return Ok(v["result"].clone());
            }
            if v.get("id").is_some() && v.get("method").is_some() {
                return Err(
                    "Unexpected provider request; no tool or credential action was allowed".into(),
                );
            }
        }
        Err("Too many provider events".into())
    }
}
// Direct executables only: never invoke npm .cmd shims through a shell.
fn find_exe(name: &str) -> Option<PathBuf> {
    let mut paths: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(format!("{name}.exe")))
            .collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        paths.push(PathBuf::from(local).join(format!("{name}/bin/{name}.exe")))
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        paths.push(PathBuf::from(home).join(format!(".local/bin/{name}.exe")))
    }
    if name == "codex" {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let base = PathBuf::from(appdata).join("npm/node_modules/@openai");
            for arch in ["x64", "arm64"] {
                let triple = if arch == "x64" { "x86_64" } else { "aarch64" };
                for p in [
                    base.join(format!("codex/node_modules/@openai/codex-win32-{arch}")),
                    base.join(format!("codex-win32-{arch}")),
                    base.join("codex"),
                ] {
                    paths.push(p.join(format!("vendor/{triple}-pc-windows-msvc/bin/codex.exe")))
                }
            }
        }
    }
    paths.into_iter().find(|p| p.is_file())
}
fn simple(
    exe: &Path,
    args: &[String],
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let mut p = Process::spawn(exe, args)?;
    p.input.take();
    let mut out = String::new();
    loop {
        match p.rx.recv_timeout(Duration::from_millis(40)) {
            Ok(line) => {
                out.push_str(&line?);
                if out.len() > 1048576 {
                    return Err("Provider output too large".into());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let code = p.child.wait().map_err(|_| "Provider wait failed")?;
                return if code.success() {
                    Ok(out)
                } else {
                    Err("Official provider command failed. Check installation, sign-in and account limits.".into())
                };
            }
            Err(_) => {}
        }
        if cancel.load(Ordering::SeqCst) {
            return Err("Operation cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("Provider operation timed out".into());
        }
    }
}
fn server(exe: &Path, deadline: Instant, cancel: &AtomicBool) -> Result<Process, String> {
    let mut p = Process::spawn(
        exe,
        &[
            "app-server".into(),
            "-c".into(),
            "features.hooks=false".into(),
        ],
    )?;
    p.rpc(
        1,
        "initialize",
        json!({"clientInfo":{"name":"local_chess_coach","version":"0.2.14"}}),
        deadline,
        cancel,
    )?;
    p.send(&json!({"method":"initialized"}))?;
    Ok(p)
}
fn models(p: &mut Process, deadline: Instant, cancel: &AtomicBool) -> Result<Vec<Model>, String> {
    let mut out = vec![];
    let mut cursor = Value::Null;
    for i in 0..20 {
        let r = p.rpc(
            10 + i,
            "model/list",
            json!({"limit":100,"includeHidden":false,"cursor":cursor}),
            deadline,
            cancel,
        )?;
        for m in r["data"]
            .as_array()
            .ok_or("Model catalog unavailable in this Codex version")?
        {
            if m["hidden"].as_bool() == Some(true) {
                continue;
            }
            let id = m["model"].as_str().ok_or("Missing model id")?.to_string();
            let efforts = m["supportedReasoningEfforts"]
                .as_array()
                .ok_or("Effort catalog missing")?
                .iter()
                .filter_map(|e| e["reasoningEffort"].as_str().map(str::to_owned))
                .collect();
            out.push(Model {
                id,
                name: m["displayName"].as_str().unwrap_or("Codex model").into(),
                efforts,
                default_effort: m["defaultReasoningEffort"].as_str().unwrap_or("").into(),
            });
        }
        cursor = r["nextCursor"].clone();
        if cursor.is_null() {
            return Ok(out);
        }
    }
    Err("Model catalog pagination exceeded limit".into())
}
fn account_type(account: &Value) -> Option<&str> {
    account
        .get("account")
        .and_then(|a| a.get("type"))
        .and_then(Value::as_str)
}
fn codex_status(cancel: &AtomicBool, deadline: Instant) -> Status {
    let mut s = Status {
        id: "codex".into(),
        name: "Codex".into(),
        installed: false,
        available: false,
        authenticated: false,
        connection_error: false,
        billing: "Sign in with ChatGPT · plan limits apply".into(),
        version: String::new(),
        detail: "Install the official Codex CLI to play.".into(),
        source: "https://developers.openai.com/codex/cli".into(),
        models: vec![],
    };
    let Some(exe) = find_exe("codex") else {
        return s;
    };
    s.installed = true;
    let r = (|| {
        s.version = simple(&exe, &["--version".into()], deadline, cancel)?
            .trim()
            .into();
        let help = simple(&exe, &["exec".into(), "--help".into()], deadline, cancel)?;
        if ![
            "--ignore-user-config",
            "--ignore-rules",
            "--ephemeral",
            "--json",
        ]
        .iter()
        .all(|f| help.contains(f))
        {
            return Err(
                "Installed Codex lacks required isolated execution flags. Update the official CLI."
                    .into(),
            );
        }
        let mut p = server(&exe, deadline, cancel)?;
        let account = p.rpc(
            2,
            "account/read",
            json!({"refreshToken":false}),
            deadline,
            cancel,
        )?;
        match account_type(&account) {
            Some("chatgpt") => {
                s.authenticated = true;
                s.billing = format!(
                    "ChatGPT {} · existing plan usage; no API fallback",
                    account["account"]["planType"].as_str().unwrap_or("plan")
                );
            }
            None => {
                s.detail = "Sign in with ChatGPT using the official Codex flow.".into();
                return Ok(());
            }
            _ => {
                s.billing = "API / other billing detected · blocked".into();
                s.detail="This mode requires ChatGPT sign-in. API and cloud-provider credentials are not used.".into();
                return Ok(());
            }
        }
        s.models = models(&mut p, deadline, cancel)?;
        s.available = !s.models.is_empty();
        s.detail="Ready through your local Codex CLI. Catalog comes from the installed CLI; availability is confirmed when a turn runs.".into();
        Ok::<(), String>(())
    })();
    if let Err(e) = r {
        s.detail = e;
        s.connection_error = true;
    }
    s
}
#[tauri::command]
pub async fn provider_discover() -> Result<Vec<Status>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let t = Task::begin("discovery".into())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        std::thread::scope(|scope| {
            let c = scope.spawn(|| codex_status(&t.cancel, deadline));
            let a = scope.spawn(|| personal::claude_status(&t.cancel, deadline));
            let g = scope.spawn(|| personal::agy_status(&t.cancel, deadline));
            let statuses = vec![
                c.join().map_err(|_| "Codex discovery failed")?,
                a.join().map_err(|_| "Claude discovery failed")?,
                g.join().map_err(|_| "Antigravity discovery failed")?,
            ];
            for status in &statuses {
                cache_status(status);
            }
            Ok(statuses)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
static CATALOG_CACHE: OnceLock<Mutex<HashMap<String, (Instant, std::time::SystemTime, Status)>>> =
    OnceLock::new();
fn provider_exe(id: &str) -> Option<PathBuf> {
    find_exe(match id {
        "claude" => "claude",
        "antigravity" => "agy",
        _ => "codex",
    })
}
fn cache_status(status: &Status) {
    if status.connection_error {
        return;
    }
    if let Some(exe) = provider_exe(&status.id) {
        if let Ok(time) = std::fs::metadata(exe).and_then(|m| m.modified()) {
            CATALOG_CACHE
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap()
                .insert(status.id.clone(), (Instant::now(), time, status.clone()));
        }
    }
}
fn cached_status(id: &str) -> Option<Status> {
    let exe = provider_exe(id)?;
    let modified = std::fs::metadata(exe).ok()?.modified().ok()?;
    let cache = CATALOG_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let (start, time, status) = cache.get(id)?;
    if start.elapsed() < Duration::from_secs(300) && *time == modified {
        Some(status.clone())
    } else {
        None
    }
}
fn ensure_selection(s: &Selection, cancel: &AtomicBool, deadline: Instant) -> Result<(), String> {
    let status = match cached_status(&s.provider) {
        Some(status) => status,
        None => match s.provider.as_str() {
            "codex" => codex_status(cancel, deadline),
            "claude" => personal::claude_status(cancel, deadline),
            "antigravity" => personal::agy_status(cancel, deadline),
            _ => return Err("Provider unavailable".into()),
        },
    };
    cache_status(&status);
    if !status.available {
        return Err(status.detail);
    }
    let m = status
        .models
        .iter()
        .find(|m| m.id == s.model)
        .ok_or("Selected model no longer appears in the installed CLI. Refresh models.")?;
    if !(m.efforts.contains(&s.effort) || (m.efforts.is_empty() && s.effort.is_empty())) {
        return Err("Selected effort is no longer supported. Refresh models.".into());
    }
    Ok(())
}
pub fn validate_selection(s: &Selection) -> Result<(), String> {
    if !(10..=90).contains(&s.timeout_seconds) {
        return Err("Turn timeout must be 10–90 seconds".into());
    }
    ensure_selection(
        s,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(20),
    )
}
fn parse_move(raw: &str, legal: &[String]) -> Result<String, String> {
    let s = raw.trim();
    let b = s.as_bytes();
    if !(b.len() == 4 || b.len() == 5)
        || !(b[0] >= b'a'
            && b[0] <= b'h'
            && b[1] >= b'1'
            && b[1] <= b'8'
            && b[2] >= b'a'
            && b[2] <= b'h'
            && b[3] >= b'1'
            && b[3] <= b'8')
        || (b.len() == 5 && !b"qrbn".contains(&b[4]))
    {
        return Err("Malformed response: expected exactly one UCI move".into());
    }
    if !legal.iter().any(|m| m == s) {
        return Err("Illegal move for the current position".into());
    }
    Ok(s.into())
}
fn exec_args(s: &Selection) -> Vec<String> {
    let mut args = vec![
        "exec",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--ephemeral",
        "--json",
        "--sandbox",
        "read-only",
        "--color",
        "never",
        "-m",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    args.push(s.model.clone());
    for cfg in [
        format!("model_reasoning_effort=\"{}\"", s.effort),
        "forced_login_method=\"chatgpt\"".into(),
        "model_provider=\"openai\"".into(),
        "features.shell_tool=false".into(),
        "approval_policy=\"never\"".into(),
        "features.apply_patch_freeform=false".into(),
        "features.multi_agent=false".into(),
        "features.hooks=false".into(),
        "features.plugins=false".into(),
        "features.apps=false".into(),
        "features.memories=false".into(),
        "web_search=\"disabled\"".into(),
        "project_doc_max_bytes=0".into(),
    ] {
        args.push("-c".into());
        args.push(cfg);
    }
    args.push("-".into());
    args
}
fn harmless_disabled_host(item: &Value) -> bool {
    item["type"] == "error"
        && item["message"].as_str().is_some_and(|m| {
            m.starts_with("Code Mode is unavailable because code-mode host is disabled.")
                && m.contains("fail closed")
        })
}
fn exec_move(
    s: &Selection,
    prompt: &str,
    deadline: Instant,
    cancel: &AtomicBool,
    raw: &mut RawOutput,
) -> Result<String, String> {
    let exe = find_exe("codex").ok_or("Codex CLI is missing")?;
    let feature_list = simple(&exe, &["features".into(), "list".into()], deadline, cancel)?;
    let mut args = exec_args(s);
    for name in [
        "view_image",
        "computer_use",
        "browser_use",
        "browser_use_external",
        "image_generation",
        "code_mode",
        "code_mode_host",
        "workspace_dependencies",
        "tool_suggest",
        "skill_mcp_dependency_install",
        "goals",
        "sleep_tool",
    ] {
        if feature_list
            .lines()
            .any(|line| line.split_whitespace().next() == Some(name))
        {
            args.insert(args.len() - 1, "-c".into());
            args.insert(args.len() - 1, format!("features.{name}=false"));
        }
    }
    if !feature_list
        .lines()
        .any(|line| line.split_whitespace().next() == Some("view_image"))
    {
        args.insert(args.len() - 1, "-c".into());
        args.insert(args.len() - 1, "tools.view_image=false".into());
    }
    let mut p = Process::spawn(&exe, &args)?;
    let mut input = p.input.take().ok_or("Provider input unavailable")?;
    input
        .write_all(prompt.as_bytes())
        .map_err(|_| "Provider prompt write failed")?;
    drop(input);
    let mut response = None;
    for _ in 0..2000 {
        let line = p.line(deadline, cancel)?;
        raw.append(&line);
        let v: Value = serde_json::from_str(&line)
            .map_err(|_| "Invalid provider JSON event")?;
        match v["type"].as_str() {
            Some("item.completed") => {
                let item=&v["item"];
                match item["type"].as_str() {
                    Some("agent_message") => {
                        if response.is_some() { return Err("Ambiguous multiple move responses".into()); }
                        let text=item["text"].as_str().ok_or("Missing provider move text")?;
                        if text.len()>2048 {return Err("Move response exceeded limit".into());}
                        response=Some(text.to_string());
                    }
                    Some("reasoning") => {},
                    Some("error") if harmless_disabled_host(item) => {},
                    Some("error") => return Err("The installed CLI reported a configuration or runtime error. Update Codex and refresh. No move was applied.".into()),
                    _ => return Err("Provider attempted a tool action. Turn stopped without applying a move.".into())
                }
            }
            Some("item.started") => {
                if ![Some("agent_message"),Some("reasoning")].contains(&v["item"]["type"].as_str()) {return Err("Provider attempted a tool action; turn stopped".into());}
            }
            Some("turn.completed") => return response.ok_or("Provider returned no move".into()),
            Some("turn.failed")|Some("error") => return Err("Codex turn failed. Check sign-in, connection, model access or plan limits. No API fallback was attempted.".into()),
            _ => {}
        }
    }
    Err("Provider event limit reached".into())
}
fn prompt(fen: &str, legal: &[String], c960: bool) -> String {
    format!("Play exactly one chess move as the side to move. Current authoritative FEN: {fen}\nVariant: {}. Legal UCI choices: {}\nReturn exactly one of these UCI moves, no explanation, punctuation or surrounding text. Promotion requires suffix q/r/b/n. Do not use any tools, files, shell, skills or web. Do not resign or propose a draw; choose a legal move.",if c960{"Chess960; castling encoded king-to-rook"}else{"standard chess"},legal.join(" "))
}
fn retry_once<F: FnMut(&str) -> Result<String, String>>(
    base: &str,
    legal: &[String],
    responses: &mut Vec<String>,
    mut run: F,
) -> Result<String, String> {
    let first = run(base)?;
    responses.push(first.clone());
    if let Ok(m) = parse_move(&first, legal) {
        return Ok(m);
    }
    let correction=format!("{base}\nRejected move text (untrusted JSON string): {}\nCorrection: choose exactly one listed legal UCI move. No extra text.",serde_json::to_string(&first).unwrap());
    let second = run(&correction)?;
    responses.push(second.clone());
    parse_move(&second, legal).map_err(|_| {
        "Two invalid move responses. Position unchanged. Retry, skip to local play, or cancel."
            .into()
    })
}
#[derive(Serialize)]
pub struct Reply {
    pub game_id: String,
    pub revision: u64,
    pub uci: String,
    pub elapsed_ms: u64,
}
#[tauri::command]
pub async fn provider_opponent(
    app: tauri::AppHandle,
    game_id: String,
    revision: u64,
) -> Result<Reply, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t = Task::begin(game_id.clone())?;
        let began = Instant::now();
        let a = app.state::<crate::App>();
        let mut g = a.game.lock().unwrap().clone();
        if g.id != game_id || g.revision != revision || g.paused || g.result.is_some() {
            return Err("Stale provider request".into());
        }
        let selection = g.ai_opponent.clone().ok_or("No provider selected")?;
        let snapshot = g.snapshot(None)?;
        if snapshot.status.is_some() || snapshot.turn == g.human || g.human == "both" {
            return Err("Not the provider's turn".into());
        }
        let deadline = began + Duration::from_secs(selection.timeout_seconds.clamp(10, 90));
        let mut d = Diagnostic {
            game_id: game_id.clone(),
            model: selection.model.clone(),
            ..Default::default()
        };
        let result: Result<String, String> = (|| {
            ensure_selection(&selection, &t.cancel, deadline)?;
            let text = prompt(&snapshot.fen, &snapshot.legal, g.chess960);
            let mv = retry_once(&text, &snapshot.legal, &mut d.responses, |p| {
                d.prompts.push(p.to_owned());
                d.raw_outputs.push(RawOutput::default());
                let raw = d.raw_outputs.last_mut().unwrap();
                if selection.provider == "codex" {
                    exec_move(&selection, p, deadline, &t.cancel, raw)
                } else {
                    personal::move_text(&selection, p, deadline, &t.cancel, raw)
                }
            })?;
            if t.cancel.load(Ordering::SeqCst) {
                return Err("Turn cancelled".into());
            }
            let mut current = a.game.lock().unwrap();
            let now = current.snapshot(None)?;
            if current.id != game_id
                || current.revision != revision
                || now.paused
                || now.status.is_some()
                || now.fen != snapshot.fen
            {
                return Err("Position changed; provider response discarded".into());
            }
            parse_move(&mv, &now.legal)?;
            Ok(mv)
        })();
        d.elapsed_ms = began.elapsed().as_millis() as u64;
        d.attempts = d.prompts.len();
        d.outcome = if result.is_ok() {
            "Legal move validated".into()
        } else {
            result.as_ref().unwrap_err().clone()
        };
        record(d.clone());
        result.map(|uci| Reply {
            game_id,
            revision,
            uci,
            elapsed_ms: d.elapsed_ms,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn provider_login(
    app: tauri::AppHandle,
    provider: Option<String>,
) -> Result<String, String> {
    let id = provider.unwrap_or_else(|| "codex".into());
    if id != "codex" {
        if !["claude", "antigravity"].contains(&id.as_str()) {
            return Err("Unknown provider".into());
        }
        return personal::login(&id);
    }
    tauri::async_runtime::spawn_blocking(move||{let t=Task::begin("login".into())?;let exe=find_exe("codex").ok_or("Install Codex first")?;let deadline=Instant::now()+Duration::from_secs(300);let mut p=server(&exe,deadline,&t.cancel)?;let r=p.rpc(3,"account/login/start",json!({"type":"chatgpt"}),deadline,&t.cancel)?;let url=r["authUrl"].as_str().ok_or("CLI returned no sign-in URL")?;let u=reqwest::Url::parse(url).map_err(|_|"Invalid sign-in URL")?;if u.scheme()!="https"||![Some("auth.openai.com"),Some("auth0.openai.com"),Some("auth.chatgpt.com")].contains(&u.host_str()){return Err("Unexpected official login host; sign in with codex login in your terminal instead".into())}open_browser(url)?;let _=app.emit("provider-setup",json!({"stage":"Complete sign-in in your browser. The official CLI owns your credentials."}));loop{let v:Value=serde_json::from_str(&p.line(deadline,&t.cancel)?).map_err(|_|"Invalid login event")?;if v["method"]=="account/login/completed"{return if v["params"]["success"]==true{Ok("Codex sign-in complete. Refreshing models…".into())}else{Err("Official sign-in did not complete. Try again.".into())}}}}).await.map_err(|e|e.to_string())?
}
fn open_browser(url: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let explorer =
        PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
            .join("explorer.exe");
    Command::new(explorer)
        .arg(url)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|_| "Could not open official provider page")?;
    Ok(())
}
#[tauri::command]
pub fn provider_source(provider: String) -> Result<(), String> {
    let url = match provider.as_str() {
        "codex" => "https://developers.openai.com/codex/cli",
        "claude" => "https://code.claude.com/docs/en/quickstart",
        "antigravity" => "https://antigravity.google/docs/cli/headless/",
        _ => return Err("Unknown provider".into()),
    };
    open_browser(url)
}
#[tauri::command]
pub async fn provider_install(
    app: tauri::AppHandle,
    provider: Option<String>,
) -> Result<String, String> {
    let id = provider.unwrap_or_else(|| "codex".into());
    if id != "codex" {
        return tauri::async_runtime::spawn_blocking(move || {
            let t = Task::begin(format!("install:{id}"))?;
            personal::install(&app, &id, &t.cancel)
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move||{let t=Task::begin("install".into())?;let node=find_exe("node").ok_or("Install Node.js LTS from nodejs.org, then retry the official npm installation.")?;let npm=node.parent().unwrap().join("node_modules/npm/bin/npm-cli.js");if !npm.is_file(){return Err("npm CLI not found beside Node.js. Install Node.js LTS with npm and retry.".into())}let _=app.emit("provider-setup",json!({"stage":"Installing @openai/codex from the npm registry. This may take a few minutes…"}));simple(&node,&[npm.to_string_lossy().into_owned(),"install".into(),"-g".into(),"@openai/codex".into(),"--registry=https://registry.npmjs.org".into(),"--no-audit".into(),"--no-fund".into()],Instant::now()+Duration::from_secs(180),&t.cancel)?;if find_exe("codex").is_none(){return Err("npm finished, but a supported native Codex binary was not found. Restart the app or check npm's global prefix.".into())}Ok("Codex installed. Sign in with ChatGPT next.".into())}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub fn provider_local(
    a: tauri::State<crate::App>,
    game_id: String,
    revision: u64,
    skip: bool,
) -> Result<crate::Snapshot, String> {
    provider_cancel(game_id.clone());
    let mut g = a.game.lock().unwrap();
    if g.id != game_id || g.revision != revision {
        return Err("Stale game".into());
    }
    g.ai_opponent = None;
    if !skip {
        g.human = "both".into()
    }
    g.revision += 1;
    crate::save(&a, &g)?;
    g.snapshot(None)
}
#[cfg(test)]
mod tests {
    #[test]
    fn raw_output_preserves_events_and_bounds_utf8() {
        let mut raw = super::RawOutput::default();
        let event = "{\"event\":\"result\",\"response\":\"e2e4\"}\r\n";
        raw.append(event);
        assert_eq!(raw.text, event);
        assert!(!raw.truncated);
        raw.append(&"é".repeat(262144));
        assert!(raw.truncated);
        assert!(raw.text.len() <= 262144);
        let before = raw.text.clone();
        raw.append("another event");
        assert_eq!(raw.text, before);
        assert!(raw.text.len() <= 262144);
    }
    use super::*;
    #[test]
    fn harmless_cli_diagnostic_is_not_a_move_or_tool() {
        assert!(harmless_disabled_host(
            &json!({"type":"error","message":"Code Mode is unavailable because code-mode host is disabled. Code mode will fail closed; enable host."})
        ));
        assert!(!harmless_disabled_host(
            &json!({"type":"error","message":"unrecognized safety configuration"})
        ));
        assert!(!harmless_disabled_host(
            &json!({"type":"command_execution","message":"Code Mode is unavailable because code-mode host is disabled. fail closed"})
        ));
    }
    #[test]
    fn strict_untrusted_text() {
        let l = vec!["e2e4".into()];
        assert_eq!(parse_move(" e2e4\n", &l).unwrap(), "e2e4");
        for s in [
            "e2e5",
            "e2e4 e7e5",
            "```e2e4```",
            "Move: e2e4",
            "E2E4",
            "e2e4; calc",
            "a",
            "é2e4",
            "resign",
        ] {
            assert!(parse_move(s, &l).is_err(), "{s}")
        }
    }
    #[test]
    fn single_correction() {
        let l = vec!["e2e4".into()];
        let mut r = vec![];
        let mut calls = 0;
        assert_eq!(
            retry_once("position", &l, &mut r, |p| {
                calls += 1;
                if calls == 1 {
                    Ok("e2e5".into())
                } else {
                    assert!(p.contains("Rejected"));
                    Ok("e2e4".into())
                }
            })
            .unwrap(),
            "e2e4"
        );
        assert_eq!(calls, 2);
        r.clear();
        calls = 0;
        assert!(retry_once("position", &l, &mut r, |_| {
            calls += 1;
            Ok("e2e5".into())
        })
        .is_err());
        assert_eq!(calls, 2);
        r.clear();
        calls = 0;
        assert!(retry_once("position", &l, &mut r, |_| {
            calls += 1;
            Err("process died".into())
        })
        .is_err());
        assert_eq!(calls, 1);
    }
    #[test]
    fn chess_specials() {
        for (fen, m) in [
            ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1g1"),
            ("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7a8n"),
            ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 2", "e5d6"),
        ] {
            let mut g = crate::game::Game::new(fen.into(), "b".into(), 400, 0., 0., false).unwrap();
            let s = g.snapshot(None).unwrap();
            let mv = parse_move(m, &s.legal).unwrap();
            g.commit(&mv, g.revision, false).unwrap();
            assert_eq!(g.live, 1)
        }
    }
    #[test]
    fn cancelled_and_timeout_process() {
        let exe = PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/ping.exe");
        let args = vec!["-n".into(), "10".into(), "127.0.0.1".into()];
        assert!(simple(
            &exe,
            &args,
            Instant::now() + Duration::from_millis(100),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("timed out"));
        assert!(simple(
            &exe,
            &args,
            Instant::now() + Duration::from_secs(10),
            &AtomicBool::new(true)
        )
        .unwrap_err()
        .contains("cancelled"));
        assert!(simple(
            &exe,
            &["--invalid-argument".into()],
            Instant::now() + Duration::from_secs(5),
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    fn unavailable_and_no_fallback() {
        let s = Selection {
            provider: "claude".into(),
            model: "invented".into(),
            display_name: "x".into(),
            effort: "low".into(),
            timeout_seconds: 30,
        };
        assert!(validate_selection(&s).is_err());
        let args = exec_args(&s);
        assert!(args.contains(&"forced_login_method=\"chatgpt\"".into()));
        assert!(args.contains(&"--ignore-user-config".into()));
    }
    #[test]
    fn auth_states_never_treat_api_as_subscription() {
        assert_eq!(account_type(&json!({"account":null})), None);
        assert_eq!(
            account_type(&json!({"account":{"type":"chatgpt","planType":"plus"}})),
            Some("chatgpt")
        );
        for mode in ["apiKey", "amazonBedrock", "chatgptAuthTokens"] {
            assert_ne!(
                account_type(&json!({"account":{"type":mode}})),
                Some("chatgpt")
            );
        }
    }
    #[test]
    fn provider_game_roundtrip_and_old_save() {
        let mut g =
            crate::game::Game::new(crate::game::START.into(), "b".into(), 400, 0., 0., false)
                .unwrap();
        g.ai_opponent = Some(Selection {
            provider: "codex".into(),
            model: "catalog-model".into(),
            display_name: "Model".into(),
            effort: "low".into(),
            timeout_seconds: 30,
        });
        let mut j = serde_json::to_value(&g).unwrap();
        let saved: crate::game::Game = serde_json::from_value(j.clone()).unwrap();
        assert_eq!(saved.ai_opponent.unwrap().model, "catalog-model");
        j.as_object_mut().unwrap().remove("ai_opponent");
        let old: crate::game::Game = serde_json::from_value(j).unwrap();
        assert!(old.ai_opponent.is_none());
    }
    #[test]
    fn auth_catalog_mapping_without_secrets() {
        let cancel = AtomicBool::new(false);
        let s = codex_status(&cancel, Instant::now());
        assert!(!s.available);
        assert!(!s.authenticated);
        assert!(s.models.is_empty());
    }
}
