#![allow(dead_code, unexpected_cfgs)]

mod app;
mod config;
mod input;
mod renderer;
mod terminal;
mod ui;
mod ssh;

use std::sync::Arc;
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
                        use cocoa::foundation::NSString as NSStringTrait;
                        use objc::runtime::{Object, YES};
                        use objc::{msg_send, sel, sel_impl, class};
                        let ns_view: *mut Object = appkit.ns_view.as_ptr() as *mut Object;
                        let ns_window: *mut Object = msg_send![ns_view, window];
                        let _: () = msg_send![ns_window, setTitlebarAppearsTransparent: YES];
                        let name = cocoa::foundation::NSString::alloc(cocoa::base::nil)
                            .init_str("NSAppearanceNameVibrantDark");
                        let appearance: *mut Object = msg_send![
                            class!(NSAppearance),
                            appearanceNamed: name
                        ];
                        let _: () = msg_send![ns_window, setAppearance: appearance];
                    }
                }
            }
        }

        self.window = Some(window.clone());

        let config = self.config.clone();
        let app = pollster::block_on(App::new(window, config));
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
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                app.resize(size.width, size.height);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                app.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = (position.x, position.y);
                let s = app.scale_factor;
                app.mouse_move(position.x as f32 * s, position.y as f32 * s);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.cursor_pos;
                let s = app.scale_factor;
                let px = x as f32 * s;
                let py = y as f32 * s;
                match (button, state) {
                    (winit::event::MouseButton::Left, winit::event::ElementState::Pressed) => {
                        let now = std::time::Instant::now();
                        let dt = now.duration_since(self.last_click).as_millis();
                        let (lx, ly) = self.last_click_pos;
                        let dist = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                        if app.check_zoom_buttons(px, py) {
                            // Handled by UI button, skip double-click logic
                        } else if dt < 400 && dist < 10.0 {
                            let (cx, cy) = app.screen_to_canvas(px, py);
                            match app.canvas.hit_test(cx, cy, app.scale_factor) {
                                Some((_, true, _)) => app.start_rename(),
                                None => app.spawn_tile_at(cx, cy),
                                _ => {}
                            }
                        } else {
                            app.mouse_down(px, py);
                        }
                        self.last_click = now;
                        self.last_click_pos = (x, y);
                    }
                    (winit::event::MouseButton::Left, winit::event::ElementState::Released) => {
                        app.mouse_up();
                    }
                    (winit::event::MouseButton::Middle, winit::event::ElementState::Pressed) => {
                        app.middle_mouse_down(px, py);
                    }
                    (winit::event::MouseButton::Middle, winit::event::ElementState::Released) => {
                        app.middle_mouse_up();
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y_delta = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 20.0,
                };
                if app.modifiers.super_key() {
                    let s = app.scale_factor;
                    let (mx, my) = self.cursor_pos;
                    let step = y_delta * 0.05;
                    app.zoom_at(mx as f32 * s, my as f32 * s, step);
                } else {
                    let lines = y_delta as i32 * 3;
                    if lines != 0 {
                        app.scroll(lines);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                match app.handle_key_event(&event) {
                    AppAction::SpawnTile => app.spawn_tile(),
                    AppAction::ClosePane => app.close_focused(),
                    AppAction::Quit => event_loop.exit(),
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

    let config = Config::load();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut lume = LumeApp::new(config);
    event_loop.run_app(&mut lume).expect("Event loop error");
}
