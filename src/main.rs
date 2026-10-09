mod catalog;
mod connectors;
mod runtime;
mod storage;
#[cfg(test)]
mod test_support;
mod theme;
mod views;

use gpui_kit::*;

fn main() {
    // Load small local preferences before starting the GUI event loop.
    let path = storage::config_path().expect("set XDG_CONFIG_HOME or HOME");
    let (config, warning) = match storage::load(&path) {
        Ok(config) => (config, None),
        Err(error) => (storage::Config::default(), Some(error.to_string())),
    };
    let persistence = storage::Persistence::new(path);
    // Network is opt-in; invalid configuration stops startup instead of silently falling back.
    let client = match std::env::var("CATALOG_URL") {
        Ok(url) => connectors::catalog::Client::http(&url).expect("invalid CATALOG_URL"),
        Err(std::env::VarError::NotPresent) => connectors::catalog::Client::memory(),
        Err(error) => panic!("invalid CATALOG_URL: {error}"),
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            theme::init(cx);
            let bounds = Bounds::centered(None, size(px(900.), px(640.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(env!("CARGO_PKG_NAME").into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    let execution =
                        runtime::Execution::production(cx.background_executor().clone());
                    views::app_shell::open(
                        window,
                        cx,
                        client,
                        execution,
                        config,
                        Some(persistence),
                        warning,
                    )
                },
            )
            .expect("open application window");
            cx.activate(true);
        });
}
