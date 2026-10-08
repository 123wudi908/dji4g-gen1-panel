//! Interactive Windows acceptance: real eframe HWND, settings clicks and DWM readback.
//! Run each ignored test separately on an unlocked desktop (winit owns one event loop).
#![cfg(all(windows, debug_assertions))]

use dji4g_application::{
    AutostartKnownState, AutostartStatus, Controller, ThemeCode, UiCommand, UiSendError,
};
use dji4g_panel::{
    app::{Page, PanelApp, PanelInputs, SettingsBackend, UiCommandSink},
    config::{ConfigError, ConfigPaths, ConfigStore, ConfigV1},
};
use dji4g_windows_platform::{AutostartObservedState, PlatformError};
use eframe::egui;
use std::{
    ffi::c_void,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmGetWindowAttribute(hwnd: isize, attribute: u32, value: *mut c_void, size: u32) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn IsIconic(hwnd: isize) -> i32;
    fn IsZoomed(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, command: i32) -> i32;
    fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
    fn SendMessageW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
}

struct Commands(
    Mutex<Controller>,
    dji4g_application::sync::watch::Sender<Arc<dji4g_application::ControllerSnapshot>>,
);
impl UiCommandSink for Commands {
    fn try_send(&self, command: UiCommand) -> Result<(), UiSendError> {
        let mut controller = self.0.lock().unwrap();
        controller
            .handle_command(command)
            .expect("settings command accepted");
        self.1.send(Arc::new(controller.snapshot())).unwrap();
        Ok(())
    }
}
struct Settings(ConfigStore);
impl SettingsBackend for Settings {
    fn save_config(&self, config: &ConfigV1) -> Result<(), ConfigError> {
        self.0.save(config)
    }
    fn set_autostart(&self, _: bool) -> Result<AutostartObservedState, PlatformError> {
        panic!("theme acceptance must never change autostart")
    }
}

fn dark_attribute(hwnd: isize) -> Result<bool, String> {
    for attribute in [20, 19] {
        let mut value = 0i32;
        // SAFETY: valid live eframe HWND; writable BOOL and exact byte size.
        let result =
            unsafe { DwmGetWindowAttribute(hwnd, attribute, (&mut value as *mut i32).cast(), 4) };
        if result >= 0 {
            return Ok(value != 0);
        }
    }
    Err("DWM dark-mode readback unsupported on this desktop".into())
}

struct Acceptance {
    panel: PanelApp,
    hwnd: isize,
    initial: ThemeCode,
    step: usize,
    deadline: Instant,
    pending_click: Option<(egui::Pos2, bool)>,
    result: Arc<Mutex<Result<(), String>>>,
    store: ConfigStore,
    startup_error: Option<String>,
    restore_worker: Option<std::thread::JoinHandle<bool>>,
}
impl Acceptance {
    fn check(&self, dark: bool) -> Result<(), String> {
        let actual = dark_attribute(self.hwnd)?;
        if actual == dark {
            Ok(())
        } else {
            Err(format!(
                "step {}: native titlebar dark={actual}, expected {dark}",
                self.step
            ))
        }
    }
    fn text_position(ctx: &egui::Context, label: &str) -> Result<egui::Pos2, String> {
        // Clone paint lists: observing labels must not consume the real rendered frame.
        let shapes = ctx.graphics(|layers| layers.clone().drain(&[], &Default::default()));
        shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .ok_or_else(|| format!("settings label not visible: {label}"))
    }
    fn click_label(&mut self, ctx: &egui::Context, label: &str) -> Result<(), String> {
        self.pending_click = Some((Self::text_position(ctx, label)?, true));
        Ok(())
    }
    fn capture(&self, label: &str) {
        if let Ok(script) = std::env::var("DJI4G_TITLEBAR_CAPTURE") {
            let status = std::process::Command::new("python")
                .args([&script, &self.hwnd.to_string(), label])
                .status()
                .unwrap();
            assert!(status.success(), "native screenshot capture failed");
        }
    }
    fn advance(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if let Some(error) = self.startup_error.take() {
            return Err(error);
        }
        let dark = self.initial == ThemeCode::Dark;
        let current = if dark { "深色" } else { "浅色" };
        let other = if dark { "浅色" } else { "深色" };
        match self.step {
            0 => {
                self.check(dark)?;
                self.capture(&format!("{current}-startup"));
                self.click_label(ctx, current)?;
            }
            1 => self.click_label(ctx, other)?,
            2 => {
                self.check(!dark)?;
                let saved = ConfigStore::<
                    dji4g_panel::config::StdFileOps,
                    dji4g_panel::config::SystemClock,
                >::decode(
                    &std::fs::read(&self.store.paths().config_file).unwrap()
                )
                .unwrap();
                assert_eq!(
                    saved.theme,
                    if dark {
                        ThemeCode::Light
                    } else {
                        ThemeCode::Dark
                    }
                );
                self.capture(&format!("{current}-to-{other}"));
                self.click_label(ctx, other)?;
            }
            3 => self.click_label(ctx, current)?,
            4 => {
                self.check(dark)?;
                self.capture(&format!("{other}-to-{current}"));
                // Real native system command, same window path as the maximize button.
                unsafe {
                    SendMessageW(self.hwnd, 0x0112, 0xF030, 0);
                }
            }
            5 => {
                if unsafe { IsZoomed(self.hwnd) } == 0 {
                    return Err("maximize failed".into());
                }
                self.check(dark)?;
                self.capture(&format!("{current}-maximized"));
                unsafe {
                    SendMessageW(self.hwnd, 0x0112, 0xF120, 0);
                }
            }
            6 => {
                if unsafe { IsZoomed(self.hwnd) } != 0 {
                    return Err("restore failed".into());
                }
                self.check(dark)?;
                let hwnd = self.hwnd;
                // eframe can stop painting while minimized; a test worker observes and restores
                // this test's own HWND, then wakes the event loop.
                let ctx = ctx.clone();
                self.restore_worker = Some(std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(400));
                    let minimized = unsafe { IsIconic(hwnd) } != 0;
                    unsafe {
                        ShowWindow(hwnd, 9);
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                    minimized
                }));
                unsafe {
                    SendMessageW(self.hwnd, 0x0112, 0xF020, 0);
                }
            }
            7 => {
                if !self
                    .restore_worker
                    .take()
                    .unwrap()
                    .join()
                    .map_err(|_| "restore worker failed")?
                {
                    return Err("minimize failed".into());
                }
                if unsafe { IsIconic(self.hwnd) } != 0 {
                    return Err("restore from minimize failed".into());
                }
                self.check(dark)?;
                self.capture(&format!("{current}-restored"));
                let style = unsafe { GetWindowLongW(self.hwnd, -16) };
                if style & 0x00CF0000 != 0x00CF0000 {
                    return Err(format!(
                        "native caption/system/min/max style missing: {style:x}"
                    ));
                }
                if unsafe { SendMessageW(self.hwnd, 0x007F, 2, 0) } == 0 {
                    return Err("native titlebar icon missing".into());
                }
                let saved = ConfigStore::<
                    dji4g_panel::config::StdFileOps,
                    dji4g_panel::config::SystemClock,
                >::decode(
                    &std::fs::read(&self.store.paths().config_file).unwrap()
                )
                .unwrap();
                assert_eq!(saved.theme, self.initial);
                *self.result.lock().unwrap() = Ok(());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
        println!("native acceptance step {} passed", self.step);
        self.step += 1;
        self.deadline = Instant::now() + Duration::from_millis(700);
        Ok(())
    }
}
impl eframe::App for Acceptance {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.panel.clear_color(visuals)
    }
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        if let Some((pos, pressed)) = self.pending_click.take() {
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            if pressed {
                self.pending_click = Some((pos, false));
            }
        }
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.step > 7 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.panel.update(ctx, frame);
        // The native preference must match on the same frame that resolves the UI theme.
        if let Err(error) = self.check(ctx.style().visuals.dark_mode) {
            *self.result.lock().unwrap() = Err(error);
            self.step = 99;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if Instant::now() >= self.deadline {
            if let Err(error) = self.advance(ctx) {
                self.capture("failed-before-fix");
                *self.result.lock().unwrap() = Err(error);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.step = 99;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}

fn acceptance(initial: ThemeCode) {
    let directory = tempfile::tempdir().unwrap();
    let paths = std::env::var_os("DJI4G_TITLEBAR_CONFIG").map_or_else(
        || ConfigPaths::under_root(directory.path()),
        |path| ConfigPaths::under_root(std::path::Path::new(&path)),
    );
    let store = ConfigStore::new(paths);
    let config = if store.paths().config_file.exists() {
        ConfigStore::<dji4g_panel::config::StdFileOps, dji4g_panel::config::SystemClock>::decode(
            &std::fs::read(&store.paths().config_file).unwrap(),
        )
        .unwrap()
    } else {
        ConfigV1 {
            theme: initial,
            onboarding_completed: true,
            ..Default::default()
        }
    };
    assert_eq!(
        config.theme, initial,
        "restart must restore the stored theme"
    );
    store.save(&config).unwrap();
    let mut controller = Controller::for_test(SystemTime::now());
    controller.set_theme(config.theme);
    controller.set_autostart_state(AutostartStatus::Ready(AutostartKnownState::Disabled));
    let (tx, rx) = dji4g_application::sync::watch::channel(Arc::new(controller.snapshot()));
    let commands = Arc::new(Commands(Mutex::new(controller), tx));
    let inputs = PanelInputs::new(
        rx,
        commands,
        None,
        Some(Arc::new(Settings(ConfigStore::new(store.paths().clone())))),
    );
    let result = Arc::new(Mutex::new(Err("acceptance did not finish".into())));
    let output = result.clone();
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/brand/icon.png")).unwrap();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 780.0])
            .with_icon(icon),
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        dji4g_panel::app::PANEL_WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            let mut panel = PanelApp::new(inputs, cc);
            panel.configure_onboarding(&config);
            panel.set_review_page(Page::Settings);
            let hwnd = dji4g_windows_platform::tray::find_process_window(
                dji4g_panel::app::PANEL_WINDOW_TITLE,
            )
            .expect("real native panel HWND");
            // Inspect immediately, before the first update, so startup wiring is covered too.
            let startup = dark_attribute(hwnd).and_then(|dark| {
                if dark == (initial == ThemeCode::Dark) {
                    Ok(())
                } else {
                    Err(format!(
                        "startup before first frame: native dark={dark}, expected {initial:?}"
                    ))
                }
            });
            Ok(Box::new(Acceptance {
                panel,
                hwnd,
                initial,
                step: 0,
                deadline: Instant::now() + Duration::from_secs(1),
                pending_click: None,
                result: output,
                store,
                startup_error: startup.err(),
                restore_worker: None,
            }))
        }),
    )
    .unwrap();
    let verdict = result.lock().unwrap().clone();
    assert!(verdict.is_ok(), "{verdict:?}");
}

#[test]
#[ignore = "requires unlocked Windows desktop; run separately from the light test"]
fn native_dark_startup_and_theme_switches() {
    acceptance(ThemeCode::Dark);
}

#[test]
#[ignore = "requires unlocked Windows desktop; run separately from the dark test"]
fn native_light_startup_and_theme_switches() {
    acceptance(ThemeCode::Light);
}
