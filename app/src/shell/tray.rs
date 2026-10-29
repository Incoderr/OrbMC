use crate::app::LauncherApp;
use gpui_kit::{App, WeakEntity, Window};

pub fn install(window: &mut Window, view: WeakEntity<LauncherApp>, cx: &App) -> bool {
    #[cfg(target_os = "windows")]
    let tray = {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let result = HasWindowHandle::window_handle(window)
            .map_err(|error| error.to_string())
            .and_then(|handle| match handle.as_raw() {
                RawWindowHandle::Win32(handle) => {
                    // GPUI owns this window; the close callback retains the tray on its UI thread.
                    unsafe { crate::platform::tray::Tray::new(handle.hwnd.get()) }
                        .map_err(|error| error.to_string())
                }
                _ => Err("unsupported native window handle".to_owned()),
            });
        match result {
            Ok(tray) => Some(tray),
            Err(error) => {
                tracing::warn!(%error, "tray integration unavailable");
                None
            }
        }
    };
    #[cfg(target_os = "windows")]
    let available = tray.is_some();
    #[cfg(not(target_os = "windows"))]
    let available = false;

    window.on_window_should_close(cx, move |_window, cx| {
        #[cfg(target_os = "windows")]
        if let Some(tray) = &tray
            && !tray.exit_requested()
            && let Some(view) = view.upgrade()
            && view.read(cx).hide_to_tray(cx)
        {
            let english = view.read(cx).tray_uses_english(cx);
            if let Err(error) = tray.hide(english) {
                use gpui_kit::component::WindowExt;
                tracing::error!(%error, "could not hide launcher in tray; keeping window open");
                _window.push_notification(
                    if english {
                        "Could not create the tray icon. The window remains open."
                    } else {
                        "Не удалось создать значок трея. Окно осталось открытым."
                    },
                    cx,
                );
            }
            return false;
        }
        #[cfg(not(target_os = "windows"))]
        let _ = &view;
        cx.quit();
        true
    });
    available
}
