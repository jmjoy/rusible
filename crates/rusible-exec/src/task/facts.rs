use crate::Error;
use rusible_meta::task::{
    TaskDetails, TaskResult,
    facts::{FactsDetails, FactsTaskData},
};
use std::io::{Error as IoError, ErrorKind};

pub(crate) async fn execute(_task: &FactsTaskData) -> Result<TaskResult, Error> {
    let hostname = hostname::get()?.to_string_lossy().trim().to_string();
    if hostname.is_empty() {
        return Err(IoError::new(ErrorKind::InvalidData, "hostname is empty").into());
    }

    Ok(TaskResult::ok("collected host facts")
        .with_details(TaskDetails::Facts(FactsDetails { hostname })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusible_meta::task::TaskDetails;

    #[tokio::test]
    async fn collects_non_empty_hostname() {
        let result = execute(&FactsTaskData { name: None }).await.unwrap();

        match result.details {
            Some(TaskDetails::Facts(details)) => assert!(!details.hostname.is_empty()),
            other => panic!("unexpected facts details: {other:?}"),
        }
    }
}
