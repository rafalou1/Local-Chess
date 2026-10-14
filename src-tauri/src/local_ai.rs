use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock},
};
use tauri::Manager;
struct Server {
    child: Child,
    url: String,
    token: String,
    _job: std::os::windows::io::OwnedHandle,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
static SERVER: OnceLock<Mutex<Option<Server>>> = OnceLock::new();
fn slot() -> &'static Mutex<Option<Server>> {
    SERVER.get_or_init(|| Mutex::new(None))
}
pub fn token() -> Option<String> {
    slot().lock().unwrap().as_ref().map(|s| s.token.clone())
}
pub fn stop() {
    *slot().lock().unwrap() = None;
}
fn root(app: &tauri::AppHandle) -> PathBuf {
    let p = app
        .path()
        .resource_dir()
        .unwrap_or_default()
        .join("resources/ai");
    if p.join("llama-server.exe").exists() {
        p
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ai")
    }
}
pub fn ensure(app: &tauri::AppHandle) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let mut guard = slot().lock().unwrap();
    if let Some(s) = guard.as_mut() {
        if s.child.try_wait().map_err(|e| e.to_string())?.is_none() {
            return Ok(s.url.clone());
        }
        *guard = None;
    }
    let model = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("Local app data missing")?)
        .join("Local Chess Coach/models/Qwen3-4B-Instruct-2507-Q4_K_M.gguf");
    if !model.exists() {
        return Err("Local Qwen model missing. Run scripts/Setup-LocalAI.ps1; immediate engine explanation retained.".into());
    }
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(&model).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if format!("{:x}", hash.finalize())
        != "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597"
    {
        return Err("Qwen model checksum mismatch; engine explanation retained".into());
    }
    let sys = sysinfo::System::new_all();
    if sys.available_memory() < 4608 * 1024 * 1024 {
        return Err(
            "Insufficient free memory for optional local AI; engine explanation retained.".into(),
        );
    }
    let executable = root(app).join("llama-server.exe");
    let devices = Command::new(&executable)
        .arg("--list-devices")
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;
    let listing = format!(
        "{}{}",
        String::from_utf8_lossy(&devices.stdout),
        String::from_utf8_lossy(&devices.stderr)
    );
    let device = listing
        .lines()
        .find(|l| l.contains("RX 7800 XT"))
        .and_then(|l| l.trim().split(':').next())
        .ok_or("RX 7800 XT Vulkan device unavailable; engine fallback retained")?
        .to_owned();
    let device_line = listing
        .lines()
        .find(|l| l.contains("RX 7800 XT"))
        .ok_or("GPU unavailable")?;
    let free_mib = device_line
        .split("MiB free")
        .next()
        .and_then(|s| s.split_whitespace().last())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("GPU memory availability unavailable; engine fallback retained")?;
    if free_mib < 6144 {
        return Err("Less than6GiB discrete GPU memory free; engine explanation retained".into());
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    let data = std::env::var_os("LOCAL_CHESS_COACH_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or(app.path().app_data_dir().map_err(|e| e.to_string())?);
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let log = std::fs::File::create(data.join("local-ai.log")).map_err(|e| e.to_string())?;
    let token = uuid::Uuid::new_v4().to_string();
    let child = Command::new(&executable)
        .args([
            "--model",
            model.to_str().ok_or("Invalid model path")?,
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            &token,
            "--ctx-size",
            "4096",
            "--parallel",
            "1",
            "--n-gpu-layers",
            "99",
            "--device",
            &device,
            "--threads",
            "4",
            "--no-webui",
            "--no-warmup",
            "--verbose",
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| e.to_string())?;
    let url = format!("http://127.0.0.1:{port}");
    let job = crate::engine::supervise(&child)?;
    *guard = Some(Server {
        child,
        url: url.clone(),
        token,
        _job: job,
    });
    Ok(url)
}
#[tauri::command]
pub async fn warm_local_ai(app: tauri::AppHandle) -> Result<String, String> {
    let url = tauri::async_runtime::spawn_blocking(move || ensure(&app))
        .await
        .map_err(|e| e.to_string())??;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .map_err(|e| e.to_string())?;
    let began = std::time::Instant::now();
    while began.elapsed().as_secs() < 20 {
        if client
            .get(format!("{url}/health"))
            .bearer_auth(token().unwrap_or_default())
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return Ok("Qwen/Vulkan ready".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    Err("Optional Qwen still warming; engine feedback remains available".into())
}
