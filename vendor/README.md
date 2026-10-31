# GPUI inspector compatibility patch

`gpui-pre/` is the unchanged crates.io distribution of `gpui-pre` 0.3.7 except
for the inspector fixes described below. Its original licenses and source
metadata are retained. The workspace uses it through `[patch.crates-io]`.

Changes:

- `src/window.rs`: keep pointer events in the inspector panel available while
  picking application elements. Share the panel width calculation with layout.
- `src/elements/div.rs`: omit inspector-only hitboxes for transparent,
  noninteractive containers covering the application viewport. This prevents
  empty GPUI Component notification/tooltip hosts from masking real elements.

Regression coverage lives in `app/tests/inspector.rs` and exercises the public
GPUI mouse event and inspector renderer APIs.

These changes do not alter normal application hitboxes or input handling when
the inspector is closed. Remove the patch when the upstream GPUI version used
by GPUI Kit contains equivalent fixes.

## Minecraft installation core

`mc-launcher-core/` vendors the MIT-licensed `mc-launcher-core` 0.1.2 crate.
The local patch avoids compiling the crate's unused `sysinfo` Windows feature
set: architecture checks use Rust's target constant, while `sysinfo` remains a
non-Windows-only dependency for OS version detection.

- `src/utils/helper.rs` (`download_file`): Java runtime files from Mojang are
  compressed as legacy `.lzma` (LZMA-alone), which the plain `.xz` decoder
  rejected with "stream/file format not recognized", so no Java runtime could
  be downloaded. The file is now read with liblzma's auto decoder, which
  accepts both formats.
