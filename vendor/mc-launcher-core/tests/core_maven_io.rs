use std::{fs, path::Path};

use mc_launcher_core::{
    core::maven::MavenCoordinate,
    io::{hash::sha1_file, paths::safe_join},
};

/// Renders a path with forward slashes so the expected values below are
/// platform-independent (`artifact_path` uses `PathBuf::push`).
fn slash_form(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn parses_maven_coordinate_with_classifier() {
    let coordinate = MavenCoordinate::parse("org.lwjgl:lwjgl:3.3.2:natives-macos-arm64").unwrap();
    assert_eq!(coordinate.group, "org.lwjgl");
    assert_eq!(coordinate.artifact, "lwjgl");
    assert_eq!(coordinate.version, "3.3.2");
    assert_eq!(
        coordinate.classifier.as_deref(),
        Some("natives-macos-arm64")
    );
    assert_eq!(coordinate.extension, "jar");
    assert_eq!(
        slash_form(&coordinate.artifact_path()),
        "org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2-natives-macos-arm64.jar"
    );
}

#[test]
fn parses_maven_coordinate_with_extension() {
    let coordinate = MavenCoordinate::parse("com.example:demo:1.0@zip").unwrap();
    assert_eq!(coordinate.extension, "zip");
    assert_eq!(
        slash_form(&coordinate.artifact_path()),
        "com/example/demo/1.0/demo-1.0.zip"
    );
}

#[test]
fn rejects_invalid_maven_coordinate() {
    let err = MavenCoordinate::parse("bad").unwrap_err();
    assert!(err.to_string().contains("invalid maven coordinate"));
}

#[test]
fn computes_sha1_for_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sample.txt");
    fs::write(&file, b"minecraft").unwrap();

    assert_eq!(
        sha1_file(&file).unwrap(),
        "624c22a8c8f8c93f18fe5ecd4713100c8d754507"
    );
}

#[test]
fn safe_join_rejects_parent_escape() {
    let dir = tempfile::tempdir().unwrap();
    let err = safe_join(dir.path(), "../escape.jar").unwrap_err();
    assert!(err.to_string().contains("unsafe path"));
}
