#![allow(dead_code, unexpected_cfgs)]

mod app;
mod config;
mod http_pane;
mod input;
mod pane;
mod postgres_pane;
mod renderer;
mod state;
mod terminal;
mod ui;
mod ssh;

use std::sync::Arc;
use std::fs::File;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::{App, AppAction};
use crate::config::Config;

struct LumeApp {
    config: Config,
    app: Option<App>,
    window: Option<Arc<Window>>,
    cursor_pos: (f64, f64),
    last_click: std::time::Instant,
    last_click_pos: (f64, f64),
}

impl LumeApp {
    fn new(config: Config) -> Self {
        Self {
            config,
            app: None,
            window: None,
            cursor_pos: (0.0, 0.0),
            last_click: std::time::Instant::now(),
            last_click_pos: (0.0, 0.0),
        }
    }
}

impl ApplicationHandler for LumeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("SunnyTerm")
            .with_inner_size(winit::dpi::LogicalSize::new(1344.0, 900.0));

        let window = Arc::new(event_loop.create_window(attrs).expect("Failed to create window"));

        #[cfg(target_os = "macos")]
        {
            use winit::raw_window_handle::HasWindowHandle;
            if let Ok(handle) = window.window_handle() {
                if let winit::raw_window_handle::RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                    #[allow(deprecated, unexpected_cfgs)]
                    unsafe {
                        use objc::runtime::{Object, YES};
                        use objc::{msg_send, sel, sel_impl, class};
                        let ns_view: *mut Object = appkit.ns_view.as_ptr() as *mut Object;
                        let ns_window: *mut Object = msg_send![ns_view, window];
                        let _: () = msg_send![ns_window, setTitlebarAppearsTransparent: YES];

                        // Disable default Cmd+Q so it goes through our event loop
                        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
                        let main_menu: *mut Object = msg_send![app, mainMenu];
                        if !main_menu.is_null() {
                            let count: i64 = msg_send![main_menu, numberOfItems];
                            for i in 0..count {
                                let item: *mut Object = msg_send![main_menu, itemAtIndex: i];
                                let submenu: *mut Object = msg_send![item, submenu];
                                if !submenu.is_null() {
                                    let sub_count: i64 = msg_send![submenu, numberOfItems];
                                    for j in (0..sub_count).rev() {
                                        let sub_item: *mut Object = msg_send![submenu, itemAtIndex: j];
                                        let action: objc::runtime::Sel = msg_send![sub_item, action];
                                        if action == sel!(terminate:) {
                                            let _: () = msg_send![submenu, removeItemAtIndex: j];
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        self.window = Some(window.clone());

        let config = self.config.clone();
        let app = pollster::block_on(App::new(window.clone(), config));

        // Set initial titlebar appearance based on saved theme
        app::set_macos_appearance(&window, app.is_dark);

        self.app = Some(app);
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::Poll) {
            if let Some(app) = &mut self.app {
                app.read_all_ptys();
                app.request_redraw();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = &mut self.app else { return };

        match event {
            WindowEvent::CloseRequested => {
                if confirm_quit() {
                    app.save_state();
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                app.resize(size.width, size.height);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                app.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = (position.x, position.y);
                app.mouse_move(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.cursor_pos;
                let px = x as f32;
                let py = y as f32;
                match (button, state) {
                    (winit::event::MouseButton::Left, winit::event::ElementState::Pressed) => {
                        // Ctrl+Click: open URL under cursor
                        if app.modifiers.super_key() {
                            let (cx, cy) = app.screen_to_canvas(px, py);
                            if let Some(url) = app.url_at_canvas_pos(cx, cy) {
                                let _ = std::process::Command::new("open").arg(&url).spawn();
                            }
                            self.last_click = std::time::Instant::now();
                            self.last_click_pos = (x, y);
                        } else {
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(self.last_click).as_millis();
                            let (lx, ly) = self.last_click_pos;
                            let dist = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                            if app.check_zoom_buttons(px, py) {
                                // Handled by UI button, skip double-click logic
                            } else if dt < 400 && dist < 10.0 {
                                let (cx, cy) = app.screen_to_canvas(px, py);
                                match app.canvas.hit_test(cx, cy, app.scale_factor) {
                                    Some((_, true, _, _)) => app.start_rename(),
                                    None => app.spawn_tile_at(cx, cy),
                                    _ => {}
                                }
                            } else {
                                app.mouse_down(px, py);
                            }
                            self.last_click = now;
                            self.last_click_pos = (x, y);
                        }
                    }
                    (winit::event::MouseButton::Left, winit::event::ElementState::Released) => {
                        app.mouse_up();
                    }
                    (winit::event::MouseButton::Middle | winit::event::MouseButton::Right, winit::event::ElementState::Pressed) => {
                        app.middle_mouse_down(px, py);
                    }
                    (winit::event::MouseButton::Middle | winit::event::MouseButton::Right, winit::event::ElementState::Released) => {
                        app.middle_mouse_up();
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x_delta, y_delta) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                if app.modifiers.super_key() {
                    // Cmd + scroll = zoom
                    let (mx, my) = self.cursor_pos;
                    let step = if y_delta > 0.0 { 0.1 } else { -0.1 };
                    app.zoom_at(mx as f32, my as f32, step);
                } else if app.cursor_over_tile(self.cursor_pos) {
                    // Scroll tile content when cursor is over a tile
                    let lines = if y_delta.abs() > 0.0 {
                        (y_delta / 20.0).round() as i32
                    } else {
                        0
                    };
                    if lines != 0 {
                        if app.modifiers.shift_key() {
                            // Shift+scroll = horizontal scroll
                            app.scroll_x(-lines);
                        } else {
                            app.scroll(lines);
                        }
                    }
                    // Trackpad horizontal scroll
                    let h_lines = if x_delta.abs() > 0.0 {
                        (x_delta / 20.0).round() as i32
                    } else {
                        0
                    };
                    if h_lines != 0 {
                        app.scroll_x(-h_lines);
                    }
                } else {
                    // Scroll on empty canvas = pan
                    let dx = x_delta / app.canvas_zoom;
                    let dy = y_delta / app.canvas_zoom;
                    app.canvas_pan.0 -= dx;
                    app.canvas_pan.1 -= dy;
                    app.update_projection();
                }
            }
            WindowEvent::PinchGesture { delta, .. } => {
                let (mx, my) = self.cursor_pos;
                let step = delta as f32 * 0.6;
                app.zoom_at(mx as f32, my as f32, step);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Track space key for space+drag panning
                if let winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) = &event.logical_key {
                    app.space_held = event.state == winit::event::ElementState::Pressed;
                }
                if event.state != winit::event::ElementState::Pressed {
                    // Don't process key releases through handle_key_event
                    return;
                }
                match app.handle_key_event(&event) {
                    AppAction::SpawnTile => app.spawn_tile(),
                    AppAction::SpawnHttpTile => {
                        let (cx, cy) = app.screen_to_canvas(self.cursor_pos.0 as f32, self.cursor_pos.1 as f32);
                        app.spawn_http_tile_at(cx, cy);
                    }
                    AppAction::SpawnPostgresTile => {
                        let (cx, cy) = app.screen_to_canvas(self.cursor_pos.0 as f32, self.cursor_pos.1 as f32);
                        app.spawn_postgres_tile_at(cx, cy);
                    }
                    AppAction::ClosePane => app.close_focused(),
                    AppAction::Quit => {
                        if confirm_quit() {
                            app.save_state();
                            event_loop.exit();
                        }
                    }
                    AppAction::None => {}
                }
            }
            WindowEvent::RedrawRequested => {
                match app.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => {
                        let size = self.window.as_ref().unwrap().inner_size();
                        app.resize(size.width, size.height);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        event_loop.exit();
                    }
                    Err(e) => log::error!("Render error: {e}"),
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();

    // Single instance lock
    let lock_path = crate::state::AppState::data_dir().join("sunnyterm.lock");
    let lock_file = File::create(&lock_path).expect("Failed to create lock file");
    use std::os::unix::io::AsRawFd;
    let fd = lock_file.as_raw_fd();
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if ret != 0 {
        eprintln!("[sunnyterm] Another instance is already running.");
        std::process::exit(0);
    }
    // Keep lock_file alive for the duration of the process
    let _lock = lock_file;

    let config = Config::load();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut lume = LumeApp::new(config);
    event_loop.run_app(&mut lume).expect("Event loop error");
}

/// Show a native macOS confirmation dialog before quitting.
fn confirm_quit() -> bool {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(r#"display dialog "Are you sure you want to quit SunnyTerm?" buttons {"Cancel", "Quit"} default button "Cancel" cancel button "Cancel" with icon caution with title "SunnyTerm""#)
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => true, // If osascript fails, allow quit
    }
}
