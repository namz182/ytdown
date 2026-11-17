use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub max_concurrent: usize,
    pub default_quality: String,
    pub download_folder: String,
    pub theme: String,
    pub notifications: bool,
    pub auto_close: bool,
    pub auto_close_delay: u64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            max_concurrent: 3,
            default_quality: "720p (HD)".into(),
            download_folder: String::new(),
            theme: "System".into(),
            notifications: true,
            auto_close: false,
            auto_close_delay: 5,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub thumbnail: Option<String>,
    pub quality: String,
    pub folder: String,
    pub playlist_start: u32,
    pub status: String,
    pub progress: f64,
    #[serde(default)]
    pub downloaded_bytes: u64,
    #[serde(default)]
    pub total_bytes: Option<u64>,
    #[serde(default)]
    pub dismissed: bool,
    #[serde(default)]
    pub phase: String,
    pub speed: String,
    pub eta: String,
    pub error: String,
    pub created: u64,
    #[serde(default)]
    pub completed_at: Option<u64>,
    pub files: Vec<String>,
    pub log: Vec<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    settings: Preferences,
    tasks: Vec<Task>,
}
struct Inner {
    updating: bool,
    stopping: bool,
    store: Store,
    children: HashMap<String, Arc<Mutex<Child>>>,
}
pub struct Service {
    inner: Arc<Mutex<Inner>>,
    data: PathBuf,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn persist(service: &Service, inner: &Inner) -> Result<(), String> {
    let temp = service.data.join("state.tmp");
    fs::write(
        &temp,
        serde_json::to_vec_pretty(&inner.store).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temp, service.data.join("state.json")).map_err(|e| e.to_string())
}
impl Service {
    pub fn new(app: &tauri::AppHandle) -> Result<Self, String> {
        let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        fs::create_dir_all(&data).map_err(|e| e.to_string())?;
        let mut store: Store = match fs::read(data.join("state.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| format!("Cannot read saved downloads: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::default(),
            Err(e) => return Err(e.to_string()),
        };
        store.settings.default_quality = normalize_quality(&store.settings.default_quality);
        for task in &mut store.tasks {
            task.quality = normalize_quality(&task.quality);
        }
        if store.settings.download_folder.is_empty() {
            store.settings.download_folder = app
                .path()
                .download_dir()
                .or_else(|_| app.path().home_dir().map(|p| p.join("Downloads")))
                .map_err(|e| e.to_string())?
                .join("YT_DOWN")
                .to_string_lossy()
                .into_owned();
        }
        for task in &mut store.tasks {
            if matches!(task.status.as_str(), "Downloading" | "Paused") {
                task.status = "Failed".into();
                task.error =
                    "Download interrupted when the application closed. Retry to resume.".into();
            }
        }
        let service = Self {
            inner: Arc::new(Mutex::new(Inner {
                updating: false,
                stopping: false,
                store,
                children: HashMap::new(),
            })),
            data,
        };
        {
            let inner = service.inner.lock().unwrap();
            persist(&service, &inner)?;
        }
        Ok(service)
    }
}
fn downloader(service: &Service) -> PathBuf {
    let managed = service.data.join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });
    if managed.is_file() {
        managed
    } else {
        let name = if cfg!(windows) {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        };
        let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default();
        if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
            paths.push(PathBuf::from(home).join(".local").join("bin"));
        }
        paths
            .into_iter()
            .map(|path| path.join(name))
            .find(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from(name))
    }
}
async fn ensure_downloader(app: tauri::AppHandle) -> Result<(), String> {
    if tool_status(app.clone()).await?["version"]
        .as_str()
        .is_none()
    {
        update_downloader(app).await.map_err(|error| format!("Could not install yt-dlp: {error}. Check your connection and try again, or install it in Settings → Updates."))?;
    }
    Ok(())
}
fn command(service: &Service) -> Command {
    let mut cmd = Command::new(downloader(service));
    cmd.env("PYTHONUNBUFFERED", "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}
fn valid_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Enter a valid YouTube URL.")?;
    let host = parsed.host_str().unwrap_or_default();
    let is_youtube = host == "youtu.be"
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com");
    if !matches!(parsed.scheme(), "https" | "http") || !is_youtube {
        return Err("Use a YouTube video or playlist URL.".into());
    }
    Ok(())
}
fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("downloads-changed", ());
}
fn update_task(app: &tauri::AppHandle, id: &str, change: impl FnOnce(&mut Task)) {
    let service = app.state::<Service>();
    let mut inner = service.inner.lock().unwrap();
    if let Some(task) = inner.store.tasks.iter_mut().find(|t| t.id == id) {
        change(task);
    }
    let _ = persist(&service, &inner);
    drop(inner);
    changed(app);
}
#[tauri::command]
pub fn get_state(service: tauri::State<Service>) -> Value {
    let inner = service.inner.lock().unwrap();
    json!({ "settings": inner.store.settings, "tasks": inner.store.tasks })
}
#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    service: tauri::State<Service>,
    mut settings: Preferences,
) -> Result<(), String> {
    settings.default_quality = normalize_quality(&settings.default_quality);
    settings.max_concurrent = settings.max_concurrent.clamp(1, 10);
    settings.auto_close_delay = settings.auto_close_delay.clamp(1, 120);
    if !Path::new(&settings.download_folder).is_absolute() {
        return Err("Choose an absolute download folder.".into());
    }
    fs::create_dir_all(&settings.download_folder).map_err(|e| e.to_string())?;
    {
        let mut inner = service.inner.lock().unwrap();
        inner.store.settings = settings;
        persist(&service, &inner)?;
    }
    schedule(app);
    Ok(())
}
#[tauri::command]
pub async fn enqueue_download(
    app: tauri::AppHandle,
    url: String,
    quality: String,
    playlist_start: u32,
    title: Option<String>,
    thumbnail: Option<String>,
) -> Result<Task, String> {
    valid_url(&url)?;
    let quality = normalize_quality(&quality);
    if ![
        "480p (SD)",
        "720p (HD)",
        "1080p (Full HD)",
        "Best available",
        "Audio only (MP3)",
    ]
    .contains(&quality.as_str())
    {
        return Err("Choose a supported quality.".into());
    }
    // Prepare the dependency before creating a task, so a fresh installation
    // never produces a failed history entry just because yt-dlp is missing.
    ensure_downloader(app.clone()).await?;
    let service = app.state::<Service>();
    let task;
    {
        let mut inner = service.inner.lock().unwrap();
        if inner.store.tasks.iter().any(|t| {
            t.url == url && matches!(t.status.as_str(), "Queued" | "Downloading" | "Paused")
        }) {
            return Err("This URL is already in the queue.".into());
        }
        let folder = inner.store.settings.download_folder.clone();
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        task = Task {
            id: format!("{}-{}", now(), inner.store.tasks.len()),
            url: url.clone(),
            title: title
                .filter(|title| !title.trim().is_empty())
                .unwrap_or(url),
            thumbnail: thumbnail.filter(|url| url.starts_with("https://")),
            quality,
            folder,
            playlist_start: playlist_start.max(1),
            status: "Queued".into(),
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: None,
            dismissed: false,
            phase: String::new(),
            speed: String::new(),
            eta: String::new(),
            error: String::new(),
            created: now(),
            completed_at: None,
            files: vec![],
            log: vec![],
        };
        inner.store.tasks.insert(0, task.clone());
        persist(&service, &inner)?;
    }
    changed(&app);
    // Create the progress window before the worker starts, including for queued jobs.
    if let Err(error) = show_download(app.clone(), task.id.clone()).await {
        let _ = app.emit(
            "download-window-error",
            format!("Download queued, but its progress window could not open: {error}"),
        );
    }
    schedule(app);
    Ok(task)
}

#[tauri::command]
pub async fn inspect_playlist(app: tauri::AppHandle, url: String) -> Result<Value, String> {
    valid_url(&url)?;
    ensure_downloader(app.clone()).await?;
    let service = app.state::<Service>();
    let executable = downloader(&service);
    let output = tauri::async_runtime::spawn_blocking(move || {
        Command::new(executable)
            .args(["--ignore-config", "--flat-playlist", "--dump-single-json", "--no-warnings", &url])
            .output()
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("Could not inspect this link: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let info: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "yt-dlp returned unreadable playlist information.".to_string())?;
    let count = info.get("entries").and_then(Value::as_array).map_or(0, Vec::len);
    Ok(json!({
        "isPlaylist": count > 1,
        "title": info.get("title").and_then(Value::as_str).unwrap_or("Playlist"),
        "totalItems": count,
    }))
}

pub async fn enqueue_browser_link(app: tauri::AppHandle, url: String) -> Result<Task, String> {
    let quality = app
        .state::<Service>()
        .inner
        .lock()
        .unwrap()
        .store
        .settings
        .default_quality
        .clone();
    enqueue_download(app, url, quality, 1, None, None).await
}
fn take_queued(store: &mut Store) -> Vec<Task> {
    let running = store
        .tasks
        .iter()
        .filter(|t| matches!(t.status.as_str(), "Downloading" | "Paused"))
        .count();
    let capacity = store.settings.max_concurrent.saturating_sub(running);
    let tasks: Vec<Task> = store
        .tasks
        .iter_mut()
        .rev()
        .filter(|t| t.status == "Queued")
        .take(capacity)
        .map(|t| {
            t.status = "Downloading".into();
            t.clone()
        })
        .collect();
    tasks
}
pub fn schedule(app: tauri::AppHandle) {
    let service = app.state::<Service>();
    let mut inner = service.inner.lock().unwrap();
    if inner.stopping || inner.updating {
        return;
    }
    let tasks = take_queued(&mut inner.store);
    let _ = persist(&service, &inner);
    drop(inner);
    for task in tasks {
        let app = app.clone();
        std::thread::spawn(move || run_download(app, task));
    }
}
fn normalize_quality(quality: &str) -> String {
    if matches!(quality, "Audio only (MP3)" | "Audio only (MP4)") {
        "Audio only (MP3)".into()
    } else { quality.into() }
}
fn download_arguments(task: &Task) -> Vec<String> {
    let mut args: Vec<String> = [
        "--ignore-config",
        "--newline",
        "--no-colors",
        "--no-simulate",
        "--progress",
        "--progress-delta",
        "1",
        "--continue",
        "--windows-filenames",
        "--no-write-info-json",
        "--no-write-thumbnail",
        "--no-write-playlist-metafiles",
        "--no-mtime",
        "--no-overwrites",
        "--playlist-start",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.extend([task.playlist_start.to_string(), "-P".into(), task.folder.clone(), "-o".into(), "%(playlist_title|.)s/%(title)s.%(ext)s".into(), "--print".into(), "before_dl:YT_TITLE:%(title)s".into(), "--print".into(), "after_move:YT_FILE:%(filepath)s".into(), "--progress-template".into(), "download:YT_PROGRESS:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s|%(progress.downloaded_bytes)s|%(progress.total_bytes,progress.total_bytes_estimate)s".into()]);
    let audio_only = task.quality.starts_with("Audio only");
    let format = if audio_only { "ba/b".into() } else {
        let height = match task.quality.as_str() {
            "480p (SD)" => Some(480),
            "720p (HD)" => Some(720),
            "1080p (Full HD)" => Some(1080),
            _ => None,
        };
        height.map(|h| format!("bv*[ext=mp4][height<={h}]+ba[ext=m4a]/b[ext=mp4][height<={h}]/bv*[height<={h}]+ba/b[height<={h}]")).unwrap_or_else(|| "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b".into())
    };
    args.extend(["-f".into(), format]);
    if audio_only {
        args.extend(["-x".into(), "--audio-format".into(), "mp3".into(), "--audio-quality".into(), "192K".into()]);
    } else {
        args.extend(["--merge-output-format".into(), "mp4".into(), "--recode-video".into(), "mp4".into()]);
    }
    args
}
fn apply_progress(task: &mut Task, line: &str) {
    let parts: Vec<_> = line.split('|').collect();
    task.progress = parts[0]
        .trim()
        .trim_end_matches('%')
        .parse::<f64>()
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    task.speed = parts.get(1).unwrap_or(&"").trim().into();
    task.eta = parts.get(2).unwrap_or(&"").trim().into();
    task.downloaded_bytes = parts
        .get(3)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    task.total_bytes = parts.get(4).and_then(|v| v.trim().parse().ok());
    task.phase.clear();
}
fn run_download(app: tauri::AppHandle, task: Task) {
    let service = app.state::<Service>();
    if service.inner.lock().unwrap().stopping {
        return;
    }
    let mut cmd = command(&service);
    cmd.args(download_arguments(&task));
    cmd.arg("--")
        .arg(&task.url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            update_task(&app, &task.id, |t| {
                if t.status != "Cancelled" {
                    t.status = "Failed".into();
                    t.error =
                        format!("Cannot start yt-dlp: {e}. Install it in Settings → Updates.");
                }
            });
            schedule(app.clone());
            return;
        }
    };
    let mut child = child;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let child = Arc::new(Mutex::new(child));
    {
        let mut inner = service.inner.lock().unwrap();
        if inner.stopping
            || !inner
                .store
                .tasks
                .iter()
                .any(|t| t.id == task.id && matches!(t.status.as_str(), "Downloading" | "Paused"))
        {
            let mut c = child.lock().unwrap();
            let _ = kill_process(&mut c);
            let _ = c.wait();
            return;
        }
        inner.children.insert(task.id.clone(), child.clone());
    }
    let app_err = app.clone();
    let id_err = task.id.clone();
    let errors = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            update_task(&app_err, &id_err, |t| {
                if line.starts_with("ERROR:") {
                    t.error = line.clone();
                }
                t.log.push(line);
                if t.log.len() > 100 {
                    t.log.remove(0);
                }
            });
        }
    });
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        update_task(&app, &task.id, |t| {
            if t.status == "Cancelled" {
                return;
            }
            if let Some(title) = line.strip_prefix("YT_TITLE:") {
                t.title = title.into();
                t.progress = 0.0;
            } else if let Some(file) = line.strip_prefix("YT_FILE:") {
                if !t.files.contains(&file.to_string()) {
                    t.files.push(file.into());
                }
            } else if let Some(progress) = line.strip_prefix("YT_PROGRESS:") {
                apply_progress(t, progress);
            } else if line.starts_with("[ExtractAudio]") || line.starts_with("[AudioConvertor]") {
                t.phase = "Processing MP3".into();
            } else if line.starts_with("[Merger]") || line.starts_with("[VideoConvertor]") {
                t.phase = "Processing MP4".into();
            }
            t.log.push(line);
            if t.log.len() > 100 {
                t.log.remove(0);
            }
        });
    }
    let result = child.lock().unwrap().wait();
    let _ = errors.join();
    let (settings, completed, title) = {
        let mut inner = service.inner.lock().unwrap();
        inner.children.remove(&task.id);
        let Some(t) = inner.store.tasks.iter_mut().find(|t| t.id == task.id) else {
            drop(inner);
            schedule(app.clone());
            return;
        };
        if t.status != "Cancelled" {
            if result.is_ok_and(|s| s.success()) {
                t.status = "Completed".into();
                t.completed_at = Some(now());
                let size: u64 = t
                    .files
                    .iter()
                    .filter_map(|path| fs::metadata(path).ok())
                    .map(|metadata| metadata.len())
                    .sum();
                if size > 0 {
                    t.downloaded_bytes = size;
                    t.total_bytes = Some(size);
                }
                t.phase.clear();
                t.progress = 100.0;
                t.error.clear();
            } else {
                t.status = "Failed".into();
                if t.error.is_empty() {
                    t.error =
                        "Download failed. Check the progress window for the download log.".into();
                }
            }
        }
        let completed = t.status == "Completed";
        let title = t.title.clone();
        let settings = inner.store.settings.clone();
        let _ = persist(&service, &inner);
        (settings, completed, title)
    };
    changed(&app);
    if settings.notifications {
        notify(
            if completed {
                "Download complete"
            } else {
                "Download stopped"
            },
            &title,
        );
    }
    if completed && settings.auto_close {
        let app = app.clone();
        let label = format!("download-{}", task.id);
        let delay = settings.auto_close_delay;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(delay));
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.close();
            }
        });
    }
    schedule(app.clone());
}
fn kill_process(child: &mut Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        // Each downloader owns a process group, including its FFmpeg workers.
        let result = unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        if result != 0 {
            return child.kill().map_err(|e| e.to_string());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            child.kill().map_err(|e| e.to_string())
        }
    }
}
fn notify(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("notify-send")
            .args(["--app-name=YT_DOWN", "--", title, body])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification {} with title {}",
            json!(body),
            json!(title)
        );
        let _ = Command::new("osascript").args(["-e", &script]).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let escaped = body.replace('\'', "''");
        let escaped_title = title.replace('\'', "''");
        let _ = Command::new("powershell").args(["-NoProfile", "-Command", &format!("Add-Type -AssemblyName System.Windows.Forms; $n = New-Object System.Windows.Forms.NotifyIcon; $n.Icon = [System.Drawing.SystemIcons]::Information; $n.Visible = $true; $n.ShowBalloonTip(5000, '{escaped_title}', '{escaped}', [System.Windows.Forms.ToolTipIcon]::Info); Start-Sleep -Seconds 6; $n.Dispose()")]).spawn();
    }
}
#[tauri::command]
pub fn cancel_download(
    app: tauri::AppHandle,
    service: tauri::State<Service>,
    id: String,
) -> Result<(), String> {
    let child = {
        let mut inner = service.inner.lock().unwrap();
        let task = inner
            .store
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("Download not found.")?;
        if !matches!(task.status.as_str(), "Queued" | "Downloading" | "Paused") {
            return Ok(());
        }
        task.status = "Cancelled".into();
        persist(&service, &inner)?;
        inner.children.get(&id).cloned()
    };
    if let Some(child) = child {
        kill_process(&mut child.lock().unwrap())?;
    }
    changed(&app);
    schedule(app);
    Ok(())
}
#[tauri::command]
pub fn clear_history(app: tauri::AppHandle, service: tauri::State<Service>) -> Result<(), String> {
    {
        let mut inner = service.inner.lock().unwrap();
        inner
            .store
            .tasks
            .retain(|t| matches!(t.status.as_str(), "Queued" | "Downloading" | "Paused"));
        persist(&service, &inner)?;
    }
    changed(&app);
    Ok(())
}
#[tauri::command]
pub async fn show_download(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let service = app.state::<Service>();
    if !service
        .inner
        .lock()
        .unwrap()
        .store
        .tasks
        .iter()
        .any(|t| t.id == id)
    {
        return Err("Download not found.".into());
    }
    let label = format!("download-{id}");
    if let Some(window) = app.get_webview_window(&label) {
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(format!("index.html?download={id}").into()),
    )
    .title("YT_DOWN · Download")
    .inner_size(500.0, 280.0)
    .min_inner_size(440.0, 260.0)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn show_updates(app: tauri::AppHandle) -> Result<(), String> {
    const LABEL: &str = "updates";
    if let Some(window) = app.get_webview_window(LABEL) {
        window.show().map_err(|error| error.to_string())?;
        return window.set_focus().map_err(|error| error.to_string());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        LABEL,
        tauri::WebviewUrl::App("index.html?updates=1".into()),
    )
    .title("YT_DOWN · Updates")
    .inner_size(540.0, 410.0)
    .min_inner_size(480.0, 340.0)
    .build()
    .map_err(|error| error.to_string())?;
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    path: String,
    name: String,
    size: u64,
    modified: u64,
    kind: String,
    playlist: String,
}
fn scan(root: &Path, dir: &Path, result: &mut Vec<Media>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            scan(root, &path, result)?;
        } else {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ![
                "mp4", "mkv", "webm", "mp3", "m4a", "wav", "ogg", "flac", "mov",
            ]
            .contains(&ext.as_str())
            {
                continue;
            }
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            let playlist = path
                .parent()
                .unwrap()
                .strip_prefix(root)
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .into_owned();
            result.push(Media {
                path: path.to_string_lossy().into_owned(),
                name: path.file_name().unwrap().to_string_lossy().into_owned(),
                size: meta.len(),
                modified: meta
                    .modified()
                    .unwrap_or(UNIX_EPOCH)
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                kind: ext,
                playlist: if playlist == "." {
                    String::new()
                } else {
                    playlist
                },
            });
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn list_media(app: tauri::AppHandle) -> Result<Vec<Media>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<Service>();
        let folder = service
            .inner
            .lock()
            .unwrap()
            .store
            .settings
            .download_folder
            .clone();
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let mut result = vec![];
        scan(Path::new(&folder), Path::new(&folder), &mut result)?;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn media_path(service: &Service, path: &str) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let inner = service.inner.lock().unwrap();
    let mut roots = vec![PathBuf::from(&inner.store.settings.download_folder)];
    for task in &inner.store.tasks {
        if task
            .files
            .iter()
            .any(|file| fs::canonicalize(file).is_ok_and(|p| p == path))
        {
            roots.push(PathBuf::from(&task.folder));
        }
    }
    if !path.is_file()
        || !roots
            .into_iter()
            .any(|root| fs::canonicalize(root).is_ok_and(|root| path.starts_with(root)))
    {
        return Err("Select a downloaded file.".into());
    }
    Ok(path)
}
fn rename_file(path: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.ends_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
    {
        return Err("Enter a filename without slashes or special characters.".into());
    }
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .ok_or("File has no extension.")?;
    let destination = path.with_file_name(format!("{name}.{extension}"));
    if destination == path {
        return Ok(destination);
    }
    // Linking first prevents overwriting a different existing file, including races.
    fs::hard_link(path, &destination).map_err(|e| format!("Could not rename file: {e}"))?;
    if let Err(error) = fs::remove_file(path) {
        let _ = fs::remove_file(&destination);
        return Err(error.to_string());
    }
    Ok(destination)
}
#[tauri::command]
pub fn rename_media(app: tauri::AppHandle, path: String, name: String) -> Result<(), String> {
    let service = app.state::<Service>();
    let source = media_path(&service, &path)?;
    {
        let inner = service.inner.lock().unwrap();
        if inner.store.tasks.iter().any(|t| {
            matches!(t.status.as_str(), "Queued" | "Downloading" | "Paused")
                && t.files.contains(&path)
        }) {
            return Err("Wait for this download to finish before renaming it.".into());
        }
    }
    let destination = rename_file(&source, &name)?;
    let mut inner = service.inner.lock().unwrap();
    for task in &mut inner.store.tasks {
        let single = task.files.len() == 1;
        for file in &mut task.files {
            if Path::new(file) == source || *file == path {
                *file = destination.to_string_lossy().into_owned();
                if single {
                    task.title = name.trim().into();
                }
            }
        }
    }
    persist(&service, &inner)?;
    drop(inner);
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn clear_queue(app: tauri::AppHandle) -> Result<(), String> {
    let service = app.state::<Service>();
    let mut inner = service.inner.lock().unwrap();
    for task in &mut inner.store.tasks {
        if !matches!(task.status.as_str(), "Queued" | "Downloading" | "Paused") {
            task.dismissed = true;
        }
    }
    persist(&service, &inner)?;
    drop(inner);
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn delete_download(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let service = app.state::<Service>();
    let files = {
        let inner = service.inner.lock().unwrap();
        let task = inner
            .store
            .tasks
            .iter()
            .find(|t| t.id == id)
            .ok_or("Download not found.")?;
        if matches!(task.status.as_str(), "Queued" | "Downloading" | "Paused") {
            return Err("Cancel the download before deleting its files.".into());
        }
        task.files.clone()
    };
    // Validate every file before removing any; missing files have already been deleted.
    let paths: Vec<_> = files
        .iter()
        .filter(|p| Path::new(p).exists())
        .map(|p| media_path(&service, p))
        .collect::<Result<_, _>>()?;
    for path in paths {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    let mut inner = service.inner.lock().unwrap();
    inner.store.tasks.retain(|t| t.id != id);
    persist(&service, &inner)?;
    drop(inner);
    if let Some(window) = app.get_webview_window(&format!("download-{id}")) {
        let _ = window.close();
    }
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn toggle_pause(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let service = app.state::<Service>();
    let mut inner = service.inner.lock().unwrap();
    let task = inner
        .store
        .tasks
        .iter()
        .find(|t| t.id == id)
        .ok_or("Download not found.")?;
    if !matches!(task.status.as_str(), "Downloading" | "Paused") {
        return Err("Only running downloads can be paused.".into());
    }
    let resume = task.status == "Paused";
    let child = inner
        .children
        .get(&id)
        .ok_or("The downloader is still starting. Try again in a moment.")?
        .lock()
        .unwrap();
    #[cfg(unix)]
    {
        let signal = if resume { libc::SIGCONT } else { libc::SIGSTOP };
        if unsafe { libc::kill(-(child.id() as i32), signal) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(unix))]
    {
        return Err("Pause is not supported on this platform.".into());
    }
    drop(child);
    inner
        .store
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .unwrap()
        .status = if resume { "Downloading" } else { "Paused" }.into();
    persist(&service, &inner)?;
    drop(inner);
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn media_action(
    service: tauri::State<Service>,
    path: String,
    action: String,
) -> Result<(), String> {
    let path = media_path(&service, &path)?;
    match action.as_str() {
        "delete" => fs::remove_file(path).map_err(|e| e.to_string()),
        "play" => service_app_open(&path),
        "folder" => service_app_open(path.parent().unwrap()),
        _ => Err("Unknown file action.".into()),
    }
}
fn service_app_open(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let mut cmd = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("explorer");
        c
    };
    cmd.arg(path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn open_folder(service: tauri::State<Service>) -> Result<(), String> {
    let folder = service
        .inner
        .lock()
        .unwrap()
        .store
        .settings
        .download_folder
        .clone();
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    service_app_open(Path::new(&folder))
}
#[tauri::command]
pub async fn tool_status(app: tauri::AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<Service>();
        let version = command(&service)
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
        let ffmpeg = Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success());
        json!({"version": version, "ffmpeg": ffmpeg, "pauseSupported": cfg!(unix)})
    })
    .await
    .map_err(|e| e.to_string())
}
struct UpdateGuard(tauri::AppHandle);
impl Drop for UpdateGuard {
    fn drop(&mut self) {
        self.0.state::<Service>().inner.lock().unwrap().updating = false;
        schedule(self.0.clone());
    }
}
#[tauri::command]
pub async fn update_downloader(app: tauri::AppHandle) -> Result<String, String> {
    let service = app.state::<Service>();
    {
        let mut inner = service.inner.lock().unwrap();
        if inner.updating
            || inner
                .store
                .tasks
                .iter()
                .any(|t| matches!(t.status.as_str(), "Downloading" | "Paused"))
        {
            return Err("Wait for active downloads or the current update to finish.".into());
        }
        inner.updating = true;
    }
    let _guard = UpdateGuard(app.clone());
    let asset = if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else if cfg!(target_arch = "aarch64") {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    // Pin the release before fetching both the executable and its digest.
    let release: Value = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .header("User-Agent", "YT_DOWN/0.1.0")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or("Release version missing.")?;
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err("Invalid release version.".into());
    }
    let base = format!("https://github.com/yt-dlp/yt-dlp/releases/download/{tag}");
    let mut response = client
        .get(format!("{base}/{asset}"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let total = response.content_length();
    let mut bytes = Vec::new();
    let mut last_emit = std::time::Instant::now();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        if last_emit.elapsed() >= Duration::from_millis(200) {
            let _ = app.emit(
                "tool-update-progress",
                json!({"received":bytes.len(), "total":total}),
            );
            last_emit = std::time::Instant::now();
        }
    }
    let _ = app.emit(
        "tool-update-progress",
        json!({"received":bytes.len(), "total":total}),
    );
    let checksums = client
        .get(format!("{base}/SHA2-256SUMS"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let expected = checksums
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            let file = fields.next()?.trim_start_matches('*');
            (file == asset).then_some(digest)
        })
        .ok_or("Download checksum missing.")?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err("Download checksum did not match. Try again.".into());
    }
    let temp = service.data.join(if cfg!(windows) {
        "yt-dlp.download.exe"
    } else {
        "yt-dlp.download"
    });
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    let executable = temp.clone();
    let version = tauri::async_runtime::spawn_blocking(move || {
        Command::new(executable).arg("--version").output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("Downloaded yt-dlp could not run: {e}"))?;
    if !version.status.success() {
        return Err(format!(
            "Downloaded yt-dlp could not run: {}",
            String::from_utf8_lossy(&version.stderr)
        ));
    }
    let dest = service.data.join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });
    fs::rename(temp, dest).map_err(|e| e.to_string())?;
    Ok("yt-dlp installed and ready.".into())
}
fn newer_version(current: &str, latest: &str) -> bool {
    let parse = |version: &str| {
        version
            .trim_start_matches('v')
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
    };
    let (Ok(current), Ok(latest)) = (parse(current), parse(latest)) else {
        return false;
    };
    (0..current.len().max(latest.len()))
        .map(|i| *latest.get(i).unwrap_or(&0))
        .cmp((0..current.len().max(latest.len())).map(|i| *current.get(i).unwrap_or(&0)))
        .is_gt()
}
#[tauri::command]
pub async fn check_updates() -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .user_agent("YT_DOWN/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let tool: Value = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let app_result = async {
        let response: Value = client.post("https://ytdown.michotech.me/api/v1/version/check").json(&json!({"platform": std::env::consts::OS, "version": env!("CARGO_PKG_VERSION"), "runtime": "tauri"})).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
        if response["success"] != true { return Err(response["message"].as_str().unwrap_or("App update service unavailable.").to_string()); }
        Ok(response)
    }.await;
    let mut result = json!({"latestTool": tool["tag_name"], "appVersion": env!("CARGO_PKG_VERSION"), "appUrl": "https://ytdown.michotech.me"});
    match app_result {
        Ok(remote) => {
            // The reference server also distributes the Python desktop app. Only
            // offer an installer when the server explicitly identifies a Tauri build.
            if remote["runtime"] == "tauri" || remote["framework"] == "tauri" {
                if remote["latest_version"]
                    .as_str()
                    .is_some_and(|version| newer_version(env!("CARGO_PKG_VERSION"), version))
                {
                    result["latestApp"] = remote["latest_version"].clone();
                }
                if let Some(url) = remote["download_url"]
                    .as_str()
                    .filter(|url| url.starts_with("https://"))
                {
                    result["appUrl"] = json!(url);
                }
            } else {
                result["appError"] =
                    json!("No compatible Tauri release is published by the update service yet.");
            }
        }
        Err(error) => result["appError"] = json!(format!("App update check failed: {error}")),
    }
    Ok(result)
}
pub fn stop(app: &tauri::AppHandle) {
    let service = app.state::<Service>();
    let mut inner = service.inner.lock().unwrap();
    inner.stopping = true;
    for task in &mut inner.store.tasks {
        if matches!(task.status.as_str(), "Downloading" | "Paused") {
            task.status = "Failed".into();
            task.error = "Download interrupted when the app closed. Retry to resume.".into();
        }
    }
    let _ = persist(&service, &inner);
    for child in inner.children.values() {
        let _ = kill_process(&mut child.lock().unwrap());
    }
}
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    stop(&app);
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, status: &str) -> Task {
        Task {
            id: id.into(),
            url: "https://youtu.be/example".into(),
            title: id.into(),
            thumbnail: None,
            quality: "720p (HD)".into(),
            folder: String::new(),
            playlist_start: 1,
            status: status.into(),
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: None,
            dismissed: false,
            phase: String::new(),
            speed: String::new(),
            eta: String::new(),
            error: String::new(),
            created: 0,
            completed_at: None,
            files: vec![],
            log: vec![],
        }
    }
    #[test]
    fn queue_obeys_concurrency_and_fifo() {
        let mut store = Store {
            settings: Preferences {
                max_concurrent: 2,
                ..Default::default()
            },
            tasks: vec![
                task("newest", "Queued"),
                task("oldest", "Queued"),
                task("running", "Downloading"),
            ],
        };
        let selected = take_queued(&mut store);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "oldest");
        assert!(take_queued(&mut store).is_empty());
        store.settings.max_concurrent = 1;
        assert!(take_queued(&mut store).is_empty());
    }
    #[test]
    fn queue_ignores_terminal_tasks() {
        let mut store = Store {
            settings: Preferences {
                max_concurrent: 1,
                ..Default::default()
            },
            tasks: vec![
                task("cancelled", "Cancelled"),
                task("completed", "Completed"),
                task("failed", "Failed"),
                task("queued", "Queued"),
            ],
        };
        let selected = take_queued(&mut store);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "queued");
    }
    #[test]
    fn rejects_non_youtube_and_confusable_urls() {
        for url in [
            "file:///etc/passwd",
            "https://youtube.com.evil.test/watch?v=x",
            "https://youtube.com@evil.test/",
            "--exec=bad",
            "javascript:alert(1)",
        ] {
            assert!(valid_url(url).is_err(), "{url}");
        }
        for url in [
            "https://youtu.be/abc",
            "https://www.youtube.com/watch?v=abc",
            "https://music.youtube.com/playlist?list=abc",
        ] {
            assert!(valid_url(url).is_ok());
        }
    }
    #[test]
    fn updates_only_offer_newer_versions() {
        assert!(newer_version("0.1.0", "0.2.0"));
        assert!(newer_version("0.1.0", "v0.1.1"));
        assert!(!newer_version("0.1.0", "0.1"));
        assert!(!newer_version("1.10.0", "1.9.0"));
        assert!(!newer_version("0.1.0", "invalid"));
    }
    #[test]
    fn mp4_arguments_use_clean_names_without_sidecars() {
        let task = task("sample", "Queued");
        let args = download_arguments(&task);
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-o", "%(playlist_title|.)s/%(title)s.%(ext)s"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--recode-video", "mp4"])
        );
        assert!(args.contains(&"--no-write-info-json".into()));
        assert!(args.contains(&"--no-write-thumbnail".into()));
        assert!(!args.iter().any(|arg| arg.contains("%(id)")));
        let audio = Task {
            quality: "Audio only (MP3)".into(),
            ..task
        };
        let args = download_arguments(&audio);
        assert!(args.windows(2).any(|pair| pair == ["-f", "ba/b"]));
        assert!(args.windows(2).any(|pair| pair == ["--audio-format", "mp3"]));
        assert!(args.windows(2).any(|pair| pair == ["--audio-quality", "192K"]));
        assert!(!args.contains(&"--recode-video".into()));
        assert_eq!(normalize_quality("Audio only (MP4)"), "Audio only (MP3)");
    }
    #[test]
    fn progress_records_transfer_sizes_and_unknown_totals() {
        let mut task = task("sample", "Downloading");
        apply_progress(&mut task, " 14.0%|2 MiB/s|00:30|234420000|1530000000");
        assert_eq!(task.progress, 14.0);
        assert_eq!(task.downloaded_bytes, 234420000);
        assert_eq!(task.total_bytes, Some(1530000000));
        apply_progress(&mut task, " NA%|Unknown|Unknown|NA|NA");
        assert_eq!(task.progress, 0.0);
        assert_eq!(task.total_bytes, None);
    }
    #[test]
    fn paused_tasks_keep_their_concurrency_slot() {
        let mut store = Store {
            settings: Preferences {
                max_concurrent: 1,
                ..Default::default()
            },
            tasks: vec![task("queued", "Queued"), task("paused", "Paused")],
        };
        assert!(take_queued(&mut store).is_empty());
    }
    #[test]
    fn renaming_preserves_the_extension_and_never_overwrites() {
        let root =
            std::env::temp_dir().join(format!("ytdown-rename-{}-{}", std::process::id(), now()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("before.mp4");
        fs::write(&source, b"video").unwrap();
        fs::write(root.join("existing.mp4"), b"keep").unwrap();
        assert!(rename_file(&source, "existing").is_err());
        assert_eq!(fs::read(root.join("existing.mp4")).unwrap(), b"keep");
        for name in ["", "..", "../outside", "bad/name", "bad\\name"] {
            assert!(rename_file(&source, name).is_err());
        }
        let destination = rename_file(&source, "after").unwrap();
        assert_eq!(destination, root.join("after.mp4"));
        assert!(!source.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"video");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn old_settings_receive_new_defaults() {
        let settings: Preferences =
            serde_json::from_str(r#"{"maxConcurrent":4,"theme":"Dark"}"#).unwrap();
        assert_eq!(settings.max_concurrent, 4);
        assert_eq!(settings.auto_close_delay, 5);
        assert!(settings.notifications);
    }
    #[test]
    fn library_groups_media_and_rejects_outside_paths() {
        let root =
            std::env::temp_dir().join(format!("ytdown-test-{}-{}", std::process::id(), now()));
        let folder = root.join("downloads");
        fs::create_dir_all(folder.join("Playlist")).unwrap();
        fs::write(folder.join("Playlist/song.mp3"), b"audio").unwrap();
        fs::write(folder.join("metadata.info.json"), b"{}").unwrap();
        fs::write(root.join("outside.mp4"), b"outside").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("outside.mp4"), folder.join("link.mp4")).unwrap();
        let mut media = vec![];
        scan(&folder, &folder, &mut media).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].playlist, "Playlist");
        assert_eq!(media[0].size, 5);
        let service = Service {
            data: root.clone(),
            inner: Arc::new(Mutex::new(Inner {
                updating: false,
                stopping: false,
                children: HashMap::new(),
                store: Store {
                    settings: Preferences {
                        download_folder: folder.to_string_lossy().into_owned(),
                        ..Default::default()
                    },
                    tasks: vec![task("persisted", "Completed")],
                },
            })),
        };
        assert!(media_path(&service, root.join("outside.mp4").to_str().unwrap()).is_err());
        assert!(media_path(&service, folder.join("Playlist/song.mp3").to_str().unwrap()).is_ok());
        #[cfg(unix)]
        assert!(media_path(&service, folder.join("link.mp4").to_str().unwrap()).is_err());
        {
            let inner = service.inner.lock().unwrap();
            persist(&service, &inner).unwrap();
        }
        let saved: Store =
            serde_json::from_slice(&fs::read(root.join("state.json")).unwrap()).unwrap();
        assert_eq!(saved.tasks[0].id, "persisted");
        fs::remove_dir_all(root).unwrap();
    }
}
