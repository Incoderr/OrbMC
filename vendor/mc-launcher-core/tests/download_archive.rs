use std::{fs, io::Write};

use mc_launcher_core::{
    install::{
        assets::{
            asset_index_path, plan_asset_index_download, plan_asset_object_downloads_from_index,
            AssetIndexJson,
        },
        client::fetch_vanilla_version,
    },
    io::archive::extract_zip_safely,
    net::download::{execute_plan, should_skip_existing, Checksum, DownloadPlan, DownloadTask},
    progress::ProgressEvent,
};

#[test]
fn skips_existing_file_when_sha1_matches() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hello.txt");
    fs::write(&file, b"minecraft").unwrap();

    let task = DownloadTask {
        url: "https://example.invalid/hello.txt".to_string(),
        destination: file,
        checksum: Some(Checksum::Sha1(
            "624c22a8c8f8c93f18fe5ecd4713100c8d754507".to_string(),
        )),
        size: None,
        label: "hello".to_string(),
    };

    assert!(should_skip_existing(&task).unwrap());
}

#[test]
fn rejects_zip_entry_that_escapes_destination() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("bad.zip");
    let file = fs::File::create(&zip_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"bad").unwrap();
    writer.finish().unwrap();

    let err = extract_zip_safely(&zip_path, dir.path().join("out")).unwrap_err();
    assert!(err.to_string().contains("unsafe path"));
}

#[test]
#[ignore = "requires network"]
fn downloads_and_reuses_asset_objects_in_parallel() {
    let dir = tempfile::tempdir().unwrap();
    let version = fetch_vanilla_version("1.20.4").unwrap();
    let asset_index = version.asset_index.clone().unwrap();

    let index_plan = DownloadPlan {
        tasks: plan_asset_index_download(&version, dir.path()).unwrap(),
    };
    execute_plan(&index_plan, &mut |_event: ProgressEvent| {}).unwrap();

    let index_path = asset_index_path(dir.path(), &asset_index.id);
    let index: AssetIndexJson = serde_json::from_slice(&fs::read(index_path).unwrap()).unwrap();

    let mut object_plan = plan_asset_object_downloads_from_index(&index, dir.path());
    object_plan.tasks.truncate(16);
    let expected = object_plan.tasks.clone();

    let mut started = 0;
    let mut finished = 0;
    let mut reporter = |event: ProgressEvent| match event {
        ProgressEvent::TaskStarted { .. } => started += 1,
        ProgressEvent::TaskFinished { .. } => finished += 1,
        _ => {}
    };
    execute_plan(&object_plan, &mut reporter).unwrap();

    for task in &expected {
        assert!(
            task.destination.is_file(),
            "missing {}",
            task.destination.display()
        );
    }
    assert_eq!(started, 16);
    assert_eq!(finished, 16);

    // A second run must reuse the verified files without re-downloading.
    let mut skipped = 0;
    execute_plan(&object_plan, &mut |event: ProgressEvent| {
        if matches!(event, ProgressEvent::TaskSkipped { .. }) {
            skipped += 1;
        }
    })
    .unwrap();
    assert_eq!(skipped, 16);
}
