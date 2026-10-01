/// Presentation-neutral notification request. Platform integration must only construct this
/// after the shared configuration permits desktop notifications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

pub fn permitted(enabled: bool, notification: Notification) -> Option<Notification> {
    enabled.then_some(notification)
}

/// A real system-notification adapter. Requests run on a dedicated worker so D-Bus/OS delivery
/// never blocks GPUI. Completion content is deliberately generic and never includes filenames or keys.
pub struct NotificationService {
    enabled: bool,
    sender: std::sync::mpsc::Sender<(Notification, std::sync::mpsc::Sender<Result<(), String>>)>,
}

impl NotificationService {
    pub fn new(enabled: bool) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<(
            Notification,
            std::sync::mpsc::Sender<Result<(), String>>,
        )>();
        std::thread::spawn(move || {
            while let Ok((notification, acknowledgement)) = receiver.recv() {
                let result = notify_rust::Notification::new()
                    .appname("Filebeam")
                    .summary(&notification.title)
                    .body(&notification.body)
                    .show()
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                let _ = acknowledgement.send(result);
            }
        });
        Self { enabled, sender }
    }

    pub fn transfer_completed(&self) -> Option<std::sync::mpsc::Receiver<Result<(), String>>> {
        let notification = permitted(
            self.enabled,
            Notification {
                title: "Filebeam".into(),
                body: "Transfer completed".into(),
            },
        )?;
        let (sender, acknowledgement) = std::sync::mpsc::channel();
        self.sender.send((notification, sender)).ok()?;
        Some(acknowledgement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_gates_notifications() {
        let note = Notification {
            title: "Filebeam".into(),
            body: "Transfer complete".into(),
        };
        assert!(permitted(false, note.clone()).is_none());
        assert_eq!(permitted(true, note.clone()), Some(note));
    }
}
