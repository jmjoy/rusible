use super::{TaskDataSpec, TaskDetails, TaskSpec, TaskValidationError, resolve_optional};
use crate::field::Field;
use serde::{Deserialize, Serialize};
use toml::Table;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FactsTask {
    pub name: Field<String>,
}

impl TaskSpec for FactsTask {
    type Data = FactsTaskData;
    type Details = FactsDetails;

    fn resolve(self, context: &Table) -> Result<Self::Data, TaskValidationError> {
        Ok(FactsTaskData {
            name: resolve_optional("facts", "name", self.name, context)?,
        })
    }

    fn try_from_details(details: TaskDetails) -> Option<Self::Details> {
        if let TaskDetails::Facts(details) = details {
            Some(details)
        } else {
            None
        }
    }

    fn expected_task_kind() -> &'static str {
        "facts"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactsTaskData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl TaskDataSpec for FactsTaskData {
    fn validate(&self) -> Result<(), TaskValidationError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactsDetails {
    pub hostname: String,
}
