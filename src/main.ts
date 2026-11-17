import "./styles.css";
import { fitSidebars, unifiedQueue } from "./ui-model";
import { open, confirm } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { createIcons, House, FolderOpen, Activity, History, Settings, Info, Download, Search, Clipboard, RefreshCw, Trash2, Copy, ExternalLink, Check, Clock3, ArrowDownToLine, Bell, Palette, Keyboard, UserRound, Globe, HardDrive, FileVideo, Music2, ListVideo, ChevronRight, ChevronLeft, Ellipsis, Pause, Play, Flag, ChevronDown, Pencil, X, LayoutGrid, List } from "lucide";

type Page = "Home" | "History" | "Settings" | "About";
type YouTubeVideo = { id: string; title: string; channel: string; duration: string; thumbnail: string };
type Preferences = { maxConcurrent: number; defaultQuality: string; downloadFolder: string; theme: string; notifications: boolean; autoClose: boolean; autoCloseDelay: number };
type Task = { dismissed?: boolean; downloadedBytes?: number; totalBytes?: number | null; phase?: string; thumbnail?: string | null; completedAt?: number | null; id: string; url: string; title: string; quality: string; folder: string; playlistStart: number; status: string; progress: number; speed: string; eta: string; error: string; created: number; files: string[]; log: string[] };
type Media = { path: string; name: string; size: number; modified: number; kind: string; playlist: string };
const pages: Page[] = ["Home", "History", "About"];
const glyph: Record<Page, string> = { Home: "house", History: "history", Settings: "settings", About: "info" };
const qualities = [["480p (SD)", "480p"], ["720p (HD)", "720p"], ["1080p (Full HD)", "1080p"], ["Best available", "Best"], ["Audio only (MP3)", "Audio"]];
let settings: Preferences = { maxConcurrent: 3, defaultQuality: "720p (HD)", downloadFolder: "", theme: "System", notifications: true, autoClose: false, autoCloseDelay: 5 };
let tasks: Task[] = [], media: Media[] = [];
let page: Page = "Home", pageStack: Page[] = ["Home"], pageIndex = 0;
let mediaLoading = false, mediaError = "", mediaFilter = "", sortKey: keyof Media = "modified", sortAsc = false;
let currentFolder = "";
let mediaView: "grid" | "list" = localStorage.getItem("ytdown-media-view") === "list" ? "list" : "grid";
let panelWidths = { left: 208, right: 280 };
try { const saved = JSON.parse(localStorage.getItem("ytdown-panels") || "null"); if (saved) panelWidths = fitSidebars(innerWidth - 24, Number(saved.left), Number(saved.right)); } catch { /* Use the standard desktop widths. */ }
let tools: { version: string | null; ffmpeg: boolean; pauseSupported?: boolean } | null = null;
let updates: { latestTool: string; appVersion: string; appUrl: string; latestApp?: string; appError?: string } | null = null, updateBusy = false;
let updateMessage = "";
let youtubeQuery = "", youtubeResults: YouTubeVideo[] = [], youtubeSearchLoading = false, youtubeSearchError = "", youtubeSearchTimer = 0, youtubeSearchRequest = 0, alertTimer = 0;
let playlistInspecting = false;
let selectedMedia = "";
let preparingDownload = false;
let progressTab = "Info";
let progressLogOpen = false;
let progressTitle = "";
const detailId = new URLSearchParams(location.search).get("download");
const updateWindow = new URLSearchParams(location.search).has("updates");
const app = document.querySelector<HTMLDivElement>("#app")!;
const iconSet = { House, FolderOpen, Activity, History, Settings, Info, Download, Search, Clipboard, RefreshCw, Trash2, Copy, ExternalLink, Check, Clock3, ArrowDownToLine, Bell, Palette, Keyboard, UserRound, Globe, HardDrive, FileVideo, Music2, ListVideo, ChevronRight, ChevronLeft, Ellipsis, Pause, Play, Flag, ChevronDown, Pencil, X, LayoutGrid, List };
const icon = (name: string, cls = "") => `<i class="icon ${cls}" data-lucide="${name}" aria-hidden="true"></i>`;
const e = (value: unknown) => escapeHtml(String(value ?? "")).replace(/"/g, "&quot;").replace(/'/g, "&#39;");
const active = (t: Task) => ["Queued", "Downloading", "Paused"].includes(t.status);
const icons = () => createIcons({ icons: iconSet, attrs: { "stroke-width": 1.8 } });
const empty = (name: string, title: string, description: string) => `<div class="empty-state"><span>${icon(name)}</span><b>${e(title)}</b><small>${e(description)}</small></div>`;
function render() {
  if (detailId) { renderDetail(); return; }
  if (updateWindow) { renderUpdateManager(); return; }
  app.innerHTML = `<div class="window"><aside class="sidebar"><div class="brand"><img src="/icons/ytdown.ico" alt=""/><b>YT_DOWN</b></div><nav>${pages.map(item => `<button class="nav-item ${page === item ? "selected" : ""}" data-page="${item}">${icon(glyph[item])}<span>${item}</span></button>`).join("")}</nav><div class="sidebar-footer"><button class="update-button" data-action="manage-updates">${icon("refresh-cw")} Updates</button><small>YT_DOWN 0.1.0</small></div></aside>
  <header class="toolbar"><button class="history-button" data-nav="back" aria-label="Back" ${!pageIndex ? "disabled" : ""}>${icon("chevron-left")}</button><button class="history-button" data-nav="forward" aria-label="Forward" ${pageIndex === pageStack.length - 1 ? "disabled" : ""}>${icon("chevron-right")}</button><div class="search-control-wrap"><div class="global-search">${icon("search")}<input id="youtube-search-input" class="youtube-search-input" type="search" placeholder="Search videos or paste a YouTube link" value="${e(youtubeQuery)}" aria-label="Search YouTube videos or paste a video link" autocomplete="off"><button class="search-clear" data-youtube-clear aria-label="Clear search" ${!youtubeQuery ? "hidden" : ""}>${icon("x")}</button></div><section class="search-popover" id="youtube-search-popover" aria-label="YouTube search results" hidden><h2>YouTube</h2><div class="youtube-results" id="youtube-results"></div></section></div><button class="toolbar-button" data-action="folder" aria-label="Open download folder" title="Open download folder">${icon("folder-open")}</button><button class="toolbar-button" data-page="Settings" aria-label="Settings" title="Settings">${icon("settings")}</button></header><main class="main"><div class="content" id="content"></div></main><aside class="sidepanel" id="sidepanel"></aside><div class="panel-resizer left-resizer" role="separator" aria-label="Resize navigation sidebar" aria-orientation="vertical" tabindex="0" data-resize="left"></div><div class="panel-resizer right-resizer" role="separator" aria-label="Resize download manager" aria-orientation="vertical" tabindex="0" data-resize="right"></div></div><div class="alert-pill" id="alert-pill" role="status" aria-live="polite" hidden></div>`;
  renderPage(); renderToolNotice(); renderSidePanel(); applyPanelWidths(); bindPanelResizing(); icons(); renderYouTubeResults();
  app.querySelectorAll<HTMLInputElement>(".youtube-search-input").forEach(input => {
    input.addEventListener("input", () => handleYouTubeSearchInput(input));
    input.addEventListener("focus", () => {
      const panel = app.querySelector<HTMLElement>("#youtube-search-popover");
      if (panel) panel.hidden = !youtubeQuery;
      renderYouTubeResults();
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-youtube-clear]").forEach(button => button.addEventListener("click", () => clearYouTubeSearch(button.closest(".global-search")?.querySelector<HTMLInputElement>(".youtube-search-input") ?? undefined)));
  app.querySelector<HTMLDivElement>("#youtube-results")?.addEventListener("click", handleYouTubeResultClick);
}
function heading(title: string, actions = "") { return `<div class="page-heading"><h1>${title}</h1><div class="heading-actions">${actions}</div></div>`; }
function taskRow(task: Task, history = false) {
  const thumbnail = taskThumbnail(task);
  return `<article class="download-row" data-task="${e(task.id)}"><div class="file-icon">${thumbnail ? `<img src="${e(thumbnail)}" alt="" loading="lazy">` : icon(task.quality.startsWith("Audio") ? "music-2" : "download")}</div><div class="file-info"><b title="${e(task.title)}">${e(task.title)}</b><small>${e(task.quality)} · ${e(new Date(task.completedAt ?? task.created).toLocaleString())}</small></div><span class="task-status">${e(task.status)}</span>${task.status === "Completed" ? `<button class="quiet-button" data-task-menu aria-label="Download actions" title="Download actions">${icon("ellipsis")}</button>` : history ? `<div class="row-actions"><button class="secondary" data-task-action="retry" title="Try again" aria-label="Try again">${icon("refresh-cw")}</button><button class="secondary" data-task-action="copy" title="Copy URL" aria-label="Copy URL">${icon("copy")}</button></div>` : ""}</article>`;
}
function renderPage() {
  const content = app.querySelector<HTMLElement>("#content")!;
  if (page === "Home") {
    content.innerHTML = `<div id="tool-notice"></div><section class="browser-card"><div class="browser-tools"><label class="filter">${icon("search")}<input id="media-filter" value="${e(mediaFilter)}" placeholder="Filter files…" aria-label="Filter files"></label><div class="view-switch" role="group" aria-label="Media view"><button class="${mediaView === "grid" ? "selected" : ""}" data-view="grid" aria-label="Grid view" title="Grid view">${icon("layout-grid")}</button><button class="${mediaView === "list" ? "selected" : ""}" data-view="list" aria-label="List view" title="List view">${icon("list")}</button></div><button class="secondary" data-action="refresh" title="Refresh downloads" aria-label="Refresh downloads">${icon("refresh-cw")}</button><button class="secondary" data-action="folder">${icon("folder-open")} Open Folder</button></div><div id="media-results"></div></section>`;
    renderMedia();
  } else if (page === "History") {
    const history = tasks.filter(t => !active(t));
    content.innerHTML = `${heading("Download History", `<button class="danger" data-action="clear-history" ${!history.length ? "disabled" : ""}>${icon("trash-2")} Clear All</button>`)}<section>${history.length ? history.map(t => taskRow(t,true)).join("") : empty("history", "No download history", "Completed and stopped downloads appear here.")}</section>`;
  } else if (page === "Settings") { content.innerHTML = settingsMarkup(); }
  else {
    content.innerHTML = `${heading("About")}<section class="about-hero"><img src="/icons/ytdown.ico" alt=""><h2>YT_DOWN</h2><p>Version 0.1.0</p></section><section class="about-card"><h2>Keyboard Shortcuts</h2>${[["Ctrl + D", "Search YouTube"],["Ctrl + B", "Downloads"],["Ctrl + L", "Clear finished downloads"],["Ctrl + ,", "Settings"],["Ctrl + Q", "Quit"],["Enter", "Download a pasted link"]].map(([key, label]) => `<div class="shortcut-row"><kbd>${key}</kbd><span>${label}</span></div>`).join("")}</section>`;
  }
}

function renderMedia() {
  const result = app.querySelector<HTMLElement>("#media-results"); if (!result) return;
  if (mediaLoading) { result.innerHTML = `<div class="loading-state"><span class="spinner"></span><b>Scanning download folder…</b></div>`; return; }
  if (mediaError) { result.innerHTML = empty("info", "Could not scan folder", mediaError); icons(); return; }
  const query = mediaFilter.trim().toLowerCase();
  const normalizePath = (path: string) => path.replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
  const current = normalizePath(currentFolder);
  const descendants = media.filter(item => {
    const folder = normalizePath(item.playlist);
    return current ? folder === current || folder.startsWith(`${current}/`) : true;
  });
  const directFiles = descendants.filter(item => normalizePath(item.playlist) === current && `${item.name} ${item.playlist} ${item.kind}`.toLowerCase().includes(query));
  const folderPaths = [...new Set(descendants
    .map(item => normalizePath(item.playlist))
    .filter(path => !!path && path !== current && (!current || path.startsWith(`${current}/`)))
    .map(path => path.slice(current ? current.length + 1 : 0).split("/")[0])
    .filter(Boolean))];
  const folders = folderPaths.filter(name => !query || name.toLowerCase().includes(query)).map(name => {
    const path = current ? `${current}/${name}` : name;
    const files = descendants.filter(item => normalizePath(item.playlist) === path);
    const nestedCount = descendants.filter(item => normalizePath(item.playlist).startsWith(`${path}/`)).length;
    const coverFiles = descendants.filter(item => normalizePath(item.playlist) === path || normalizePath(item.playlist).startsWith(`${path}/`));
    return { name, path, count: files.length + nestedCount, thumbnail: coverFiles.map(mediaThumbnail).find(Boolean) || null };
  }).sort((a,b) => a.name.localeCompare(b.name));
  const items = directFiles.sort((a,b) => {
    const av = a[sortKey], bv = b[sortKey];
    return (typeof av === "number" && typeof bv === "number" ? av-bv : String(av).localeCompare(String(bv))) * (sortAsc ? 1 : -1);
  });
  const crumbs = current ? `<nav class="media-breadcrumb" aria-label="Folder path"><button data-folder-path="">Downloads</button>${current.split("/").map((part,index,parts) => { const path = parts.slice(0,index+1).join("/"); return `${icon("chevron-right")}<button data-folder-path="${e(path)}" ${index === parts.length-1 ? 'aria-current="page"' : ""}>${e(part)}</button>`; }).join("")}</nav>` : "";
  const folderMarkup = folders.map(folder => `<button class="media-tile folder-tile" data-open-folder="${e(folder.path)}" title="Open ${e(folder.name)}"><span class="media-art">${folder.thumbnail ? `<img src="${e(folder.thumbnail)}" alt="" loading="lazy">` : icon("folder-open")}<span class="folder-open-hint">Open ${icon("chevron-right")}</span></span><small>Folder · ${folder.count} ${folder.count === 1 ? "video" : "videos"}</small><b>${e(folder.name)}</b></button>`).join("");
  const fileMarkup = items.map(item => {
    const thumbnail = mediaThumbnail(item);
    const title = item.name.replace(/\.[^.]+$/, "");
    return `<article class="media-tile file-tile ${selectedMedia === item.path ? "is-selected" : ""}" data-media="${e(item.path)}" tabindex="0"><span class="media-art">${thumbnail ? `<img src="${e(thumbnail)}" alt="" loading="lazy">` : icon(item.kind === "mp3" ? "music-2" : "file-video")}<button class="media-file-menu" data-media-menu title="File actions" aria-label="File actions">${icon("ellipsis")}</button></span><small>${item.kind.toUpperCase()} · ${formatBytes(item.size)}</small><b title="${e(title)}">${e(title)}</b></article>`;
  }).join("");
  const grid = `<div class="media-grid">${folderMarkup}${fileMarkup}</div>`;
  const list = `<div class="media-list"><div class="media-list-head"><span>Name</span><span>Size</span><span>Date modified</span><span></span></div>${folders.map(folder => `<button class="media-list-row folder-list-row" data-open-folder="${e(folder.path)}"><span class="media-list-name"><span class="list-art">${folder.thumbnail ? `<img src="${e(folder.thumbnail)}" alt="">` : icon("folder-open")}</span><b>${e(folder.name)}</b><small>Folder · ${folder.count} ${folder.count === 1 ? "video" : "videos"}</small></span><small>-</small><small>-</small><span class="open-folder-action">Open ${icon("chevron-right")}</span></button>`).join("")}${items.map(item => `<div class="media-list-row ${selectedMedia === item.path ? "is-selected" : ""}" data-media="${e(item.path)}" tabindex="0"><span class="media-list-name"><span class="list-art">${mediaThumbnail(item) ? `<img src="${e(mediaThumbnail(item)!)}" alt="">` : icon(item.kind === "mp3" ? "music-2" : "file-video")}</span><b title="${e(item.name)}">${e(item.name)}</b></span><small>${formatBytes(item.size)}</small><small>${e(new Date(item.modified).toLocaleDateString())}</small><button class="quiet-button" data-media-menu aria-label="File actions">${icon("ellipsis")}</button></div>`).join("")}</div>`;
  result.innerHTML = `${crumbs}${folders.length || items.length ? `${mediaView === "grid" ? grid : list}` : empty("folder-open", media.length ? "No matching files" : "No downloaded media", media.length ? "This folder has no matching files." : "Completed downloads will appear here.")}`;
  icons();
}
function mediaThumbnail(item: Media) {
  const task = tasks.find(task => task.files.some(path => path === item.path));
  return task ? taskThumbnail(task) : null;
}
function formatBytes(value: number) { return value >= 1073741824 ? `${(value/1073741824).toFixed(2)} GiB` : value >= 1048576 ? `${(value/1048576).toFixed(2)} MiB` : value >= 1024 ? `${(value/1024).toFixed(1)} KiB` : `${value} B`; }
function taskThumbnail(task: Task) {
  if (task.thumbnail?.startsWith("https://")) return task.thumbnail;
  const id = youtubeVideoId(task.url);
  return id ? `https://i.ytimg.com/vi/${id}/hqdefault.jpg` : null;
}
function youtubeVideoId(value: string): string | null {
  try {
    const url = new URL(normalizeYoutubeUrl(value));
    if (!isYoutubeHost(url.hostname)) return null;
    const id = url.hostname === "youtu.be" ? url.pathname.slice(1).split("/")[0] : url.searchParams.get("v") || url.pathname.match(/^\/(?:shorts|live|embed|v)\/([^/]+)/)?.[1];
    return id && /^[\w-]{11}$/.test(id) ? id : null;
  } catch { return null; }
}
function isYoutubeHost(host: string) {
  return /(^|\.)(youtube\.com|youtube-nocookie\.com|youtu\.be)$/i.test(host);
}
function normalizeYoutubeUrl(value: string) {
  const trimmed = value.trim().replace(/^<|>$/g, "").replace(/[),.;!?]+$/, "");
  return /^[a-z][a-z\d+.-]*:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
}
function isYoutubeLink(value: string) {
  try { return isYoutubeHost(new URL(normalizeYoutubeUrl(value)).hostname); }
  catch { return false; }
}
function managerTask(task: Task) {
  const running = task.status === "Downloading" || task.status === "Paused";
  return `<article class="manager-task" data-task="${e(task.id)}"><div class="manager-title"><b title="${e(task.title)}">${e(task.title)}</b><span>${running ? `${task.progress.toFixed(0)}%` : e(task.status)}</span></div>${active(task) || task.status === "Completed" ? `<progress max="100" value="${task.progress}" aria-label="${e(task.title)} progress"></progress>` : ""}<div class="manager-meta"><small>${running ? `${task.status === "Paused" ? "Paused" : e(task.phase || task.speed || "Starting…")}${task.eta && task.status !== "Paused" ? ` · ${e(task.eta)} left` : ""}` : e(task.quality)}</small><div class="manager-controls">${active(task) ? `<button class="quiet-button" data-task-action="show" title="Open progress window" aria-label="Open progress window">${icon("external-link")}</button><button class="quiet-button cancel-task" data-task-action="cancel" title="Cancel download" aria-label="Cancel download">${icon("x")}</button>` : task.status !== "Completed" ? `<button class="quiet-button" data-task-action="retry" title="Try again" aria-label="Try again">${icon("refresh-cw")}</button>` : ""}<button class="quiet-button" data-task-menu title="Download actions" aria-label="Download actions">${icon("ellipsis")}</button></div></div></article>`;
}
function renderSidePanel() {
  const panel = app.querySelector<HTMLElement>("#sidepanel"); if (!panel) return;
  const list = unifiedQueue(tasks);
  const scrollTop = panel.querySelector<HTMLElement>(".side-content")?.scrollTop || 0;
  const focused = document.activeElement as HTMLElement | null;
  const focusedTask = focused && panel.contains(focused) ? focused.closest<HTMLElement>("[data-task]")?.dataset.task : undefined;
  const focusedAction = focused?.dataset.taskAction;
  const focusedMenu = focused?.hasAttribute("data-task-menu");
  panel.innerHTML = `<div class="side-tabs"><h2>Downloads</h2><button class="text-button queue-clear" data-action="clear-queue" title="Clear finished entries; keep History" ${list.some(task => !active(task)) ? "" : "disabled"}>Clear</button></div><div class="side-content">${list.length ? `<p class="manager-counts">${list.filter(t => t.status === "Downloading").length} downloading · ${list.filter(t => t.status === "Queued").length} queued</p><div class="manager-queue">${list.map(managerTask).join("")}</div>` : `<div class="side-empty"><b>Queue is empty</b></div>`}</div>`;
  const content = panel.querySelector<HTMLElement>(".side-content"); if (content) content.scrollTop = scrollTop;
  if (focusedTask && (focusedAction || focusedMenu)) panel.querySelector<HTMLElement>(`[data-task="${CSS.escape(focusedTask)}"] ${focusedMenu ? "[data-task-menu]" : `[data-task-action="${CSS.escape(focusedAction!)}"]`}`)?.focus({preventScroll: true});
  icons();
}
function settingsMarkup() {
  const row = (label: string, control: string) => `<label class="setting-row"><b>${label}</b>${control}</label>`;
  const toggle = (id: string, checked: boolean) => `<span class="switch"><input id="${id}" type="checkbox" ${checked ? "checked" : ""}><span class="switch-track"><span></span></span></span>`;
  return `${heading("Settings")}<div id="settings-form"><section class="settings-section"><div class="settings-title"><h2>Downloads</h2></div>${row("Concurrent downloads", `<input id="concurrent" type="number" min="1" max="10" value="${settings.maxConcurrent}">`)}${row("Default quality", `<select id="default-quality">${qualities.map(([q]) => `<option ${q === settings.defaultQuality ? "selected" : ""}>${q}</option>`).join("")}</select>`)}<div class="setting-row"><b>Download folder</b><div class="setting-folder"><input id="settings-folder" value="${e(settings.downloadFolder)}" readonly><button class="secondary" type="button" data-action="browse">${icon("folder-open")} Choose</button></div></div></section><section class="settings-section"><div class="settings-title"><h2>Appearance & Notifications</h2></div>${row("Theme", `<select id="theme">${["System","Dark","Light"].map(t => `<option ${settings.theme === t ? "selected" : ""}>${t}</option>`).join("")}</select>`)}${row("Desktop notifications", toggle("notifications", settings.notifications))}${row("Close completed download windows", toggle("auto-close", settings.autoClose))}${row("Close after (seconds)", `<input id="auto-close-delay" type="number" min="1" max="120" value="${settings.autoCloseDelay}" ${settings.autoClose ? "" : "disabled"}>`)}</section></div>`;
}
async function saveSettingsFromUI() {
  const form = app.querySelector<HTMLElement>("#settings-form");
  if (!form) return;
  const control = (id: string) => form.querySelector<HTMLInputElement | HTMLSelectElement>(`#${id}`)!;
  const maxConcurrent = Math.min(10, Math.max(1, Number(control("concurrent").value) || 1));
  const autoCloseDelay = Math.min(120, Math.max(1, Number(control("auto-close-delay").value) || 1));
  control("concurrent").value = String(maxConcurrent);
  control("auto-close-delay").value = String(autoCloseDelay);
  const saved: Preferences = {
    maxConcurrent,
    defaultQuality: control("default-quality").value,
    downloadFolder: control("settings-folder").value,
    theme: control("theme").value,
    notifications: (control("notifications") as HTMLInputElement).checked,
    autoClose: (control("auto-close") as HTMLInputElement).checked,
    autoCloseDelay,
  };
  if (saved.downloadFolder !== settings.downloadFolder) currentFolder = "";
  await invoke("save_settings", {settings: saved});
  settings = saved;
  localStorage.setItem("ytdown-settings", JSON.stringify(saved));
  applyTheme(saved.theme);
  renderSidePanel();
  setStatus("Settings saved.");
}
function updateMarkup() { return `<p>yt-dlp: ${e(tools?.version || "Not installed")} · FFmpeg: ${tools?.ffmpeg ? "Available" : "Not found - needed for MP4 merging and MP3 extraction"}</p>${updateBusy ? `<p><span class="spinner small"></span> Updating… <span id="update-progress"></span></p>` : ""}${updates ? `<p>Latest yt-dlp: ${e(updates.latestTool)} · App: ${e(updates.appVersion)}${updates.latestApp ? ` · Available: ${e(updates.latestApp)}` : ""}</p>${updates.appError ? `<p class="error-text">${e(updates.appError)}</p>` : ""}` : ""}`; }
function renderUpdateManager() {
  if (!updateWindow) return;
  app.innerHTML = `<main class="updates-window"><header><h1>Updates</h1><p>Manage the downloader and application updates.</p></header><section class="settings-section"><div class="settings-title"><h2>Installed tools</h2></div><div id="update-info">${updateMarkup()}</div><div class="row-actions"><button class="secondary" data-action="check-updates" ${updateBusy ? "disabled" : ""}>${icon("refresh-cw")} Check for Updates</button><button class="secondary" data-action="update-tool" ${updateBusy ? "disabled" : ""}>${icon("download")} ${tools?.version ? "Update yt-dlp" : "Install yt-dlp"}</button>${updates?.latestApp ? `<button class="primary" data-action="update-app">Get app update</button>` : ""}</div></section><p class="updates-result" role="status">${e(updateMessage)}</p></main>`;
  icons();
}
async function refreshState() {
  const state = await invoke<{settings: Preferences; tasks: Task[]}>("get_state");
  const previous = tasks; tasks = state.tasks; settings = state.settings;
  localStorage.setItem("ytdown-settings", JSON.stringify(settings));
  if (detailId) { renderDetail(); return; }
  renderSidePanel();
  if (page === "Home") { renderToolNotice(); renderMedia(); }
  else if (page === "History") { renderPage(); icons(); }
  for (const task of tasks) { if (!active(task) && previous.some(t => t.id === task.id && active(t))) { setStatus(`${task.status}: ${task.title}`); if (page === "Home") void loadMedia(); } }
}
let refreshTimer = 0;
function queueRefresh() { clearTimeout(refreshTimer); refreshTimer = window.setTimeout(() => { void refreshState().catch(err => setStatus(String(err))); }, 120); }
async function loadMedia() {
  mediaLoading = true; mediaError = ""; if (page === "Home") renderMedia();
  try { media = await invoke<Media[]>("list_media"); } catch (err) { mediaError = String(err); }
  mediaLoading = false; if (page === "Home") renderMedia();
}
function navigate(next: Page) { if (next === page) return; pageStack = pageStack.slice(0, pageIndex+1); pageStack.push(next); pageIndex++; page = next; render(); if (next === "Home") void loadMedia(); if (next === "Settings") void loadTools(); }
function renderToolNotice() {
  const notice = app.querySelector("#tool-notice");
  if (notice) notice.innerHTML = tools && !tools.version ? `<div class="dependency-notice">yt-dlp will be installed automatically with your first download.<button class="text-button" data-page="Settings">Open Settings ${icon("chevron-right")}</button></div>` : "";
  icons();
}
async function loadTools() { try { tools = await invoke("tool_status"); if (detailId) renderDetail(); if (updateWindow) renderUpdateManager(); else { const info = app.querySelector("#update-info"); if (info) info.innerHTML = updateMarkup(); renderToolNotice(); } } catch(err) { setStatus(String(err)); } }

async function startDownload(url: string, quality = settings.defaultQuality, start = 1, title?: string, thumbnail?: string) {
  if (preparingDownload) throw new Error("A download is being prepared. Please wait.");
  preparingDownload = true;
  renderYouTubeResults();
  if (!tools?.version) setStatus("Preparing yt-dlp for your download…");
  try {
    const task = await invoke<Task>("enqueue_download", {url, quality, playlistStart: start, title: title ?? null, thumbnail: thumbnail ?? null});
    await refreshState(); void loadTools(); setStatus("Download added to queue."); return task;
  } finally { preparingDownload = false; renderYouTubeResults(); }
}
function renderDetail() {
  document.body.classList.add("detail-window");
  const task = tasks.find(task => task.id === detailId);
  if (!task) { app.innerHTML = `<main class="download-detail">${empty("info", "Download unavailable", "This entry has been cleared.")}</main>`; icons(); return; }
  const focused = document.activeElement as HTMLElement | null;
  const focusedTab = focused?.dataset.progressTab;
  const focusedAction = focused?.dataset.taskAction;
  const focusedLog = focused?.hasAttribute("data-toggle-log");
  const title = `${task.progress.toFixed(0)}% · ${task.title}`;
  if (progressTitle !== title) { progressTitle = title; void getCurrentWindow().setTitle(title).catch(() => {}); }
  const row = (label: string, value: string, tooltip = "") => `<dt>${label}</dt><dd ${tooltip ? `title="${e(tooltip)}"` : ""}>${value}</dd>`;
  const info = `<dl class="transfer-info">${row("Size:", task.totalBytes ? formatBytes(task.totalBytes) : "Calculating…")}${row("Downloaded:", `${formatBytes(task.downloadedBytes || 0)} (${task.progress.toFixed(1)}%)`)}${row("Speed:", task.status === "Paused" ? "0 B/s" : e(task.speed || "-"))}${row("Time left:", e(task.status === "Queued" ? "Waiting" : task.eta || "-"))}${row("Quality:", e(task.quality))}</dl>`;
  const speed = `<dl class="transfer-info">${row("Transfer rate:", e(task.status === "Paused" ? "0 B/s" : task.speed || "-"))}${row("Downloaded:", formatBytes(task.downloadedBytes || 0))}${row("Total:", task.totalBytes ? formatBytes(task.totalBytes) : "-")}${row("Quality:", e(task.quality))}${row("Speed limit:", "None")}</dl>`;
  const completion = `<dl class="transfer-info">${row("On finish:", settings.autoClose ? `Close after ${settings.autoCloseDelay}s` : "Keep window open")}${row("Save to:", e(task.folder), task.folder)}</dl><button class="text-button" data-action="folder">${icon("folder-open")} Open Folder</button>`;
  const oldLog = app.querySelector<HTMLElement>(".transfer-log");
  const followLog = !oldLog || oldLog.scrollHeight - oldLog.scrollTop - oldLog.clientHeight < 30;
  const previousScroll = oldLog?.scrollTop || 0;
  const thumbnailUrl = taskThumbnail(task);
  const thumbnail = thumbnailUrl ? `<img class="transfer-thumbnail" src="${e(thumbnailUrl)}" alt="">` : `<span class="transfer-thumbnail transfer-thumbnail-fallback">${icon("file-video")}</span>`;
  app.innerHTML = `<main class="download-detail transfer-window"><nav class="transfer-tabs" role="tablist" aria-label="Download information">${[["Info", "info"], ["Speed", "activity"], ["On Completion", "flag"]].map(([tab,glyph]) => `<button role="tab" id="tab-${tab.replace(/ /g, "-")}" aria-selected="${progressTab === tab}" aria-controls="transfer-panel" data-progress-tab="${tab}" class="${progressTab === tab ? "selected" : ""}">${icon(glyph)} ${tab}</button>`).join("")}</nav><header class="transfer-preview">${thumbnail}<b title="${e(task.title)}">${e(task.title)}</b></header><section id="transfer-panel" role="tabpanel" aria-labelledby="tab-${progressTab.replace(/ /g, "-")}" class="transfer-panel">${progressTab === "Info" ? info : progressTab === "Speed" ? speed : completion}${task.error ? `<p class="error-text">${e(task.error)}</p>` : ""}${progressLogOpen ? `<pre class="transfer-log" aria-label="Download log">${e(task.log.join("\n") || "Waiting for the downloader…")}</pre>` : ""}</section><footer class="transfer-footer" data-task="${e(task.id)}"><div class="transfer-progress"><progress max="100" value="${task.progress}" aria-label="Download progress"></progress></div><div class="transfer-controls"><button class="secondary transfer-log-toggle" data-toggle-log title="${progressLogOpen ? "Hide" : "Show"} download details" aria-label="${progressLogOpen ? "Hide" : "Show"} download details" aria-expanded="${progressLogOpen}">${icon("chevron-down")}</button><div>${active(task) ? `<button class="secondary" data-task-action="pause" ${task.status === "Queued" || !tools?.pauseSupported ? "disabled" : ""}>${icon(task.status === "Paused" ? "play" : "pause")} ${task.status === "Paused" ? "Resume" : "Pause"}</button><button class="secondary" data-task-action="cancel">Cancel</button>` : `<button class="secondary" data-close-window>Close</button>`}</div></div></footer></main><div class="alert-pill" id="alert-pill" role="status" aria-live="polite" hidden></div>`;
  const log = app.querySelector<HTMLElement>(".transfer-log"); if (log) log.scrollTop = followLog ? log.scrollHeight : previousScroll;
  const selector = focusedTab ? `[data-progress-tab="${CSS.escape(focusedTab)}"]` : focusedAction ? `[data-task-action="${CSS.escape(focusedAction)}"]` : focusedLog ? "[data-toggle-log]" : null;
  if (selector) app.querySelector<HTMLElement>(selector)?.focus({preventScroll: true});
  icons();
}
async function handleAction(name: string) {
  if (name === "folder") await invoke("open_folder");
  if (name === "refresh") await loadMedia();
  if (name === "clear-queue") { await invoke("clear_queue"); await refreshState(); setStatus("Finished entries cleared. History is available."); }
  if (name === "manage-updates") await invoke("show_updates");
  if (name === "browse") { const folder = await open({directory: true, multiple: false, title:"Choose download folder"}); if (folder) { app.querySelector<HTMLInputElement>("#settings-folder")!.value = folder; await saveSettingsFromUI(); } }
  if (name === "clear-history" && await confirm("Clear completed and stopped downloads from history? Files will remain on disk.", {title:"Clear history", kind:"warning"})) { await invoke("clear_history"); await refreshState(); }
  if (name === "check-updates" || name === "update-tool") {
    if (updateBusy) return; updateBusy = true; renderUpdateManager();
    try {
      if (name === "update-tool") { updateMessage = await invoke<string>("update_downloader"); await loadTools(); }
      else { updates = await invoke("check_updates"); updateMessage = updates?.appError || "Update check complete."; }
    } catch (error) { updateMessage = String(error); throw error; }
    finally { updateBusy = false; renderUpdateManager(); }
  }
  if (name === "update-app" && updates?.appUrl.startsWith("https://")) await openUrl(updates.appUrl);
}
function applyPanelWidths() {
  const layout = app.querySelector<HTMLElement>(".window");
  const right = app.querySelector<HTMLElement>("#sidepanel");
  if (!layout || !right) return;
  const fitted = fitSidebars(layout.clientWidth, panelWidths.left, panelWidths.right, getComputedStyle(right).display !== "none");
  layout.style.setProperty("--sidebar-width", `${fitted.left}px`);
  layout.style.setProperty("--sidepanel-width", `${fitted.right}px`);
  layout.querySelectorAll<HTMLElement>("[data-resize]").forEach(handle => {
    const side = handle.dataset.resize as "left" | "right";
    handle.setAttribute("aria-valuenow", String(Math.round(fitted[side])));
    handle.setAttribute("aria-valuemin", side === "left" ? "160" : "220");
    handle.setAttribute("aria-valuemax", side === "left" ? "320" : "420");
  });
  return fitted;
}
function bindPanelResizing() {
  app.querySelectorAll<HTMLElement>("[data-resize]").forEach(handle => {
    const side = handle.dataset.resize as "left" | "right";
    let drag: { pointerId: number; x: number; left: number; right: number } | null = null;
    const save = () => localStorage.setItem("ytdown-panels", JSON.stringify(panelWidths));
    handle.addEventListener("pointerdown", event => {
      if (event.button !== 0) return;
      const fitted = applyPanelWidths(); if (!fitted) return;
      event.preventDefault(); handle.focus(); handle.setPointerCapture(event.pointerId);
      drag = { pointerId: event.pointerId, x: event.clientX, ...fitted };
      document.body.classList.add("resizing-panels");
    });
    handle.addEventListener("pointermove", event => {
      if (!drag || event.pointerId !== drag.pointerId) return;
      panelWidths[side] = drag[side] + (event.clientX - drag.x) * (side === "left" ? 1 : -1);
      const fitted = applyPanelWidths(); if (fitted) panelWidths = { left: fitted.left, right: fitted.right || panelWidths.right };
    });
    const finish = () => { if (!drag) return; drag = null; document.body.classList.remove("resizing-panels"); save(); };
    handle.addEventListener("pointerup", finish);
    handle.addEventListener("pointercancel", finish);
    handle.addEventListener("lostpointercapture", finish);
    handle.addEventListener("keydown", event => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      if (event.key === "Home") panelWidths[side] = side === "left" ? 160 : 220;
      else if (event.key === "End") panelWidths[side] = side === "left" ? 320 : 420;
      else panelWidths[side] += (event.key === "ArrowRight" ? 10 : -10) * (side === "left" ? 1 : -1);
      const fitted = applyPanelWidths(); if (fitted) panelWidths = { left: fitted.left, right: fitted.right || panelWidths.right }; save();
    });
    handle.addEventListener("dblclick", () => { panelWidths[side] = side === "left" ? 208 : 280; applyPanelWidths(); save(); });
  });
}
window.addEventListener("resize", () => applyPanelWidths());

function taskContextMenu(task: Task, x: number, y: number) {
  document.querySelector(".file-menu")?.remove();
  const menu = document.createElement("div"); menu.className = "file-menu"; menu.setAttribute("role", "menu");
  menu.innerHTML = `<button role="menuitem" data-queue-action="play" ${task.status !== "Completed" || !task.files.length ? 'disabled title="The downloaded file is not available yet"' : ""}>${icon("play")}<span>Play</span></button><button role="menuitem" data-queue-action="window">${icon("external-link")}<span>Open Window</span></button><button role="menuitem" data-queue-action="rename" ${task.status !== "Completed" || task.files.length !== 1 ? 'disabled title="Rename individual playlist files in Downloads"' : ""}>${icon("pencil")}<span>Rename</span></button><button role="menuitem" data-queue-action="delete" ${active(task) ? 'disabled title="Cancel the download first"' : ""}>${icon("trash-2")}<span>Delete</span></button>`;
  menu.style.left = `${Math.max(8, Math.min(x - 170, innerWidth - 180))}px`;
  menu.style.top = `${Math.max(8, Math.min(y, innerHeight - 140))}px`;
  document.body.append(menu);
  menu.addEventListener("click", event => {
    const target = (event.target as Element).closest<HTMLButtonElement>("[data-queue-action]");
    if (!target || target.disabled) return; menu.remove();
    const run = async () => {
      if (target.dataset.queueAction === "window") await invoke("show_download", {id: task.id});
      if (target.dataset.queueAction === "play") await invoke("media_action", {path: task.files[0], action: "play"});
      if (target.dataset.queueAction === "rename") requestRename(task.files[0]);
      if (target.dataset.queueAction === "delete" && await confirm(task.files.length ? `Delete ${task.files.length === 1 ? task.title : `${task.files.length} downloaded files`} permanently?` : `Remove “${task.title}” from downloads?`, {title: "Delete download", kind: "warning"})) {
        await invoke("delete_download", {id: task.id}); await refreshState();
        if (page === "Home") await loadMedia();
      }
    }; void run().catch(error => setStatus(String(error)));
  });
  menu.querySelector("button")?.focus();
}
function requestRename(path: string) {
  const filename = path.split(/[\\/]/).pop() || "";
  const dot = filename.lastIndexOf(".");
  const stem = dot > 0 ? filename.slice(0, dot) : filename;
  const extension = dot > 0 ? filename.slice(dot) : "";
  const dialog = document.createElement("dialog"); dialog.className = "download-confirmation rename-dialog";
  dialog.setAttribute("aria-labelledby", "rename-title");
  dialog.innerHTML = `<form><header class="confirmation-heading"><h2 id="rename-title">Rename download</h2></header><label class="field-label" for="rename-input">Name</label><div class="rename-input-row"><input id="rename-input" value="${e(stem)}" required autocomplete="off"><span>${e(extension)}</span></div><p class="error-text" id="rename-error" role="alert" hidden></p><footer class="confirmation-footer"><button type="button" class="secondary" data-rename-cancel>Cancel</button><button type="submit" class="primary">Rename</button></footer></form>`;
  document.body.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());
  dialog.querySelector("[data-rename-cancel]")?.addEventListener("click", () => dialog.close());
  dialog.addEventListener("submit", event => {
    event.preventDefault(); const button = dialog.querySelector<HTMLButtonElement>('[type="submit"]')!; button.disabled = true;
    const name = dialog.querySelector<HTMLInputElement>("#rename-input")!.value;
    void invoke("rename_media", {path, name}).then(async () => { dialog.close(); await refreshState(); if (page === "Home") await loadMedia(); setStatus("Download renamed."); }).catch(error => {
      const message = dialog.querySelector<HTMLElement>("#rename-error")!; message.textContent = String(error); message.hidden = false;
    }).finally(() => { button.disabled = false; });
  });
  dialog.showModal(); const input = dialog.querySelector<HTMLInputElement>("#rename-input")!; input.focus(); input.select();
}

function contextMenu(path: string, x: number, y: number) {
  document.querySelector(".file-menu")?.remove(); const menu = document.createElement("div"); menu.className = "file-menu"; menu.setAttribute("role","menu");
  menu.innerHTML = [["play","Play","play"],["folder","Open folder","folder-open"],["copy","Copy path","copy"],["rename","Rename","pencil"],["delete","Delete file","trash-2"]].map(([action,label,glyph]) => `<button role="menuitem" data-file-action="${action}">${icon(glyph)}<span>${label}</span></button>`).join("");
  menu.style.left = `${Math.max(8,Math.min(x,innerWidth-180))}px`; menu.style.top = `${Math.max(8,Math.min(y,innerHeight-170))}px`; document.body.append(menu);
  menu.addEventListener("click", event => { const button = (event.target as Element).closest<HTMLElement>("[data-file-action]"); if (!button) return; menu.remove(); void fileAction(path,button.dataset.fileAction!).catch(err => setStatus(String(err))); }); menu.querySelector("button")?.focus();
}
async function fileAction(path: string, action: string) { if (action === "rename") { requestRename(path); return; } if (action === "copy") { await navigator.clipboard.writeText(path); setStatus("File path copied."); return; } if (action === "delete" && !await confirm(`Delete ${path.split(/[\\/]/).pop()} permanently?`, {title:"Delete file",kind:"warning"})) return; await invoke("media_action",{path,action}); if (action === "delete") await loadMedia(); }
function handleYouTubeSearchInput(input: HTMLInputElement) {
  const popover = app.querySelector<HTMLElement>("#youtube-search-popover");
  if (!input || !popover) return;

  youtubeQuery = input.value.trim();
  app.querySelectorAll<HTMLInputElement>(".youtube-search-input").forEach(other => { if (other !== input) other.value = input.value; });
  app.querySelectorAll<HTMLButtonElement>("[data-youtube-clear]").forEach(button => { button.hidden = !youtubeQuery; });
  window.clearTimeout(youtubeSearchTimer);
  youtubeSearchRequest++;

  if (!youtubeQuery) {
    youtubeResults = [];
    youtubeSearchLoading = false;
    youtubeSearchError = "";
    popover.hidden = true;
    renderYouTubeResults();
    return;
  }

  popover.hidden = false;
  if (isYoutubeLink(youtubeQuery)) {
    youtubeResults = [];
    youtubeSearchLoading = false;
    youtubeSearchError = "";
    renderYouTubeResults();
    return;
  }
  youtubeSearchLoading = true;
  youtubeSearchError = "";
  renderYouTubeResults();
  const requestId = youtubeSearchRequest;
  const query = youtubeQuery;
  youtubeSearchTimer = window.setTimeout(() => { void fetchYouTubeResults(query, requestId); }, 320);
}

async function fetchYouTubeResults(query: string, requestId: number) {
  try {
    const results = await invoke<YouTubeVideo[]>("search_youtube", { query });
    if (requestId !== youtubeSearchRequest) return;
    youtubeResults = results;
    youtubeSearchError = "";
  } catch {
    if (requestId !== youtubeSearchRequest) return;
    youtubeResults = [];
    youtubeSearchError = "YouTube search couldn’t load. Check your connection and try again.";
  }
  youtubeSearchLoading = false;
  renderYouTubeResults();
}

function renderYouTubeResults() {
  const resultContainers = app.querySelectorAll<HTMLElement>(".youtube-results");
  const popover = app.querySelector<HTMLElement>("#youtube-search-popover");
  if (popover) popover.hidden = !youtubeQuery || popover.hidden;
  let markup: string;
  if (youtubeSearchLoading) {
    markup = Array.from({ length: 6 }, () => `<div class="youtube-skeleton"><span></span><div><i></i><i></i></div></div>`).join("");
  } else if (isYoutubeLink(youtubeQuery)) {
    const videoId = youtubeVideoId(youtubeQuery);
    markup = `<button class="youtube-result youtube-link-result" type="button" data-video-url="${e(normalizeYoutubeUrl(youtubeQuery))}" title="Download this YouTube link" ${preparingDownload ? "disabled" : ""}><span class="youtube-thumb">${videoId ? `<img src="https://i.ytimg.com/vi/${e(videoId)}/hqdefault.jpg" alt="" loading="lazy">` : icon("list-video")}</span><span class="youtube-result-copy"><b>Download from YouTube</b><small>Click to add this link to the queue</small></span></button>`;
  } else if (youtubeSearchError) {
    markup = `<p class="youtube-search-message">${e(youtubeSearchError)}</p>`;
  } else if (!youtubeResults.length) {
    markup = `<p class="youtube-search-message">No videos found for “${e(youtubeQuery)}”.</p>`;
  } else {
    markup = youtubeResults.map((video) => `<button class="youtube-result" type="button" data-video-id="${e(video.id)}" title="Download ${e(video.title)}" ${preparingDownload ? "disabled" : ""}><span class="youtube-thumb">${video.thumbnail ? `<img src="${e(video.thumbnail)}" alt="" loading="lazy" />` : icon("file-video")}<small>${e(video.duration)}</small></span><span class="youtube-result-copy"><b>${e(video.title)}</b><small>${e(video.channel)}</small></span></button>`).join("");
  }
  resultContainers.forEach(results => { results.innerHTML = markup; });
  createIcons({ icons: iconSet, attrs: { "stroke-width": 1.8 } });
}

function clearYouTubeSearch(input?: HTMLInputElement) {
  input ||= app.querySelector<HTMLInputElement>("#youtube-search-input") || undefined;
  if (!input) return;
  input.value = "";
  input.focus();
  handleYouTubeSearchInput(input);
}

async function handleYouTubeResultClick(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const result = target.closest<HTMLButtonElement>(".youtube-result");
  const videoId = result?.dataset.videoId;
  if (videoId) await downloadYouTubeVideo(videoId);
  else if (result?.dataset.videoUrl) await downloadYouTubeLink(result.dataset.videoUrl);
}

async function downloadYouTubeVideo(videoId: string) {
  if (preparingDownload) return;
  try {
    const video = youtubeResults.find(video => video.id === videoId);
    await startDownload(`https://www.youtube.com/watch?v=${encodeURIComponent(videoId)}`, settings.defaultQuality, 1, video?.title, video?.thumbnail);
    const popover = app.querySelector<HTMLElement>("#youtube-search-popover");
    if (popover) popover.hidden = true;
  } catch (error) {
    setStatus(String(error));
  }
}
async function downloadYouTubeLink(url: string) {
  if (preparingDownload || playlistInspecting) return;
  playlistInspecting = true;
  try {
    const normalizedUrl = normalizeYoutubeUrl(url);
    setStatus("Checking YouTube link…");
    let playlistStart = 1;
    const info = await invoke<{isPlaylist: boolean; title: string; totalItems: number}>("inspect_playlist", {url: normalizedUrl});
    if (info.isPlaylist) {
      const chosen = await choosePlaylistStart(info.title, info.totalItems);
      if (chosen === null) return;
      playlistStart = chosen;
    }
    await startDownload(normalizedUrl, settings.defaultQuality, playlistStart);
    const popover = app.querySelector<HTMLElement>("#youtube-search-popover");
    if (popover) popover.hidden = true;
  } catch (error) { setStatus(String(error)); }
  finally { playlistInspecting = false; }
}
function choosePlaylistStart(title: string, total: number): Promise<number | null> {
  return new Promise(resolve => {
    const dialog = document.createElement("dialog");
    dialog.className = "download-confirmation playlist-dialog";
    dialog.setAttribute("aria-labelledby", "playlist-title");
    dialog.innerHTML = `<form method="dialog"><header class="confirmation-heading"><div><h2 id="playlist-title">Playlist detected</h2><p class="playlist-description">${e(title)} · ${total} videos</p></div><button type="button" class="quiet-button" data-playlist-cancel aria-label="Cancel">${icon("x")}</button></header><label class="field-label" for="playlist-start">Start at video</label><input id="playlist-start" class="playlist-start-input" type="number" min="1" max="${total}" value="1" required><p class="error-text" id="playlist-error" role="alert" hidden></p><footer class="confirmation-footer"><button type="button" class="secondary" data-playlist-cancel>Cancel</button><button type="submit" class="primary">Start download</button></footer></form>`;
    document.body.append(dialog);
    let result: number | null = null;
    const input = dialog.querySelector<HTMLInputElement>("#playlist-start")!;
    dialog.addEventListener("close", () => { dialog.remove(); resolve(result); }, {once: true});
    dialog.querySelectorAll("[data-playlist-cancel]").forEach(button => button.addEventListener("click", () => dialog.close()));
    dialog.addEventListener("submit", event => {
      event.preventDefault();
      const index = Number(input.value);
      if (!Number.isInteger(index) || index < 1 || index > total) {
        const error = dialog.querySelector<HTMLElement>("#playlist-error")!;
        error.textContent = `Choose a video from 1 to ${total}.`; error.hidden = false; input.focus(); return;
      }
      result = index; dialog.close();
    });
    dialog.showModal(); input.focus(); input.select();
  });
}
function handleSearchKeys(event: KeyboardEvent) {
  const popover = app.querySelector<HTMLElement>("#youtube-search-popover");
  if (event.key === "Escape" && popover && !popover.hidden) {
    popover.hidden = true;
  } else if (event.key === "/" && event.target === document.body) {
    event.preventDefault();
    app.querySelector<HTMLInputElement>("#youtube-search-input")?.focus();
  } else if (event.key === "Enter" && (event.target as Element).matches?.(".youtube-search-input")) {
    if (isYoutubeLink(youtubeQuery)) { event.preventDefault(); void downloadYouTubeLink(youtubeQuery); }
    else if (youtubeResults[0]) { event.preventDefault(); void downloadYouTubeVideo(youtubeResults[0].id); }
  }
}

function closeYouTubePopoverOutside(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Node)) return;
  const search = app.querySelector<HTMLElement>(".search-control-wrap");
  if (search && !search.contains(target)) {
    const popover = app.querySelector<HTMLElement>("#youtube-search-popover");
    if (popover) popover.hidden = true;
  }
}

function setStatus(message: string) {
  const alert = app.querySelector<HTMLElement>("#alert-pill");
  if (!alert) return;
  window.clearTimeout(alertTimer);
  alert.innerHTML = `${icon("info")}<span>${e(message)}</span>`;
  alert.hidden = false;
  createIcons({ icons: iconSet, attrs: { "stroke-width": 1.8 } });
  alertTimer = window.setTimeout(() => { alert.hidden = true; }, 3200);
}

function escapeHtml(text: string) {
  const node = document.createElement("span");
  node.textContent = text;
  return node.innerHTML;
}

function applyTheme(theme: string) { document.documentElement.dataset.theme = theme === "Dark" || (theme === "System" && matchMedia("(prefers-color-scheme: dark)").matches) ? "dark" : "light"; }
app.addEventListener("click", event => {
  const element = event.target as Element;
  const target = element.closest<HTMLElement>("button"); if (!target) return;
  const run = async () => {
    if (target.dataset.progressTab) { progressTab = target.dataset.progressTab; renderDetail(); }
    else if (target.hasAttribute("data-toggle-log")) { progressLogOpen = !progressLogOpen; void getCurrentWindow().setSize(new LogicalSize(500, progressLogOpen ? 420 : 280)).catch(() => {}); renderDetail(); }
    else if (target.hasAttribute("data-task-menu")) { const task = tasks.find(t => t.id === target.closest<HTMLElement>("[data-task]")?.dataset.task); if (task) { const rect = target.getBoundingClientRect(); taskContextMenu(task, rect.right, rect.bottom); } }
    else if (target.hasAttribute("data-close-window")) await getCurrentWindow().close();
    else if (target.dataset.page) navigate(target.dataset.page as Page);
    else if (target.dataset.view) { mediaView = target.dataset.view as "grid" | "list"; localStorage.setItem("ytdown-media-view", mediaView); renderMedia(); app.querySelectorAll<HTMLElement>(".view-switch [data-view]").forEach(button => button.classList.toggle("selected", button.dataset.view === mediaView)); }
    else if (target.dataset.folderPath !== undefined) { currentFolder = target.dataset.folderPath; renderMedia(); }
    else if (target.dataset.openFolder) { currentFolder = target.dataset.openFolder; mediaFilter = ""; const filter = app.querySelector<HTMLInputElement>("#media-filter"); if (filter) filter.value = ""; renderMedia(); }
    else if (target.dataset.nav) { const index = pageIndex + (target.dataset.nav === "back" ? -1 : 1); if (index >= 0 && index < pageStack.length) { pageIndex = index; page = pageStack[index]; render(); if (page === "Home") void loadMedia(); if (page === "Settings") void loadTools(); } }
    else if (target.dataset.action) await handleAction(target.dataset.action);
    else if (target.dataset.show) await invoke("show_download", {id: target.dataset.show});
    else if (target.dataset.taskAction) {
      const task = tasks.find(t => t.id === target.closest<HTMLElement>("[data-task]")?.dataset.task); if (!task) return;
      if (target.dataset.taskAction === "pause") { await invoke("toggle_pause", {id: task.id}); await refreshState(); }
      if (target.dataset.taskAction === "cancel") await invoke("cancel_download", {id: task.id});
      if (target.dataset.taskAction === "show") await invoke("show_download", {id: task.id});
      if (target.dataset.taskAction === "retry") await startDownload(task.url, task.quality, task.playlistStart);
      if (target.dataset.taskAction === "copy") { await navigator.clipboard.writeText(task.url); setStatus("URL copied."); }
    } else if (target.dataset.sort) { const key = target.dataset.sort as keyof Media; sortAsc = sortKey === key ? !sortAsc : true; sortKey = key; renderMedia(); }
    else if (target.hasAttribute("data-media-menu")) { const row = target.closest<HTMLElement>("[data-media]"); if (row) { const rect = target.getBoundingClientRect(); contextMenu(row.dataset.media!,rect.left,rect.bottom); } }
  }; void run().catch(err => setStatus(String(err)));
});
app.addEventListener("input", event => {
  const input = event.target as HTMLInputElement;
  if (input.id === "media-filter") { mediaFilter = input.value; renderMedia(); }
});
app.addEventListener("change", event => {
  const input = event.target as HTMLInputElement | HTMLSelectElement;
  if (!input.closest("#settings-form")) return;
  if (input.id === "theme") applyTheme(input.value);
  if (input.id === "auto-close") app.querySelector<HTMLInputElement>("#auto-close-delay")!.disabled = !(input as HTMLInputElement).checked;
  void saveSettingsFromUI().catch(error => setStatus(`Could not save settings: ${String(error)}`));
});
app.addEventListener("dblclick", event => { const row = (event.target as Element).closest<HTMLElement>("[data-media]"); if (row) void fileAction(row.dataset.media!,"play").catch(err => setStatus(String(err))); });
app.addEventListener("contextmenu", event => { const row = (event.target as Element).closest<HTMLElement>("[data-media]"); if (row) { event.preventDefault(); selectedMedia = row.dataset.media!; contextMenu(selectedMedia,event.clientX,event.clientY); } });
app.addEventListener("keydown", event => { const row = (event.target as Element).closest<HTMLElement>("[data-media]"); if (!row) return; if (event.key === "Enter") { event.preventDefault(); void fileAction(row.dataset.media!, "play").catch(err => setStatus(String(err))); } else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) { event.preventDefault(); const rect = row.getBoundingClientRect(); contextMenu(row.dataset.media!,rect.left+20,rect.top+20); } });
app.addEventListener("click", event => {
  const target = event.target as Element;
  const row = target.closest<HTMLElement>("[data-media]");
  if (!row || target.closest("[data-media-menu]")) return;
  selectedMedia = row.dataset.media!;
  app.querySelectorAll<HTMLElement>("[data-media].is-selected").forEach(item => item.classList.remove("is-selected"));
  row.classList.add("is-selected");
});
document.addEventListener("click", event => { if (!(event.target as Element).closest(".file-menu,[data-media-menu],[data-task-menu]")) document.querySelector(".file-menu")?.remove(); });
document.addEventListener("keydown", event => {
  if (event.key === "Escape") document.querySelector(".file-menu")?.remove();
  if (!(event.ctrlKey || event.metaKey) || detailId || updateWindow) return;
  const key = event.key.toLowerCase(); if (!["d","b","l",",","q"].includes(key)) return; event.preventDefault();
  if (key === "d") { navigate("Home"); app.querySelector<HTMLInputElement>("#youtube-search-input")?.focus(); }
  if (key === "b") navigate("Home");
  if (key === "l") void handleAction("clear-queue").catch(error => setStatus(String(error)));
  if (key === ",") navigate("Settings");
  if (key === "q") void invoke("quit_app");
});
document.addEventListener("keydown", handleSearchKeys);
document.addEventListener("click", closeYouTubePopoverOutside);
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { if (settings.theme === "System") applyTheme("System"); });
async function initialize() {
  try {
    const state = await invoke<{settings: Preferences; tasks: Task[]}>("get_state"); settings = state.settings; tasks = state.tasks;
    if (!localStorage.getItem("ytdown-native-settings-migrated")) {
      const cached = localStorage.getItem("ytdown-settings");
      if (cached) {
        try {
          const previous = JSON.parse(cached) as Partial<Preferences>;
          const folder = previous.downloadFolder;
          const migrated = { ...settings, ...previous, downloadFolder: folder && (/^(\/|[A-Za-z]:[\\/])/.test(folder)) ? folder : settings.downloadFolder };
          await invoke("save_settings", {settings: migrated}); settings = migrated;
        } catch { /* Ignore invalid preferences from earlier interface-only versions. */ }
      }
      localStorage.setItem("ytdown-native-settings-migrated", "true");
    }
    applyTheme(settings.theme); render();
    await listen("downloads-changed", queueRefresh);
    await listen<string>("download-window-error", event => setStatus(event.payload));
    await listen<{received: number; total: number | null}>("tool-update-progress", event => {
      const progress = app.querySelector("#update-progress");
      const value = event.payload.total ? `${Math.round(event.payload.received/event.payload.total*100)}%` : formatBytes(event.payload.received);
      if (progress) progress.textContent = value;
      if (preparingDownload) setStatus(`Installing yt-dlp… ${value}`);
    });
    void loadTools();
    // A snapshot after subscribing covers changes that occurred during startup.
    await refreshState();
    void loadMedia();
    window.setInterval(() => { if (tasks.some(active)) queueRefresh(); },2000);
  } catch(err) { render(); setStatus(`Could not connect to desktop service: ${String(err)}`); }
}
void initialize();
