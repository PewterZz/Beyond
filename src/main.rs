use anyhow::Result;
use beyonder_config::BeyonderConfig;
use beyonder_ui::App;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

fn main() -> Result<()> {
    // Use RUST_LOG / BEYONDER_LOG as-is when set; fall back to sensible defaults.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("beyonder=info,wgpu_core=warn,wgpu_hal=warn"));
    // Write to stderr — stderr is unbuffered, so logs flush immediately even
    // when output is redirected to a file. stdout is fully buffered when not a
    // tty, which causes logs to disappear on a hang.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();

    info!("Beyond starting");
    let config = BeyonderConfig::load_or_default();

    let event_loop = EventLoop::new()?;
    // WaitUntil is set per-frame in about_to_wait — not here.

    let mut handler = BeyonderHandler::new(config);
    event_loop.run_app(&mut handler)?;
    Ok(())
}

struct BeyonderHandler {
    config: BeyonderConfig,
    app: Option<App>,
    window: Option<Arc<Window>>,
    rt: tokio::runtime::Runtime,
}

impl BeyonderHandler {
    fn new(config: BeyonderConfig) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");
        Self { config, app: None, window: None, rt }
    }
}

impl ApplicationHandler for BeyonderHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }

        let window_attrs = WindowAttributes::default()
            .with_title("Beyond")
            .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 800u32))
            .with_resizable(true);

        let window = event_loop
            .create_window(window_attrs)
            .expect("Failed to create window");
        let window = Arc::new(window);

        let config = self.config.clone();
        let app = self
            .rt
            .block_on(App::new(Arc::clone(&window), config))
            .expect("Failed to init Beyond app");

        info!("Beyond initialized — window open");
        self.window = Some(Arc::clone(&window));
        self.app = Some(app);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = self.app.as_mut() else { return };

        let should_close = self.rt.block_on(app.handle_window_event(&event));
        if should_close || app.should_quit {
            event_loop.exit();
            return;
        }

        if matches!(event, WindowEvent::RedrawRequested) {
            if let Err(e) = app.render() {
                tracing::error!("Render error: {e}");
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Tick app state (drain supervisor/broker events) here, not inside
        // window_event(RedrawRequested), so it runs even when the window is
        // occluded or minimised (macOS suppresses RedrawRequested for hidden
        // windows, which would freeze streaming agent output).
        if let Some(app) = self.app.as_mut() {
            self.rt.block_on(app.tick());
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(8),
        ));
    }
}
