/// Native menu and close handlers share this state: closing a window never stops jobs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    HideWindow,
    RestoreWindow,
    CheckpointAndQuit,
    Exit,
}

#[derive(Default)]
pub struct Lifecycle {
    hidden: bool,
    active_jobs: bool,
}

impl Lifecycle {
    pub fn set_active_jobs(&mut self, active: bool) {
        self.active_jobs = active;
    }
    pub fn close_window(&mut self) -> Action {
        self.hidden = true;
        Action::HideWindow
    }
    pub fn restore(&mut self) -> Action {
        self.hidden = false;
        Action::RestoreWindow
    }
    /// Explicit Quit is the sole process-stop path. The platform must perform the returned
    /// checkpoint before terminating so staging is only activated on a later full launch.
    pub fn quit(&self) -> Action {
        if self.active_jobs {
            Action::CheckpointAndQuit
        } else {
            Action::Exit
        }
    }
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_keeps_service_alive_and_quit_is_explicit() {
        let mut lifecycle = Lifecycle::default();
        lifecycle.set_active_jobs(true);
        assert_eq!(lifecycle.close_window(), Action::HideWindow);
        assert!(lifecycle.is_hidden());
        assert_eq!(lifecycle.restore(), Action::RestoreWindow);
        assert_eq!(lifecycle.quit(), Action::CheckpointAndQuit);
    }
}
