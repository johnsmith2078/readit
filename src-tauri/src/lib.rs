use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::BufReader,
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc, Mutex, OnceLock,
    },
    time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use thiserror::Error;
use tokio::{io::AsyncWriteExt, process::Command, time::Duration};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
use windows::{
    core::{Interface, BSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, POINT, RECT, S_OK},
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_APARTMENTTHREADED,
            },
            Threading::{
                GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW,
                PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationTextRange, UIA_TextPatternId,
            },
            WindowsAndMessaging::{
GetCursorPos, GetWindowThreadProcessId, WindowFromPoint,
            },
        },
    },
};

#[derive(Debug, Error)]
enum ReaditError {
    #[error("Windows UI Automation is only implemented on Windows")]
    UnsupportedPlatform,
    #[error("Could not get current mouse position")]
    MousePosition,
    #[error("No accessible UI element found under the cursor")]
    NoElement,
    #[error("The element under the cursor is a password field")]
    PasswordField,
    #[error("The element under the cursor does not expose readable text")]
    NoTextPattern,
    #[error("No readable text found near the cursor")]
    NoText,
    #[error("edge-tts failed: {0}")]
    EdgeTts(String),
    #[error("Audio playback failed: {0}")]
    Audio(String),
    #[error("I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

impl serde::Serialize for ReaditError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
struct CaptureResult {
    text: String,
    source: String,
    bounds: Option<ScreenRect>,
}

#[derive(Debug, Clone, Serialize)]
struct ScreenRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Serialize)]
struct StatusPayload {
    status: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct SpeakPayload {
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct OverlayPayload {
    preview: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SpeakOptions {
    voice: Option<String>,
    rate: Option<String>,
    volume: Option<String>,
}

#[derive(Default)]
struct AppState {
    is_speaking: Mutex<bool>,
    hover_text: Mutex<Option<String>>,
    audio_sink: Mutex<Option<Arc<rodio::Sink>>>,
}

#[cfg(target_os = "windows")]
static MOUSE_HOOK_APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

#[cfg(target_os = "windows")]

#[tauri::command]
async fn capture_text_under_cursor() -> Result<CaptureResult, ReaditError> {
    capture_text_under_cursor_impl()
}

#[tauri::command]
async fn speak_text(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    text: String,
    options: Option<SpeakOptions>,
) -> Result<(), ReaditError> {
    speak_text_impl(app, state.inner().clone(), text, options.unwrap_or_default()).await
}

#[tauri::command]
async fn speak_text_under_cursor(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    options: Option<SpeakOptions>,
) -> Result<CaptureResult, ReaditError> {
    let captured = capture_text_under_cursor_impl()?;
    speak_text_impl(
        app,
        state.inner().clone(),
        captured.text.clone(),
        options.unwrap_or_default(),
    )
    .await?;
    Ok(captured)
}

#[tauri::command]
async fn speak_hover_text(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    options: Option<SpeakOptions>,
) -> Result<String, ReaditError> {
    let text = state
        .hover_text
        .lock()
        .expect("hover text state poisoned")
        .clone()
        .ok_or(ReaditError::NoText)?;

    speak_text_impl(app, state.inner().clone(), text.clone(), options.unwrap_or_default()).await?;
    Ok(text)
}

#[tauri::command]
async fn stop_speaking(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), ReaditError> {
    if stop_current_audio(state.inner()) {
        emit_status(&app, "idle", "Stopped reading");
    }

    #[cfg(target_os = "windows")]
    hide_overlay_if_cursor_left(&app);

    Ok(())
}

pub fn run() {
    let state = Arc::new(AppState::default());

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    let expected = Shortcut::new(
                        Some(Modifiers::CONTROL | Modifiers::ALT),
                        Code::KeyR,
                    );
                    if *shortcut == expected && event.state() == ShortcutState::Pressed {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let Some(state) = app.try_state::<Arc<AppState>>() else {
                                emit_status(&app, "error", "Application state is unavailable");
                                return;
                            };
                            match speak_text_under_cursor(
                                app.clone(),
                                state,
                                Some(SpeakOptions::default()),
                            )
                            .await
                            {
                                Ok(captured) => {
                                    let _ = app.emit("readit://captured", SpeakPayload { text: captured.text });
                                }
                                Err(error) => emit_status(&app, "error", &error.to_string()),
                            }
                        });
                    }
                })
                .build(),
        )
        .setup(|app| {
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyR);
            if let Err(error) = app.global_shortcut().register(shortcut) {
                eprintln!(
                    "Readit could not register Ctrl+Alt+R global shortcut: {error}. The app will continue without the shortcut."
                );

                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    emit_status(
                        &app_handle,
                        "error",
                        "Ctrl+Alt+R is already used by another app. Click-to-read is still active.",
                    );
                });
            }

            setup_overlay_window(app.handle())?;

            #[cfg(target_os = "windows")]
            start_hover_probe(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            capture_text_under_cursor,
            speak_text,
            speak_text_under_cursor,
            speak_hover_text,
            stop_speaking
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Readit");
}




fn setup_overlay_window(app: &AppHandle) -> Result<(), tauri::Error> {
    let overlay = WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("index.html?overlay=1".into()),
    )
    .title("Readit Overlay")
    .inner_size(240.0, 92.0)
    .position(0.0, 0.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .shadow(false)
    .build()?;

    overlay.set_focusable(false)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn start_hover_probe(app: AppHandle) {
    let app_slot = MOUSE_HOOK_APP.get_or_init(|| Mutex::new(None));
    *app_slot.lock().expect("hover probe app slot poisoned") = Some(app.clone());

    std::thread::Builder::new()
        .name("readit-hover-probe".to_string())
        .spawn(move || {
            let mut last_text = String::new();
            let mut last_point = POINT::default();
            let mut stable_ticks = 0u8;
            let mut active_bounds: Option<ScreenRect> = None;

            loop {
                std::thread::sleep(StdDuration::from_millis(180));

                let mut point = POINT::default();
                let got_point = unsafe { GetCursorPos(&mut point).is_ok() };
                if !got_point {
                    if !is_app_speaking(&app) {
                        hide_overlay(&app);
                        last_text.clear();
                        active_bounds = None;
                    }
                    stable_ticks = 0;
                    continue;
                }

                if point_is_in_inactive_app(point) {
                    hide_overlay(&app);
                    last_text.clear();
                    active_bounds = None;
                    stable_ticks = 0;
                    continue;
                }

                if let Some(bounds) = active_bounds.as_ref() {
                    if rect_contains_point(bounds, point, 2) || is_app_speaking(&app) {
                        continue;
                    }

                    hide_overlay(&app);
                    last_text.clear();
                    active_bounds = None;
                    stable_ticks = 0;
                }

                if point_belongs_to_current_process(point) {
                    continue;
                }

                let moved = (point.x - last_point.x).abs() > 3 || (point.y - last_point.y).abs() > 3;
                if moved {
                    last_point = point;
                    stable_ticks = 0;
                    continue;
                }

                stable_ticks = stable_ticks.saturating_add(1);
                if stable_ticks < 2 {
                    continue;
                }

                match capture_text_under_cursor_impl() {
                    Ok(captured) => {
                        let text = sanitize_for_tts(&captured.text);
                        if text.is_empty() || captured.bounds.is_none() {
                            if !is_app_speaking(&app) {
                                hide_overlay(&app);
                                last_text.clear();
                                active_bounds = None;
                            }
                            continue;
                        }

                        if text != last_text {
                            if let Some(state) = app.try_state::<Arc<AppState>>() {
                                *state.hover_text.lock().expect("hover text state poisoned") = Some(text.clone());
                            }
                            show_overlay(&app, captured.bounds.as_ref(), &text);
                            active_bounds = captured.bounds.clone();
                            last_text = text;
                        } else {
                            move_overlay_to_bounds(&app, captured.bounds.as_ref());
                            active_bounds = captured.bounds.clone();
                        }
                    }
                    Err(_) => {
                        if !is_app_speaking(&app) {
                            hide_overlay(&app);
                            last_text.clear();
                            active_bounds = None;
                        }
                    }
                }
            }
        })
        .expect("failed to spawn hover probe thread");
}


#[cfg(target_os = "windows")]
fn rect_contains_point(rect: &ScreenRect, point: POINT, padding: i32) -> bool {
    let left = rect.x.saturating_sub(padding);
    let top = rect.y.saturating_sub(padding);
    let right = rect.x.saturating_add(rect.width as i32).saturating_add(padding);
    let bottom = rect.y.saturating_add(rect.height as i32).saturating_add(padding);

    point.x >= left && point.x <= right && point.y >= top && point.y <= bottom
}

#[cfg(target_os = "windows")]
fn show_overlay(app: &AppHandle, bounds: Option<&ScreenRect>, text: &str) {
    let Some(_) = bounds else {
        hide_overlay(app);
        return;
    };

    move_overlay_to_bounds(app, bounds);
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.emit(
            "readit://overlay-hover",
            OverlayPayload {
                preview: truncate_chars(text, 80),
            },
        );
        let _ = overlay.show();
    }
}

#[cfg(target_os = "windows")]
fn move_overlay_to_bounds(app: &AppHandle, bounds: Option<&ScreenRect>) {
    let Some(bounds) = bounds else {
        return;
    };

    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.set_position(tauri::PhysicalPosition::new(bounds.x, bounds.y));
        let _ = overlay.set_size(tauri::PhysicalSize::new(bounds.width, bounds.height));
    }
}

#[cfg(target_os = "windows")]
fn hide_overlay(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
}

#[cfg(target_os = "windows")]
fn point_is_in_inactive_app(point: POINT) -> bool {
    window_process_name_from_point(point)
        .as_deref()
        .map(is_inactive_process_name)
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn is_inactive_process_name(process_name: &str) -> bool {
    matches!(
        process_name.to_ascii_lowercase().as_str(),
        "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "windowsterminal.exe"
            | "wt.exe"
            | "conhost.exe"
            | "openconsole.exe"
            | "code.exe"
            | "code-insiders.exe"
            | "cursor.exe"
            | "windsurf.exe"
            | "trae.exe"
            | "notepad++.exe"
            | "sublime_text.exe"
            | "atom.exe"
            | "zed.exe"
            | "devenv.exe"
            | "rider64.exe"
            | "idea64.exe"
            | "pycharm64.exe"
            | "webstorm64.exe"
            | "phpstorm64.exe"
            | "clion64.exe"
            | "rustrover64.exe"
            | "goland64.exe"
            | "rubymine64.exe"
            | "eclipse.exe"
            | "notepad.exe"
    )
}

#[cfg(target_os = "windows")]
fn window_process_name_from_point(point: POINT) -> Option<String> {
    unsafe {
        let window = WindowFromPoint(point);
        if window.0.is_null() {
            return None;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(window, Some(&mut process_id));
        if process_id == 0 || process_id == GetCurrentProcessId() {
            return None;
        }

        process_name_from_id(process_id)
    }
}

#[cfg(target_os = "windows")]
unsafe fn process_name_from_id(process_id: u32) -> Option<String> {
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;

    let result = QueryFullProcessImageNameW(
        process,
        PROCESS_NAME_WIN32,
        PWSTR(buffer.as_mut_ptr()),
        &mut size,
    );
    let _ = CloseHandle(process);

    if result.is_err() || size == 0 {
        return None;
    }

    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    PathBuf::from(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

#[cfg(target_os = "windows")]
fn point_belongs_to_current_process(point: POINT) -> bool {
    unsafe {
        let window = WindowFromPoint(point);
        if window.0.is_null() {
            return false;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(window, Some(&mut process_id));
        process_id == GetCurrentProcessId()
    }
}


fn is_app_speaking(app: &AppHandle) -> bool {
    app.try_state::<Arc<AppState>>()
        .map(|state| *state.is_speaking.lock().expect("speaking state poisoned"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn hide_overlay_if_cursor_left(app: &AppHandle) {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point).is_err() } {
        hide_overlay(app);
        return;
    }

    if point_belongs_to_current_process(point) {
        return;
    }

    hide_overlay(app);
}

async fn speak_text_impl(
    app: AppHandle,
    state: Arc<AppState>,
    text: String,
    options: SpeakOptions,
) -> Result<(), ReaditError> {
    let text = sanitize_for_tts(&normalize_text(&text));
    if text.is_empty() {
        return Err(ReaditError::NoText);
    }

    {
        let mut is_speaking = state.is_speaking.lock().expect("speaking state poisoned");
        if *is_speaking {
            return Err(ReaditError::Audio("Another text is already being spoken".into()));
        }
        *is_speaking = true;
    }

    let result = async {
        emit_status(&app, "busy", "Generating speech with edge-tts...");
        let audio_path = synthesize_with_edge_tts(&text, &options).await?;
        emit_status(&app, "busy", "Playing audio...");
        play_audio_file(audio_path, state.clone()).await?;
        emit_status(&app, "idle", "Finished reading");
        Ok::<(), ReaditError>(())
    }
    .await;

    {
        let mut is_speaking = state.is_speaking.lock().expect("speaking state poisoned");
        *is_speaking = false;
    }

    #[cfg(target_os = "windows")]
    hide_overlay_if_cursor_left(&app);

    result
}

async fn synthesize_with_edge_tts(text: &str, options: &SpeakOptions) -> Result<PathBuf, ReaditError> {
    let mut path = std::env::temp_dir();
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ReaditError::EdgeTts(error.to_string()))?
        .as_millis();
    path.push(format!("readit-{millis}.mp3"));

    let voice = options
        .voice
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("zh-CN-XiaoxiaoNeural");
    let rate = options
        .rate
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("+0%");
    let volume = options
        .volume
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("+0%");

    let script = r#"
import asyncio
import pathlib
import sys
import edge_tts

async def main():
    output = pathlib.Path(sys.argv[1])
    voice = sys.argv[2]
    rate = sys.argv[3]
    volume = sys.argv[4]
    text = sys.stdin.buffer.read().decode("utf-8", "ignore")
    text = text.encode("utf-8", "ignore").decode("utf-8", "ignore")
    communicate = edge_tts.Communicate(text, voice=voice, rate=rate, volume=volume)
    await communicate.save(str(output))

asyncio.run(main())
"#;

    let mut command = Command::new("python");
    command
        .arg("-c")
        .arg(script)
        .arg(&path)
        .arg(voice)
        .arg(rate)
        .arg(volume)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|error| ReaditError::EdgeTts(format!("failed to start python: {error}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }

    let output = tokio::time::timeout(Duration::from_secs(45), child.wait_with_output())
        .await
        .map_err(|_| ReaditError::EdgeTts("timed out while generating speech".into()))?
        .map_err(|error| ReaditError::EdgeTts(error.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "edge-tts process exited unsuccessfully".into()
        } else {
            stderr
        };
        return Err(ReaditError::EdgeTts(message));
    }

    Ok(path)
}

async fn play_audio_file(path: PathBuf, state: Arc<AppState>) -> Result<(), ReaditError> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_stream, stream_handle) = rodio::OutputStream::try_default()
            .map_err(|error| ReaditError::Audio(error.to_string()))?;
        let sink = Arc::new(
            rodio::Sink::try_new(&stream_handle)
                .map_err(|error| ReaditError::Audio(error.to_string()))?,
        );
        let file = File::open(&path)?;
        let source = rodio::Decoder::new(BufReader::new(file))
            .map_err(|error| ReaditError::Audio(error.to_string()))?;

        sink.append(source);
        *state.audio_sink.lock().expect("audio sink state poisoned") = Some(sink.clone());
        sink.sleep_until_end();
        *state.audio_sink.lock().expect("audio sink state poisoned") = None;

        let _ = std::fs::remove_file(path);
        Ok::<(), ReaditError>(())
    })
    .await
    .map_err(|error| ReaditError::Audio(error.to_string()))?
}

fn stop_current_audio(state: &Arc<AppState>) -> bool {
    let sink = state
        .audio_sink
        .lock()
        .expect("audio sink state poisoned")
        .take();

    if let Some(sink) = sink {
        sink.stop();
        true
    } else {
        false
    }
}

fn emit_status(app: &AppHandle, status: &str, message: &str) {
    let _ = app.emit(
        "readit://status",
        StatusPayload {
            status: status.to_string(),
            message: message.to_string(),
        },
    );
}

#[cfg(target_os = "windows")]
fn capture_text_under_cursor_impl() -> Result<CaptureResult, ReaditError> {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_err() {
            return Err(ReaditError::MousePosition);
        }

        let _com = ComApartment::init()?;
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| ReaditError::NoElement)?;
        let element = automation
            .ElementFromPoint(point)
            .map_err(|_| ReaditError::NoElement)?;

        if is_password_field(&element) {
            return Err(ReaditError::PasswordField);
        }

        let pattern_unknown = element
            .GetCurrentPattern(UIA_TextPatternId)
            .map_err(|_| ReaditError::NoTextPattern)?;
        let text_pattern: IUIAutomationTextPattern = pattern_unknown
            .cast()
            .map_err(|_| ReaditError::NoTextPattern)?;

        let document_range = text_pattern
            .DocumentRange()
            .map_err(|_| ReaditError::NoTextPattern)?;
        let text = range_text(&document_range, 6000)?;
        let text = best_paragraph(&text).ok_or(ReaditError::NoText)?;
        let bounds = element
            .CurrentBoundingRectangle()
            .ok()
            .and_then(screen_rect_from_rect);

        Ok(CaptureResult {
            text,
            source: "windows-uia".to_string(),
            bounds,
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn capture_text_under_cursor_impl() -> Result<CaptureResult, ReaditError> {
    Err(ReaditError::UnsupportedPlatform)
}


#[cfg(target_os = "windows")]
fn screen_rect_from_rect(rect: RECT) -> Option<ScreenRect> {
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width < 8 || height < 8 {
        return None;
    }

    let padding = 4;
    Some(ScreenRect {
        x: rect.left.saturating_sub(padding),
        y: rect.top.saturating_sub(padding),
        width: (width as u32).saturating_add((padding * 2) as u32).clamp(80, 1200),
        height: (height as u32).saturating_add((padding * 2) as u32).clamp(28, 420),
    })
}

#[cfg(target_os = "windows")]
struct ComApartment;

#[cfg(target_os = "windows")]
impl ComApartment {
    unsafe fn init() -> Result<Self, ReaditError> {
        let result = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if result.is_ok() || result == S_OK {
            Ok(Self)
        } else {
            Err(ReaditError::NoElement)
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn is_password_field(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> bool {
    element.CurrentIsPassword().map(|value| value.as_bool()).unwrap_or(false)
}

#[cfg(target_os = "windows")]


#[cfg(target_os = "windows")]
unsafe fn range_text(range: &IUIAutomationTextRange, max_length: i32) -> Result<String, ReaditError> {
    let text: BSTR = range.GetText(max_length).map_err(|_| ReaditError::NoText)?;
    Ok(text.to_string())
}

fn best_paragraph(text: &str) -> Option<String> {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return None;
    }

    let mut paragraphs = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|item| item.chars().count() >= 8)
        .collect::<Vec<_>>();

    if paragraphs.is_empty() {
        return Some(truncate_chars(&normalized, 1200));
    }

    paragraphs.sort_by_key(|item| std::cmp::Reverse(item.chars().count()));
    Some(truncate_chars(paragraphs[0], 1200))
}

fn normalize_text(text: &str) -> String {
    let mut output = String::new();
    let mut previous_was_blank = false;
    let mut newline_count = 0;

    for character in text.chars() {
        if character == '\r' {
            continue;
        }
        if character == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                output.push('\n');
            }
            previous_was_blank = true;
            continue;
        }
        newline_count = 0;
        if character.is_whitespace() {
            if !previous_was_blank {
                output.push(' ');
            }
            previous_was_blank = true;
        } else {
            output.push(character);
            previous_was_blank = false;
        }
    }

    output.trim().to_string()
}


fn sanitize_for_tts(text: &str) -> String {
    text.chars()
        .filter_map(|character| {
            let code = character as u32;
            let is_surrogate = (0xD800..=0xDFFF).contains(&code);
            let is_private_use = (0xE000..=0xF8FF).contains(&code);
            let is_noncharacter = (0xFDD0..=0xFDEF).contains(&code) || code & 0xFFFE == 0xFFFE;
            let is_disallowed_control = character.is_control()
                && character != '\n'
                && character != '\t'
                && character != ' ';

            if is_surrogate || is_private_use || is_noncharacter || is_disallowed_control {
                None
            } else {
                Some(character)
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut value = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        value.push_str("...");
    }
    value
}
