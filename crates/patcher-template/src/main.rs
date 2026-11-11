#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::{http::Response, WebContext, WebView, WebViewBuilder};

use patcher_core::config::PatcherConfig;
use patcher_core::launcher::{cleanup_self_update_backup, is_running_as_admin};
use patcher_core::overlay::{parse_embedded_assets, read_overlay_from_current_exe};
use patcher_core::{EngineStatus, PatcherEngine};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum IpcAction {
    StartGame,
    OpenSetup,
    StartUpdate,
    ForceRepatch,
    Restart,
    RequestAdmin,
    Close,
    Minimize,
    DragWindow,
}

#[derive(Debug, Clone)]
enum UserEvent {
    StatusUpdate(EngineStatus),
    CloseWindow,
    MinimizeWindow,
    DragWindow,
}

const JS_BRIDGE_SCRIPT: &str = r#"
(function() {
    var _callback = null;
    var _lastStatus = null;
    var _lastData = null;

    window.PatcherAPI = {
        startGame: function() {
            window.ipc.postMessage(JSON.stringify({ action: "start_game" }));
        },
        openSetup: function() {
            window.ipc.postMessage(JSON.stringify({ action: "open_setup" }));
        },
        startUpdate: function() {
            window.ipc.postMessage(JSON.stringify({ action: "start_update" }));
        },
        forceRepatch: function() {
            window.ipc.postMessage(JSON.stringify({ action: "force_repatch" }));
        },
        restart: function() {
            window.ipc.postMessage(JSON.stringify({ action: "restart" }));
        },
        requestAdmin: function() {
            window.ipc.postMessage(JSON.stringify({ action: "request_admin" }));
        },
        close: function() {
            window.ipc.postMessage(JSON.stringify({ action: "close" }));
        },
        minimize: function() {
            window.ipc.postMessage(JSON.stringify({ action: "minimize" }));
        },
        dragWindow: function() {
            window.ipc.postMessage(JSON.stringify({ action: "drag_window" }));
        },
        get onStatusChange() {
            return _callback;
        },
        set onStatusChange(fn) {
            _callback = fn;
            if (fn && _lastStatus !== null) {
                try { fn(_lastStatus, _lastData); } catch (e) { console.error(e); }
            }
        },
        _notify: function(status, data) {
            _lastStatus = status;
            _lastData = data;
            if (_callback) {
                try { _callback(status, data); } catch (e) { console.error(e); }
            }
        }
    };

    // Allow dragging window from any element with data-patcher-drag or class "drag-area",
    // but NEVER intercept clicks on buttons or interactive controls
    document.addEventListener("mousedown", function(e) {
        if (e.target.closest("button") || e.target.closest("a") || e.target.closest("input") || e.target.closest(".no-drag") || e.target.closest(".window-controls")) {
            return;
        }
        if (e.target.closest("[data-patcher-drag]") || e.target.closest(".drag-area")) {
            if (e.button === 0) { // Left click
                window.PatcherAPI.dragWindow();
            }
        }
    });

    // Also support Escape key to close
    document.addEventListener("keydown", function(e) {
        if (e.key === "Escape") {
            window.PatcherAPI.close();
        }
    });
})();
"#;

const DEFAULT_FALLBACK_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Modern RO Patcher Template</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; user-select: none; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            background: linear-gradient(135deg, #1a1c29 0%, #11131a 100%);
            color: #f0f2f5;
            height: 100vh;
            display: flex;
            flex-direction: column;
            border-radius: 12px;
            overflow: hidden;
            border: 1px solid rgba(255, 255, 255, 0.1);
            box-shadow: 0 20px 40px rgba(0,0,0,0.6);
        }
        .titlebar {
            height: 40px;
            background: rgba(0, 0, 0, 0.3);
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 0 16px;
            -webkit-app-region: drag;
        }
        .title { font-size: 13px; font-weight: 600; color: #a0aec0; }
        .window-controls { display: flex; align-items: center; gap: 8px; -webkit-app-region: no-drag; }
        .admin-mode-btn {
            height: 26px;
            padding: 0 10px;
            border-radius: 6px;
            font-size: 11.5px;
            font-weight: 700;
            display: inline-flex;
            align-items: center;
            gap: 6px;
            transition: all 0.2s;
        }
        .admin-mode-btn.user-mode {
            background: rgba(180, 105, 14, 0.32);
            border: 1px solid rgba(245, 158, 11, 0.78);
            color: #fbbf24;
            cursor: pointer;
        }
        .admin-mode-btn.user-mode:hover {
            background: rgba(217, 119, 6, 0.45);
            border-color: #fbbf24;
        }
        .admin-mode-btn.admin-mode {
            background: rgba(16, 120, 88, 0.35);
            border: 1px solid rgba(16, 185, 129, 0.78);
            color: #34d399;
            cursor: default;
        }
        .admin-mode-btn:focus-visible, .control-btn:focus-visible, .btn:focus-visible {
            outline: 2px solid currentColor;
            outline-offset: 2px;
        }
        .control-btn {
            background: none;
            border: none;
            color: #a0aec0;
            width: 24px;
            height: 24px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 14px;
        }
        .control-btn:hover { background: rgba(255,255,255,0.1); color: #fff; }
        .control-btn.close:hover { background: #e53e3e; color: #fff; }
        .content {
            flex: 1;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 40px;
            text-align: center;
        }
        .icon { font-size: 48px; margin-bottom: 16px; }
        h1 { font-size: 20px; margin-bottom: 8px; color: #63b3ed; }
        p { font-size: 14px; color: #a0aec0; line-height: 1.6; max-width: 480px; margin-bottom: 24px; }
        .status-box {
            background: rgba(0, 0, 0, 0.4);
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 8px;
            padding: 16px 24px;
            width: 100%;
            max-width: 500px;
        }
        .status-text { font-size: 13px; margin-bottom: 12px; font-weight: 500; }
        .progress-bar {
            width: 100%;
            height: 8px;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 4px;
            overflow: hidden;
            margin-bottom: 8px;
        }
        .progress-fill {
            height: 100%;
            width: 0%;
            background: linear-gradient(90deg, #3182ce, #63b3ed);
            border-radius: 4px;
            transition: width 0.3s ease;
        }
        .details { font-size: 12px; color: #718096; display: flex; justify-content: space-between; }
        .actions { margin-top: 24px; display: flex; gap: 12px; }
        .btn {
            background: #3182ce;
            color: white;
            border: none;
            padding: 10px 24px;
            border-radius: 6px;
            font-size: 14px;
            font-weight: 600;
            cursor: pointer;
            transition: all 0.2s;
        }
        .btn:hover { background: #2b6cb0; }
        .btn:disabled { opacity: 0.5; cursor: not-allowed; }
        .btn-secondary { background: rgba(255, 255, 255, 0.1); color: #e2e8f0; }
        .btn-secondary:hover { background: rgba(255, 255, 255, 0.18); }
    </style>
</head>
<body>
    <div class="titlebar" data-patcher-drag>
        <div class="title" id="titleText">Modern RO Patcher</div>
        <div class="window-controls no-drag">
            <button type="button" class="admin-mode-btn user-mode no-drag" id="btnAdminMode" onclick="if (!(window.PatcherAPI && window.PatcherAPI.config && window.PatcherAPI.config.is_admin)) { window.PatcherAPI.requestAdmin(); }">
                <svg width="13" height="13" viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M12 2L4 5v6.09c0 5.05 3.41 9.76 8 10.91 4.59-1.15 8-5.86 8-10.91V5l-8-3z" fill="rgba(255,255,255,0.2)" stroke="#000" stroke-width="1.8" stroke-linejoin="round"/>
                    <path d="M12 2L4 5v6.09c0 5.05 3.41 9.76 8 10.91V2z" fill="#000"/>
                </svg>
                <span id="adminModeText">User Mode (คลิกเพื่อขอ Admin)</span>
            </button>
            <button type="button" class="control-btn" onclick="window.PatcherAPI.minimize()">&#8211;</button>
            <button type="button" class="control-btn close" onclick="window.PatcherAPI.close()">&#10005;</button>
        </div>
    </div>
    <div class="content">
        <div class="icon">&#9874;</div>
        <h1>Modern RO Patcher Template</h1>
        <p>This is the unconfigured client patcher template. When built through the Patcher Builder, custom themes, assets, and server configuration will be embedded directly into this executable.</p>
        
        <div class="status-box">
            <div class="status-text" id="statusMessage">Ready</div>
            <div class="progress-bar">
                <div class="progress-fill" id="progressFill"></div>
            </div>
            <div class="details">
                <span id="detailLeft">Status: Idle</span>
                <span id="detailRight">0%</span>
            </div>
        </div>

        <div class="actions">
            <button class="btn btn-secondary" onclick="window.PatcherAPI.openSetup()">Settings</button>
            <button class="btn btn-secondary" onclick="window.PatcherAPI.forceRepatch()">Repatch</button>
            <button class="btn" id="startBtn" onclick="window.PatcherAPI.startGame()">Game Start</button>
        </div>
    </div>

    <script>
        document.addEventListener("DOMContentLoaded", function() {
            const btnAdmin = document.getElementById("btnAdminMode");
            const txtAdmin = document.getElementById("adminModeText");
            if (window.PatcherAPI && window.PatcherAPI.config && window.PatcherAPI.config.is_admin && btnAdmin && txtAdmin) {
                btnAdmin.classList.remove("user-mode");
                btnAdmin.classList.add("admin-mode");
                btnAdmin.setAttribute("aria-disabled", "true");
                txtAdmin.innerText = "Admin Mode (สิทธิ์เต็ม)";
            }
        });
        window.PatcherAPI.onStatusChange = function(status, progress) {
            const statusMsg = document.getElementById("statusMessage");
            const fill = document.getElementById("progressFill");
            const left = document.getElementById("detailLeft");
            const right = document.getElementById("detailRight");
            const startBtn = document.getElementById("startBtn");

            if (status === "checking") {
                statusMsg.innerText = "Checking for updates...";
                left.innerText = "Connecting to patch server...";
            } else if (status === "downloading") {
                const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;
                statusMsg.innerText = `Downloading patch (${progress.current}/${progress.total})`;
                fill.style.width = percent + "%";
                left.innerText = progress.speed || "";
                right.innerText = percent + "%";
            } else if (status === "installing") {
                const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;
                statusMsg.innerText = `Applying patch (${progress.current}/${progress.total})`;
                fill.style.width = percent + "%";
                left.innerText = progress.file || "";
                right.innerText = percent + "%";
            } else if (status === "up_to_date") {
                statusMsg.innerText = "Game is up-to-date! Enjoy playing.";
                fill.style.width = "100%";
                left.innerText = "All patches applied.";
                right.innerText = "100%";
                startBtn.disabled = false;
            } else if (status === "self_updated") {
                statusMsg.innerText = "Patcher updated! Restarting...";
                fill.style.width = "100%";
                left.innerText = "Restarting patcher...";
                right.innerText = "100%";
                setTimeout(() => {
                    window.PatcherAPI.restart();
                }, 1500);
            } else if (status === "error") {
                statusMsg.innerText = "Patch Error: " + progress.error;
                fill.style.background = "#e53e3e";
                left.innerText = "Failed";
            }
        };
    </script>
</body>
</html>
"##;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    // 0. Clean up leftover .exe.old backup from any prior self-update
    cleanup_self_update_backup();

    // 1. Resolve game directory from the executable's own directory first (so shortcuts / Admin elevation work)
    let game_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::env::set_current_dir(&game_dir);

    // 2. Read binary overlay (if present)
    let overlay_data = read_overlay_from_current_exe().unwrap_or(None);
    let (config, assets_map) = match overlay_data {
        Some(bytes) => match parse_embedded_assets(&bytes) {
            Ok(assets) => (assets.config, assets.files),
            Err(e) => {
                log::warn!("Failed parsing embedded assets: {:#}. Falling back to default.", e);
                (PatcherConfig::default(), HashMap::new())
            }
        },
        None => {
            log::info!("No overlay found in binary. Running in template/preview mode.");
            (PatcherConfig::default(), HashMap::new())
        }
    };

    let assets_arc = Arc::new(assets_map);

    // 3. Initialize Tao Event Loop
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // 4. Create Window
    let mut win_builder = WindowBuilder::new()
        .with_title(&config.window.title)
        .with_inner_size(LogicalSize::new(config.window.width, config.window.height))
        .with_resizable(config.window.resizable)
        .with_decorations(config.window.decorations);

    if config.window.transparent {
        win_builder = win_builder.with_transparent(true);
    }

    let window = Arc::new(win_builder.build(&event_loop)?);

    // 5. Initialize Patcher Engine
    let engine = Arc::new(PatcherEngine::new(config.clone(), &game_dir));
    let engine_for_ipc = engine.clone();
    let proxy_for_ipc = proxy.clone();
    let is_patching = Arc::new(AtomicBool::new(false));

    // Custom protocol handler serving embedded assets or fallback HTML
    let assets_for_protocol = assets_arc.clone();
    let protocol_handler = move |_id: wry::WebViewId, req: wry::http::Request<Vec<u8>>| {
        let uri_path = req.uri().path();
        let raw_trimmed = uri_path.trim_start_matches('/');
        let normalized = raw_trimmed
            .trim_start_matches("assets/")
            .trim_start_matches("app/");

        let lookup_key = if normalized.is_empty() {
            "index.html"
        } else {
            normalized
        };

        let found_asset = if !raw_trimmed.is_empty() {
            assets_for_protocol
                .get(raw_trimmed)
                .or_else(|| assets_for_protocol.get(lookup_key))
        } else {
            assets_for_protocol.get(lookup_key)
        };

        if let Some((bytes, mime)) = found_asset {
            Response::builder()
                .header("Content-Type", mime.as_str())
                .body(Cow::Owned(bytes.clone()))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(500)
                        .body(Cow::Borrowed(b"500 Internal Error".as_slice()))
                        .unwrap()
                })
        } else if lookup_key == "index.html" && assets_for_protocol.is_empty() {
            // Default fallback page
            Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .body(Cow::Borrowed(DEFAULT_FALLBACK_HTML.as_bytes()))
                .unwrap()
        } else {
            Response::builder()
                .status(404)
                .body(Cow::Borrowed(b"404 Not Found".as_slice()))
                .unwrap()
        }
    };

    // 6. Build Wry WebView using a dedicated WebContext in %LOCALAPPDATA% so we never pollute the game folder with `<exe>.WebView2`
    let webview_data_dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ModernROPatcher")
        .join("WebView2");
    let mut web_context = WebContext::new(Some(webview_data_dir));

    let is_admin = is_running_as_admin();
    let client_cfg_json = serde_json::json!({
        "title": config.window.title,
        "notice_url": config.server.notice_url,
        "is_admin": is_admin,
    });
    let init_script = format!(
        "{}\nwindow.PatcherAPI.config = {};",
        JS_BRIDGE_SCRIPT, client_cfg_json
    );

    let mut webview_builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_custom_protocol("patcher".into(), protocol_handler)
        .with_initialization_script(&init_script)
        .with_transparent(config.window.transparent);

    // Set starting URL
    webview_builder = webview_builder.with_url("patcher://assets/index.html");

    // IPC handler
    let is_patching_ipc = is_patching.clone();
    webview_builder = webview_builder.with_ipc_handler(move |req| {
        let body_str = req.body();
        if let Ok(action) = serde_json::from_str::<IpcAction>(body_str) {
            match action {
                IpcAction::Close => {
                    let _ = proxy_for_ipc.send_event(UserEvent::CloseWindow);
                }
                IpcAction::Restart => {
                    let _ = engine_for_ipc.restart_self();
                    let _ = proxy_for_ipc.send_event(UserEvent::CloseWindow);
                }
                IpcAction::RequestAdmin => {
                    if !engine_for_ipc.is_running_as_admin() {
                        engine_for_ipc.cancel();
                        match engine_for_ipc.relaunch_as_admin() {
                            Ok(true) => {
                                let _ = proxy_for_ipc.send_event(UserEvent::CloseWindow);
                            }
                            Ok(false) => {
                                log::info!("User cancelled UAC elevation prompt.");
                            }
                            Err(e) => {
                                let _ = proxy_for_ipc.send_event(UserEvent::StatusUpdate(
                                    EngineStatus::Error(format!("Failed to request Admin mode: {:#}", e)),
                                ));
                            }
                        }
                    }
                }
                IpcAction::Minimize => {
                    let _ = proxy_for_ipc.send_event(UserEvent::MinimizeWindow);
                }
                IpcAction::DragWindow => {
                    let _ = proxy_for_ipc.send_event(UserEvent::DragWindow);
                }
                IpcAction::StartGame => {
                    match engine_for_ipc.launch_game() {
                        Ok(()) => {
                            let _ = proxy_for_ipc.send_event(UserEvent::CloseWindow);
                        }
                        Err(e) => {
                            let _ = proxy_for_ipc.send_event(UserEvent::StatusUpdate(EngineStatus::Error(
                                format!("Failed to launch game: {:#}", e),
                            )));
                        }
                    }
                }
                IpcAction::OpenSetup => {
                    let _ = engine_for_ipc.launch_setup();
                }
                IpcAction::StartUpdate => {
                    start_patcher_task(engine_for_ipc.clone(), proxy_for_ipc.clone(), is_patching_ipc.clone());
                }
                IpcAction::ForceRepatch => {
                    let _ = engine_for_ipc.reset_cache();
                    start_patcher_task(engine_for_ipc.clone(), proxy_for_ipc.clone(), is_patching_ipc.clone());
                }
            }
        }
    });

    let webview = webview_builder.build(&window)?;

    // Auto-trigger patch update if servers are configured
    if !config.server.patch_servers.is_empty() {
        start_patcher_task(engine.clone(), proxy.clone(), is_patching.clone());
    }

    // 7. Tao Event Loop Handling
    event_loop.run(move |event, _, control_flow| {
        let _keep_context_alive = &web_context;
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
                std::process::exit(0);
            }
            Event::UserEvent(user_event) => match user_event {
                UserEvent::CloseWindow => {
                    *control_flow = ControlFlow::Exit;
                    std::process::exit(0);
                }
                UserEvent::MinimizeWindow => {
                    window.set_minimized(true);
                }
                UserEvent::DragWindow => {
                    let _ = window.drag_window();
                }
                UserEvent::StatusUpdate(status) => {
                    dispatch_status_to_webview(&webview, &status);
                }
            },
            _ => (),
        }
    });
}

fn start_patcher_task(
    engine: Arc<PatcherEngine>,
    proxy: EventLoopProxy<UserEvent>,
    is_patching: Arc<AtomicBool>,
) {
    if is_patching.swap(true, Ordering::SeqCst) {
        log::warn!("Patching already in progress.");
        return;
    }

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = proxy.send_event(UserEvent::StatusUpdate(EngineStatus::Error(format!(
                    "Failed to create async runtime: {:#}",
                    e
                ))));
                is_patching.store(false, Ordering::SeqCst);
                return;
            }
        };

        rt.block_on(async {
            let proxy_cb = proxy.clone();
            let res = engine
                .run_update(move |status| {
                    let _ = proxy_cb.send_event(UserEvent::StatusUpdate(status));
                })
                .await;

            match res {
                Ok(patch_res) => {
                    if patch_res.self_updated {
                        let _ = proxy.send_event(UserEvent::StatusUpdate(EngineStatus::SelfUpdated));
                    } else {
                        let _ = proxy.send_event(UserEvent::StatusUpdate(EngineStatus::UpToDate));
                    }
                }
                Err(e) => {
                    let _ = proxy.send_event(UserEvent::StatusUpdate(EngineStatus::Error(format!(
                        "{:#}",
                        e
                    ))));
                }
            }

            is_patching.store(false, Ordering::SeqCst);
        });
    });
}

fn dispatch_status_to_webview(webview: &WebView, status: &EngineStatus) {
    let script = match status {
        EngineStatus::Idle => {
            r#"if (window.PatcherAPI && window.PatcherAPI._notify) { window.PatcherAPI._notify("idle", {}); }"#.to_string()
        }
        EngineStatus::CheckingForUpdates => {
            r#"if (window.PatcherAPI && window.PatcherAPI._notify) { window.PatcherAPI._notify("checking", {}); }"#.to_string()
        }
        EngineStatus::Downloading(prog) => {
            let speed_str = format_speed(prog.bytes_per_sec);
            let escaped_file = prog.current_file.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                r#"if (window.PatcherAPI && window.PatcherAPI._notify) {{ window.PatcherAPI._notify("downloading", {{ current: {}, total: {}, speed: "{}", current_file: "{}" }}); }}"#,
                prog.downloaded_files, prog.total_files, speed_str, escaped_file
            )
        }
        EngineStatus::Installing(inst) => {
            let escaped_file = inst.current_patch_name.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                r#"if (window.PatcherAPI && window.PatcherAPI._notify) {{ window.PatcherAPI._notify("installing", {{ current: {}, total: {}, file: "{}" }}); }}"#,
                inst.installed_patches, inst.total_patches, escaped_file
            )
        }
        EngineStatus::UpToDate => {
            r#"if (window.PatcherAPI && window.PatcherAPI._notify) { window.PatcherAPI._notify("up_to_date", {}); }"#.to_string()
        }
        EngineStatus::SelfUpdated => {
            r#"if (window.PatcherAPI && window.PatcherAPI._notify) { window.PatcherAPI._notify("self_updated", {}); }"#.to_string()
        }
        EngineStatus::Error(err) => {
            let escaped = err.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ");
            format!(
                r#"if (window.PatcherAPI && window.PatcherAPI._notify) {{ window.PatcherAPI._notify("error", {{ error: "{}" }}); }}"#,
                escaped
            )
        }
    };

    let _ = webview.evaluate_script(&script);
}

fn format_speed(bps: u64) -> String {
    if bps >= 1024 * 1024 {
        format!("{:.1} MB/s", bps as f64 / (1024.0 * 1024.0))
    } else if bps >= 1024 {
        format!("{:.1} KB/s", bps as f64 / 1024.0)
    } else {
        format!("{} B/s", bps)
    }
}
