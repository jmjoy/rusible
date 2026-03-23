use super::file;
use crate::Error;
use minijinja::{Environment, UndefinedBehavior};
use rusible_meta::{TaskDetails, TaskResult, TaskStatus, TemplateDetails, TemplateTask};
use std::{fs, fs::OpenOptions};

pub(crate) fn execute(task: &TemplateTask, context: &toml::Table) -> Result<TaskResult, Error> {
    let mut changes = file::FileChangeSummary::default();
    let rendered = render_template(&task.content, context)?;

    if let Some(parent) = task.dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    if !task.dest.exists() {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&task.dest)?;
        changes.created = true;
    }

    let current = fs::read_to_string(&task.dest).ok();
    if current.as_deref() != Some(rendered.as_str()) {
        fs::write(&task.dest, &rendered)?;
        changes.content_changed = true;
    }

    changes.mode_changed = file::apply_mode(&task.dest, task.mode.as_deref())?;
    changes.ownership_changed =
        file::apply_owner_group(&task.dest, task.owner.as_deref(), task.group.as_deref())?;

    let status = if changes.any() {
        TaskStatus::Changed
    } else {
        TaskStatus::Ok
    };
    let message = if changes.any() {
        format!("template {} updated", task.dest.display())
    } else {
        format!("template {} already in desired state", task.dest.display())
    };

    Ok(task_result(
        status,
        message,
        changes.into_template_details(&task.dest),
    ))
}

fn render_template(content: &str, context: &toml::Table) -> Result<String, Error> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    let template = environment.template_from_str(content)?;
    Ok(template.render(context)?)
}

fn task_result(
    status: TaskStatus,
    message: impl Into<String>,
    details: TemplateDetails,
) -> TaskResult {
    TaskResult {
        status,
        message: Some(message.into()),
        details: Some(TaskDetails::Template(details)),
    }
}
