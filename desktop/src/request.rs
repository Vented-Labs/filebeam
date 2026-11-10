//! Following one command from dispatch to its outcome.
//!
//! The service reports a finished command in one of two append-only logs: a
//! completion, or a message carrying the failure. A request remembers where both
//! stood when it was sent, so an older outcome is never mistaken for its own.

use crate::{
    client::DesktopClient,
    model::{ClientCommand, DesktopSnapshot},
};

/// Where the service's logs stood at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    operation: u64,
    message: u64,
}

impl Mark {
    pub fn now(snapshot: &DesktopSnapshot) -> Self {
        Self {
            operation: snapshot.operations.last().map_or(0, |entry| entry.id),
            message: snapshot.messages.last().map_or(0, |entry| entry.id),
        }
    }

    /// Whether `operation` has completed since this mark.
    pub fn completed(&self, snapshot: &DesktopSnapshot, operation: &str) -> bool {
        snapshot
            .operations
            .iter()
            .any(|entry| entry.id > self.operation && entry.operation == operation)
    }

    /// The failure reported for `operation` since this mark.
    pub fn failure(&self, snapshot: &DesktopSnapshot, operation: &str) -> Option<String> {
        snapshot
            .messages
            .iter()
            .find(|entry| entry.id > self.message && entry.operation == operation)
            .map(|entry| entry.error.detail.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Waiting,
    Completed,
    Failed(String),
}

/// A command the service has not answered yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    operation: &'static str,
    mark: Mark,
}

impl Request {
    /// Queues `command` and remembers where its outcome will appear.
    pub fn send(client: &DesktopClient, command: ClientCommand) -> anyhow::Result<Self> {
        let request = Self {
            operation: command.operation(),
            mark: Mark::now(&client.snapshot()),
        };
        client.dispatch(command)?;
        Ok(request)
    }

    pub fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn outcome(&self, snapshot: &DesktopSnapshot) -> Outcome {
        if let Some(detail) = self.mark.failure(snapshot, self.operation) {
            Outcome::Failed(detail)
        } else if self.mark.completed(snapshot, self.operation) {
            Outcome::Completed
        } else {
            Outcome::Waiting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClientError, ClientErrorCode, ClientMessage, OperationStatus};

    fn completed(id: u64, operation: &str) -> OperationStatus {
        OperationStatus {
            id,
            operation: operation.into(),
        }
    }

    fn failed(id: u64, operation: &str) -> ClientMessage {
        ClientMessage {
            id,
            operation: operation.into(),
            error: ClientError {
                code: ClientErrorCode::Remote,
                detail: "refused".into(),
            },
        }
    }

    #[test]
    fn only_outcomes_after_the_request_count() {
        let mut snapshot = DesktopSnapshot::default();
        snapshot.operations.push(completed(4, "login"));
        snapshot.messages.push(failed(9, "login"));
        let (client, commands) = DesktopClient::test_client(snapshot.clone());
        let request = Request::send(&client, ClientCommand::Logout).unwrap();
        assert!(matches!(commands.try_recv(), Ok(ClientCommand::Logout)));
        assert_eq!(request.operation(), "logout");
        assert_eq!(request.outcome(&snapshot), Outcome::Waiting);

        // Another command finishing, or an older entry, says nothing about this one.
        snapshot.operations.push(completed(5, "login"));
        assert_eq!(request.outcome(&snapshot), Outcome::Waiting);
        snapshot.operations.push(completed(6, "logout"));
        assert_eq!(request.outcome(&snapshot), Outcome::Completed);

        // A failure wins: the service reports one or the other, never both.
        snapshot.messages.push(failed(10, "logout"));
        assert_eq!(
            request.outcome(&snapshot),
            Outcome::Failed("refused".into())
        );
    }

    #[test]
    fn a_mark_survives_the_log_being_trimmed() {
        let mut snapshot = DesktopSnapshot::default();
        snapshot.operations.push(completed(7, "note"));
        let mark = Mark::now(&snapshot);
        snapshot.operations.remove(0);
        assert!(!mark.completed(&snapshot, "note"));
        snapshot.operations.push(completed(8, "note"));
        assert!(mark.completed(&snapshot, "note"));
    }
}
