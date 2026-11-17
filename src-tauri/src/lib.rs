mod extension_server;
mod media_server;
mod service;
use tauri::Manager;
// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use serde::Serialize;
use serde_json::Value;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Serialize)]
struct YouTubeVideo {
    id: String,
    title: String,
    channel: String,
    duration: String,
    thumbnail: String,
}

#[tauri::command]
async fn search_youtube(query: String) -> Result<Vec<YouTubeVideo>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .build()
        .map_err(|error| error.to_string())?;

    let homepage = client
        .get("https://www.youtube.com/")
        .send()
        .await
        .map_err(|_| "Could not connect to YouTube.".to_string())?
        .error_for_status()
        .map_err(|_| "YouTube did not accept the search request.".to_string())?
        .text()
        .await
        .map_err(|_| "Could not read YouTube's search configuration.".to_string())?;

    let api_key = config_string(&homepage, "INNERTUBE_API_KEY")
        .ok_or_else(|| "Could not read YouTube's search configuration.".to_string())?;
    let client_version = config_string(&homepage, "INNERTUBE_CLIENT_VERSION")
        .unwrap_or_else(|| "2.20250101.00.00".to_string());

    let response = client
        .post(format!(
            "https://www.youtube.com/youtubei/v1/search?key={api_key}"
        ))
        .header("Origin", "https://www.youtube.com")
        .header("Referer", "https://www.youtube.com/")
        .json(&serde_json::json!({
            "context": { "client": { "clientName": "WEB", "clientVersion": client_version } },
            "query": query
        }))
        .send()
        .await
        .map_err(|_| "YouTube search could not be reached.".to_string())?
        .error_for_status()
        .map_err(|_| "YouTube rejected the search. Try again in a moment.".to_string())?
        .json::<Value>()
        .await
        .map_err(|_| "YouTube returned an unreadable search response.".to_string())?;

    let mut videos = Vec::new();
    collect_video_renderers(&response, &mut videos);
    videos.truncate(12);
    Ok(videos)
}

fn config_string(source: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = source.find(&marker)? + marker.len();
    let value = &source[start..];
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn text_value(value: &Value) -> String {
    value
        .get("simpleText")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value.get("runs").and_then(Value::as_array).map(|runs| {
                runs.iter()
                    .filter_map(|run| run.get("text").and_then(Value::as_str))
                    .collect::<String>()
            })
        })
        .unwrap_or_default()
}

fn collect_video_renderers(value: &Value, videos: &mut Vec<YouTubeVideo>) {
    if videos.len() >= 12 {
        return;
    }

    if let Some(renderer) = value.get("videoRenderer") {
        let id = renderer
            .get("videoId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let title = renderer.get("title").map(text_value).unwrap_or_default();
        if !id.is_empty() && !title.is_empty() {
            let channel = renderer
                .get("ownerText")
                .map(text_value)
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "YouTube".to_string());
            let duration = renderer
                .get("lengthText")
                .map(text_value)
                .unwrap_or_default();
            let thumbnail = renderer
                .get("thumbnail")
                .and_then(|thumb| thumb.get("thumbnails"))
                .and_then(Value::as_array)
                .and_then(|items| items.last())
                .and_then(|item| item.get("url"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .replace("&amp;", "&");

            videos.push(YouTubeVideo {
                id: id.to_string(),
                title,
                channel,
                duration,
                thumbnail,
            });
        }
    }

    match value {
        Value::Array(items) => {
            for item in items {
                collect_video_renderers(item, videos);
            }
        }
        Value::Object(items) => {
            for item in items.values() {
                collect_video_renderers(item, videos);
            }
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let service = service::Service::new(app.handle()).map_err(std::io::Error::other)?;
            service::allow_playback_directories(app.handle(), &service)
                .map_err(std::io::Error::other)?;
            app.manage(media_server::MediaServer::start()?);
            app.manage(service);
            service::schedule(app.handle().clone());

            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            let open = MenuItem::with_id(app, "open", "Open YT_DOWN", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| std::io::Error::other("The application tray icon is missing"))?;
            TrayIconBuilder::with_id("ytdown-tray")
                .menu(&menu)
                .tooltip("YT_DOWN · YouTube Downloader")
                .icon(icon)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let main_window = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_window.hide();
                    }
                });
            }
            extension_server::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            search_youtube,
            service::get_state,
            service::save_settings,
            service::enqueue_download,
            service::inspect_playlist,
            service::cancel_download,
            service::clear_history,
            service::clear_queue,
            service::rename_media,
            service::delete_download,
            service::toggle_pause,
            service::show_download,
            service::show_updates,
            service::list_media,
            service::prepare_media_playback,
            service::media_action,
            service::open_folder,
            service::tool_status,
            service::update_downloader,
            service::check_updates,
            service::quit_app
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                service::stop(app);
            }
        });
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
