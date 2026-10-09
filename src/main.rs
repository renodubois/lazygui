mod theme;
mod views;

use gpui_kit::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use lazygui::{
    git::{MutationGates, process::ProcessHost},
    lazygit_config::{
        DiscoveryOptions,
        gui::{self, OrderedStorage},
    },
};
use std::{ffi::OsString, path::PathBuf, sync::Arc};

fn arguments(
    args: impl IntoIterator<Item = OsString>,
    cwd: PathBuf,
) -> Result<DiscoveryOptions, String> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let mut options = DiscoveryOptions::from_environment(cwd.clone());
    // Explicit global files are relative to invocation cwd, not --path or a
    // subsequently selected repository. Freeze before processing repository args.
    options.anchor_global_sources();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let bytes = arg.as_os_str().as_bytes();
        let (name, inline) = if let Some(pos) = bytes.iter().position(|b| *b == b'=') {
            (
                &bytes[..pos],
                Some(OsString::from_vec(bytes[pos + 1..].to_vec())),
            )
        } else {
            (bytes, None)
        };
        if !matches!(name, b"--path" | b"--use-config-file") {
            return Err("Usage: lazygui [--path PATH] [--use-config-file FILE[,FILE]]".into());
        }
        let value = inline
            .or_else(|| args.next())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Missing option value".to_string())?;
        if name == b"--path" {
            let path = PathBuf::from(value);
            options.cwd = if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            };
        } else {
            options.cli_config_file = Some(value);
        }
    }
    Ok(options)
}
fn main() {
    let result = (|| {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let options = arguments(std::env::args_os().skip(1), cwd)?;
        // One process host, mutation coordinator and ordered profile writer per process.
        let host = ProcessHost::new();
        let gates = Arc::new(MutationGates::default());
        let profile =
            gui::profile_directory(options.home.as_deref(), options.xdg_config_home.as_deref())
                .map_err(|e| e.to_string())?;
        let (storage, preferences, _trust) =
            OrderedStorage::open(profile).map_err(|e| e.to_string())?;
        gpui_kit::application()
            .with_assets(gpui_kit::assets::AllAssets)
            .run(move |cx: &mut App| {
                gpui_kit::init(cx);
                theme::init(cx);
                let dimensions = preferences.window_size.unwrap_or([1100, 760]);
                let bounds = Bounds::centered(
                    None,
                    size(px(dimensions[0] as f32), px(dimensions[1] as f32)),
                    cx,
                );
                gpui_kit::open_window(
                    WindowOptions {
                        titlebar: Some(TitlebarOptions {
                            title: Some("LazyGUI".into()),
                            ..Default::default()
                        }),
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    cx,
                    move |window, cx| {
                        let repository =
                            lazygui::repository::Repository::open(host.clone(), options);
                        views::app_shell::open(window, cx, host, gates, storage, repository)
                    },
                )
                .expect("open application window");
                cx.activate(true);
            });
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
#[cfg(test)]
#[path = "tests/startup.rs"]
mod tests;
