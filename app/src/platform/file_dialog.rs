//! File picker limited to some extensions. GPUI's own picker has no file
//! type filter, so Windows gets the system dialog with one; elsewhere the
//! caller falls back to GPUI's picker and checks the extension itself.

use std::future::Future;
use std::path::{Path, PathBuf};

/// Files the picker offers: a label and extensions without the dot.
#[derive(Clone, Copy, Debug)]
pub struct FileFilter {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
}

impl FileFilter {
    /// Whether `path` has one of the filter's extensions, in any case.
    pub fn matches(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                self.extensions
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(extension))
            })
    }
}

/// Opens a filtered picker for one file. `None` means this OS has no
/// filtered picker here; the future yields `None` when the user cancels.
pub fn pick_file(
    title: &str,
    filter: FileFilter,
) -> Option<impl Future<Output = Option<PathBuf>> + use<>> {
    imp::pick_file(title, filter)
}

#[cfg(target_os = "windows")]
mod imp {
    use std::future::Future;
    use std::path::PathBuf;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
    };
    use windows::core::{HSTRING, PCWSTR};

    use super::FileFilter;

    pub(super) fn pick_file(
        title: &str,
        filter: FileFilter,
    ) -> Option<impl Future<Output = Option<PathBuf>> + use<>> {
        let title = title.to_owned();
        let (sender, receiver) = futures::channel::oneshot::channel();
        // The dialog is modal and blocks: it gets its own COM thread.
        let spawned = std::thread::Builder::new()
            .name("file-dialog".into())
            .spawn(move || {
                let picked = show(&title, filter).unwrap_or_else(|error| {
                    tracing::warn!(%error, "file dialog failed");
                    None
                });
                let _ = sender.send(picked);
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "failed to start the file dialog thread");
            return None;
        }
        Some(async move { receiver.await.ok().flatten() })
    }

    /// `Ok(None)` when the user closes the dialog without a file.
    fn show(title: &str, filter: FileFilter) -> windows::core::Result<Option<PathBuf>> {
        // SAFETY: plain COM calls on a thread that initializes COM for itself
        // and uninitializes it before returning; every string handed to the
        // dialog outlives the calls that use it.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
            let result = (|| {
                let dialog: IFileOpenDialog =
                    CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
                let name = HSTRING::from(filter.name);
                let spec = HSTRING::from(
                    filter
                        .extensions
                        .iter()
                        .map(|extension| format!("*.{extension}"))
                        .collect::<Vec<_>>()
                        .join(";"),
                );
                dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                    pszName: PCWSTR(name.as_ptr()),
                    pszSpec: PCWSTR(spec.as_ptr()),
                }])?;
                dialog.SetTitle(&HSTRING::from(title))?;
                dialog
                    .SetOptions(dialog.GetOptions()? | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)?;
                if dialog.Show(None).is_err() {
                    // Cancelled (or closed): nothing picked.
                    return Ok(None);
                }
                let path = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
                let text = path.to_string();
                CoTaskMemFree(Some(path.0 as *const _));
                Ok(text.ok().map(PathBuf::from))
            })();
            CoUninitialize();
            result
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use std::path::PathBuf;

    use super::FileFilter;

    pub(super) fn pick_file(
        _title: &str,
        _filter: FileFilter,
    ) -> Option<std::future::Ready<Option<PathBuf>>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::FileFilter;
    use std::path::Path;

    #[test]
    fn filter_matches_extensions_in_any_case() {
        let filter = FileFilter {
            name: "Images",
            extensions: &["png", "jpg"],
        };
        assert!(filter.matches(Path::new("C:/icons/Cover.PNG")));
        assert!(filter.matches(Path::new("cover.jpg")));
        assert!(!filter.matches(Path::new("cover.gif")));
        assert!(!filter.matches(Path::new("png")));
    }
}
