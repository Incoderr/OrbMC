//! Windows notification area integration. All methods run on the window's UI thread.
use std::cell::Cell;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

const CALLBACK: u32 = WM_APP + 71;
const SUBCLASS_ID: usize = 0x4d43;

pub struct Tray {
    // Stable address retained by the native subclass until Drop removes it.
    state: Box<State>,
}

struct State {
    hwnd: HWND,
    icon: NOTIFYICONDATAW,
    visible: Cell<bool>,
    exit: Cell<bool>,
    english: Cell<bool>,
    taskbar_created: u32,
    /// Sent by a second launcher start to bring this one forward.
    activate: u32,
}

/// The icon embedded by build.rs (resource id 1), or the stock icon when it is missing.
fn application_icon() -> windows::core::Result<HICON> {
    unsafe {
        let module = GetModuleHandleW(None)?;
        match LoadIconW(Some(module.into()), PCWSTR(std::ptr::without_provenance(1))) {
            Ok(icon) => Ok(icon),
            Err(_) => LoadIconW(None, IDI_APPLICATION),
        }
    }
}

impl Tray {
    /// The handle must belong to a live window on the calling thread.
    pub unsafe fn new(handle: isize) -> windows::core::Result<Self> {
        unsafe {
            let hwnd = HWND(handle as *mut _);
            let mut icon = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: CALLBACK,
                hIcon: application_icon()?,
                ..Default::default()
            };
            for (slot, ch) in icon.szTip.iter_mut().zip("OrbMC".encode_utf16()) {
                *slot = ch;
            }
            let state = Box::new(State {
                hwnd,
                icon,
                visible: Cell::new(false),
                exit: Cell::new(false),
                english: Cell::new(false),
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
                activate: super::single_instance::activate_message(),
            });
            SetWindowSubclass(
                hwnd,
                Some(callback),
                SUBCLASS_ID,
                &*state as *const State as usize,
            )
            .ok()?;
            Ok(Self { state })
        }
    }

    pub fn exit_requested(&self) -> bool {
        self.state.exit.get()
    }

    pub fn hide(&self, english: bool) -> windows::core::Result<()> {
        self.state.english.set(english);
        unsafe {
            // Never hide a window until its recovery icon has been accepted by Explorer.
            if !self.state.visible.get() {
                Shell_NotifyIconW(NIM_ADD, &self.state.icon).ok()?;
                self.state.visible.set(true);
            }
            let _ = ShowWindow(self.state.hwnd, SW_HIDE);
        }
        Ok(())
    }
}

impl State {
    fn restore(&self) {
        unsafe {
            let command = if IsIconic(self.hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            };
            let _ = ShowWindow(self.hwnd, command);
            let _ = SetForegroundWindow(self.hwnd);
            if self.visible.replace(false) && !Shell_NotifyIconW(NIM_DELETE, &self.icon).as_bool() {
                tracing::warn!("failed to remove tray icon");
            }
        }
    }

    fn menu(&self) -> windows::core::Result<()> {
        unsafe {
            let menu = CreatePopupMenu()?;
            let result = (|| {
                AppendMenuW(
                    menu,
                    MF_STRING,
                    1,
                    if self.english.get() {
                        w!("Open launcher")
                    } else {
                        w!("Открыть лаунчер")
                    },
                )?;
                AppendMenuW(
                    menu,
                    MF_STRING,
                    2,
                    if self.english.get() {
                        w!("Exit")
                    } else {
                        w!("Выйти")
                    },
                )?;
                let mut point = POINT::default();
                GetCursorPos(&mut point)?;
                let _ = SetForegroundWindow(self.hwnd);
                let selected = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    None,
                    self.hwnd,
                    None,
                )
                .0;
                PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0))?;
                match selected {
                    1 => self.restore(),
                    2 => {
                        PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))?;
                        self.exit.set(true);
                    }
                    _ => {}
                }
                Ok(())
            })();
            DestroyMenu(menu)?;
            result
        }
    }
}

unsafe extern "system" fn callback(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        // Only shared references cross native calls: a popup menu can reenter this callback.
        let state = &*(data as *const State);
        if message == CALLBACK {
            match lparam.0 as u32 {
                WM_LBUTTONUP => state.restore(),
                WM_RBUTTONUP => {
                    if let Err(error) = state.menu() {
                        tracing::warn!(%error, "failed to show tray menu");
                        state.restore();
                    }
                }
                _ => {}
            }
            return LRESULT(0);
        }
        if message == state.activate {
            state.restore();
            return LRESULT(0);
        }
        if message == state.taskbar_created
            && state.visible.get()
            && !Shell_NotifyIconW(NIM_ADD, &state.icon).as_bool()
        {
            tracing::warn!("failed to restore tray icon after Explorer restart");
            state.restore();
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            if self.state.visible.get() {
                let _ = Shell_NotifyIconW(NIM_DELETE, &self.state.icon);
            }
            let _ = RemoveWindowSubclass(self.state.hwnd, Some(callback), SUBCLASS_ID);
        }
    }
}
