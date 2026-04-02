use super::*;
use std::path::PathBuf;
use toml::Table;
use url::Url;

#[test]
fn task_resolves_into_task_data() {
    let task = Task::File(FileTask {
        name: "ensure example file".into(),
        path: PathBuf::from("/tmp/example").into(),
        state: FileState::File.into(),
        owner: "root".into(),
        group: Field::Nil,
        mode: "0644".into(),
        content: "hello".into(),
    });

    let resolved = task.resolve(&Table::new()).unwrap();

    assert_eq!(
        resolved,
        TaskData::File(FileTaskData {
            name: Some("ensure example file".to_string()),
            path: PathBuf::from("/tmp/example"),
            state: FileState::File,
            owner: Some("root".to_string()),
            group: None,
            mode: Some("0644".to_string()),
            content: Some("hello".to_string()),
        })
    );
}

#[test]
fn task_resolves_templates_before_transport() {
    let context = toml::toml! {
        region = "cn-north-1"
    };

    let resolved = Task::File(FileTask {
        name: "render example template".into(),
        path: PathBuf::from("/tmp/example").into(),
        state: FileState::File.into(),
        content: Field::tpl("hello {{ region }}"),
        owner: Field::Nil,
        group: Field::Nil,
        mode: Field::Nil,
    })
    .resolve(&context)
    .unwrap();

    assert_eq!(
        resolved,
        TaskData::File(FileTaskData {
            name: Some("render example template".to_string()),
            path: PathBuf::from("/tmp/example"),
            state: FileState::File,
            content: Some("hello cn-north-1".to_string()),
            owner: None,
            group: None,
            mode: None,
        })
    );
}

#[test]
fn task_data_round_trips_as_json() {
    let task = TaskData::File(FileTaskData {
        name: Some("ensure example file".to_string()),
        path: PathBuf::from("/tmp/example"),
        state: FileState::File,
        owner: Some("root".to_string()),
        group: None,
        mode: Some("0644".to_string()),
        content: Some("hello".to_string()),
    });

    let json = serde_json::to_string(&task).unwrap();
    let decoded: TaskData = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, task);
}

#[test]
fn task_request_round_trips_as_json() {
    let request = TaskRequest::new(FileTaskData {
        name: Some("render example template".to_string()),
        path: PathBuf::from("/tmp/example"),
        state: FileState::File,
        content: Some("hello cn-north-1".to_string()),
        owner: None,
        group: None,
        mode: None,
    });

    let json = serde_json::to_string(&request).unwrap();
    let decoded: TaskRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, request);
}

#[test]
fn task_result_with_details_round_trips_as_json() {
    let result = TaskResult::changed("updated").with_details(TaskDetails::File(FileDetails {
        path: PathBuf::from("/tmp/example"),
        state: FileState::File,
        created: true,
        removed: false,
        content_changed: true,
        mode_changed: false,
        ownership_changed: false,
    }));

    let json = serde_json::to_string(&result).unwrap();
    let decoded: TaskResult = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, result);
}

#[test]
fn field_resolves_templated_pathbuf() {
    let context = toml::toml! {
        app = { dir = "/tmp" }
    };

    let resolved = Field::<PathBuf>::tpl("{{ app.dir }}/example")
        .resolve(&context)
        .unwrap();

    assert_eq!(resolved, Some(PathBuf::from("/tmp/example")));
}

#[test]
fn file_task_spec_extracts_file_details() {
    let details = FileTask::try_from_details(TaskDetails::File(FileDetails {
        path: PathBuf::from("/tmp/example"),
        state: FileState::Touch,
        created: false,
        removed: false,
        content_changed: false,
        mode_changed: true,
        ownership_changed: false,
    }));

    assert!(matches!(
        details,
        Some(FileDetails {
            mode_changed: true,
            ..
        })
    ));
}

#[test]
fn task_result_with_command_details_round_trips_as_json() {
    let result = TaskResult::changed("command executed").with_details(TaskDetails::Command(
        CommandDetails {
            cmd: vec!["echo".to_string(), "hello".to_string()],
            chdir: Some(PathBuf::from("/tmp")),
            rc: Some(0),
            stdout: "hello\n".to_string(),
            stderr: String::new(),
        },
    ));

    let json = serde_json::to_string(&result).unwrap();
    let decoded: TaskResult = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, result);
}

#[test]
fn task_result_with_download_details_round_trips_as_json() {
    let result = TaskResult::changed("downloaded file").with_details(TaskDetails::Download(
        DownloadDetails {
            url: Url::parse("https://example.com/archive.tar.gz").unwrap(),
            dest: PathBuf::from("/tmp/archive.tar.gz"),
            downloaded: true,
            bytes_written: 42,
            mode_changed: false,
            ownership_changed: false,
        },
    ));

    let json = serde_json::to_string(&result).unwrap();
    let decoded: TaskResult = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, result);
}

#[test]
fn task_result_with_stat_details_round_trips_as_json() {
    let result = TaskResult::ok("path inspected").with_details(TaskDetails::Stat(StatDetails {
        path: PathBuf::from("/tmp/example"),
        exists: true,
        is_file: true,
        is_dir: false,
        is_symlink: false,
        mode: Some("0644".to_string()),
    }));

    let json = serde_json::to_string(&result).unwrap();
    let decoded: TaskResult = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, result);
}

#[test]
fn task_result_with_copy_details_round_trips_as_json() {
    let result = TaskResult::changed("copied file").with_details(TaskDetails::Copy(CopyDetails {
        src: PathBuf::from("/tmp/src"),
        dest: PathBuf::from("/tmp/dest"),
        created: true,
        content_changed: true,
        mode_changed: false,
        ownership_changed: false,
    }));

    let json = serde_json::to_string(&result).unwrap();
    let decoded: TaskResult = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, result);
}

#[test]
fn task_data_validate_rejects_invalid_wait_for() {
    let error = WaitForTaskData {
        name: None,
        host: None,
        port: 0,
        delay_secs: 0,
        timeout_secs: 0,
        connect_timeout_secs: 0,
    }
    .validate()
    .unwrap_err();

    assert!(matches!(error, TaskValidationError::InvalidField { .. }));
}
