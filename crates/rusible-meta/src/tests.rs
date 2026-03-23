use super::*;
use std::path::PathBuf;
use toml::Table;

#[test]
fn task_round_trips_as_json() {
    let task = Task::File(FileTask {
        path: PathBuf::from("/tmp/example"),
        state: FileState::File,
        owner: Some("root".to_string()),
        group: None,
        mode: Some("0644".to_string()),
        content: Some("hello".to_string()),
    });

    let json = serde_json::to_string(&task).unwrap();
    let decoded: Task = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, task);
}

#[test]
fn task_request_round_trips_as_json() {
    let mut context = Table::new();
    context.insert(
        "region".to_string(),
        toml::Value::String("cn-north-1".to_string()),
    );

    let request = TaskRequest::new(
        Task::Template(TemplateTask {
            dest: PathBuf::from("/tmp/example"),
            content: "hello {{ region }}".to_string(),
            owner: None,
            group: None,
            mode: None,
        }),
        context,
    );

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
fn file_task_converts_into_task() {
    let task: Task = FileTask {
        path: PathBuf::from("/tmp/example"),
        state: FileState::Touch,
        owner: None,
        group: None,
        mode: None,
        content: None,
    }
    .into();

    assert!(matches!(task, Task::File(_)));
}

#[test]
fn template_task_converts_into_task() {
    let task: Task = TemplateTask {
        dest: PathBuf::from("/tmp/example"),
        content: "hello".to_string(),
        owner: None,
        group: None,
        mode: None,
    }
    .into();

    assert!(matches!(task, Task::Template(_)));
}

#[test]
fn command_task_converts_into_task() {
    let task: Task = CommandTask {
        cmd: Some("echo hello".to_string()),
        argv: None,
        chdir: Some(PathBuf::from("/tmp")),
        creates: None,
        removes: None,
        stdin: None,
    }
    .into();

    assert!(matches!(task, Task::Command(_)));
}

#[test]
fn copy_task_converts_into_task() {
    let task: Task = CopyTask {
        src: PathBuf::from("/tmp/src"),
        dest: PathBuf::from("/tmp/dest"),
        owner: None,
        group: None,
        mode: Some("0755".to_string()),
    }
    .into();

    assert!(matches!(task, Task::Copy(_)));
}

#[test]
fn download_task_converts_into_task() {
    let task: Task = DownloadTask {
        url: "https://example.com/archive.tar.gz".to_string(),
        dest: PathBuf::from("/tmp/archive.tar.gz"),
        force: false,
        owner: None,
        group: None,
        mode: Some("0644".to_string()),
    }
    .into();

    assert!(matches!(task, Task::Download(_)));
}

#[test]
fn shell_task_converts_into_task() {
    let task: Task = ShellTask {
        cmd: "echo hello | cat".to_string(),
        chdir: Some(PathBuf::from("/tmp")),
        creates: None,
        removes: None,
        stdin: None,
    }
    .into();

    assert!(matches!(task, Task::Shell(_)));
}

#[test]
fn stat_task_converts_into_task() {
    let task: Task = StatTask {
        path: PathBuf::from("/tmp/example"),
    }
    .into();

    assert!(matches!(task, Task::Stat(_)));
}

#[test]
fn systemd_task_converts_into_task() {
    let task: Task = SystemdTask {
        unit: "etcd.service".to_string(),
        daemon_reload: true,
        enabled: Some(true),
        state: Some(SystemdState::Started),
    }
    .into();

    assert!(matches!(task, Task::Systemd(_)));
}

#[test]
fn unarchive_task_converts_into_task() {
    let task: Task = UnarchiveTask {
        src: PathBuf::from("/tmp/archive.tar.gz"),
        dest: PathBuf::from("/tmp"),
        creates: Some(PathBuf::from("/tmp/bin")),
    }
    .into();

    assert!(matches!(task, Task::Unarchive(_)));
}

#[test]
fn user_task_converts_into_task() {
    let task: Task = UserTask {
        name: "etcd".to_string(),
        system: true,
        create_home: false,
        shell: Some(PathBuf::from("/usr/sbin/nologin")),
        home: None,
    }
    .into();

    assert!(matches!(task, Task::User(_)));
}

#[test]
fn wait_for_task_converts_into_task() {
    let task: Task = WaitForTask {
        port: 2379,
        host: Some("127.0.0.1".to_string()),
        delay_secs: 1,
        timeout_secs: 30,
        connect_timeout_secs: 5,
    }
    .into();

    assert!(matches!(task, Task::WaitFor(_)));
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
fn command_task_spec_extracts_command_details() {
    let details = CommandTask::try_from_details(TaskDetails::Command(CommandDetails {
        cmd: vec!["echo".to_string(), "hello".to_string()],
        chdir: Some(PathBuf::from("/tmp")),
        rc: Some(0),
        stdout: "hello\n".to_string(),
        stderr: String::new(),
    }));

    assert!(matches!(
        details,
        Some(CommandDetails {
            rc: Some(0),
            ..
        })
    ));
}

#[test]
fn command_result_with_details_round_trips_as_json() {
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
fn download_result_with_details_round_trips_as_json() {
    let result = TaskResult::changed("downloaded file").with_details(TaskDetails::Download(
        DownloadDetails {
            url: "https://example.com/archive.tar.gz".to_string(),
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
fn stat_result_with_details_round_trips_as_json() {
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
fn copy_result_with_details_round_trips_as_json() {
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
