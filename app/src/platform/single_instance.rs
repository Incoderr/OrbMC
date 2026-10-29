//! Keeps one launcher per user session: a second start brings the running
//! launcher to the front (out of the tray too) instead of opening another.

/// Held by the running launcher; the next start sees it and steps aside.
pub struct InstanceGuard {
    #[cfg(target_os = "windows")]
    mutex: windows::Win32::Foundation::HANDLE,
}

/// Claims the launcher for this process. `None` means another launcher of
/// this user is already running.
pub fn acquire() -> Option<InstanceGuard> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
        use windows::Win32::System::Threading::CreateMutexW;
        use windows::core::w;

        let mutex = match unsafe { CreateMutexW(None, false, w!("Local\\OrbMC.SingleInstance")) } {
            Ok(mutex) => mutex,
            Err(error) => {
                // Better two launchers than none.
                tracing::warn!(%error, "single instance check unavailable");
                return Some(InstanceGuard {
                    mutex: Default::default(),
                });
            }
        };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            let _ = unsafe { CloseHandle(mutex) };
            return None;
        }
        Some(InstanceGuard { mutex })
    }
    #[cfg(not(target_os = "windows"))]
    Some(InstanceGuard {})
}

/// Asks the running launcher to show its window.
pub fn activate_running() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            ASFW_ANY, AllowSetForegroundWindow, HWND_BROADCAST, PostMessageW,
        };

        // This process was just started by the user, so it may hand the
        // foreground over to the launcher that is already running.
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        if let Err(error) = PostMessageW(
            Some(HWND_BROADCAST),
            activate_message(),
            WPARAM(0),
            LPARAM(0),
        ) {
            tracing::warn!(%error, "could not reach the running launcher");
        }
    }
}

/// Window message the running launcher answers by showing itself.
#[cfg(target_os = "windows")]
pub fn activate_message() -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;
    use windows::core::w;

    unsafe { RegisterWindowMessageW(w!("OrbMC.ActivateInstance")) }
}

#[cfg(target_os = "windows")]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if !self.mutex.is_invalid() {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.mutex) };
        }
    }
}
