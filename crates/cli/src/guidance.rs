use std::{
    fmt,
    io::IsTerminal,
    sync::atomic::{AtomicBool, Ordering},
};

static STYLE: AtomicBool = AtomicBool::new(false);

/// Enables bold and colored error output once global flags are known.
pub fn set_style(config: &crate::config::Config, plain: bool) {
    STYLE.store(
        !plain
            && !config.no_color
            && std::env::var_os("NO_COLOR").is_none()
            && std::env::var("TERM").unwrap_or_default() != "dumb"
            && std::io::stderr().is_terminal(),
        Ordering::Relaxed,
    );
}

/// A user-facing failure that explains what to do next. `main` prints it
/// without the internal error chain.
#[derive(Debug, Default)]
pub struct Guidance {
    message: String,
    notes: Vec<String>,
    actions: Vec<(String, String)>,
}

impl Guidance {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Self::default()
        }
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Adds a command the user can run as-is. Labels read as a lead-in to the
    /// command, such as "To start it, run".
    pub fn action(mut self, label: impl Into<String>, command: impl Into<String>) -> Self {
        self.actions.push((label.into(), command.into()));
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn render(&self, style: bool) -> String {
        let (error, bold, reset) = if style {
            ("\x1b[1;31m", "\x1b[1m", "\x1b[0m")
        } else {
            ("", "", "")
        };
        let mut text = format!(
            "{error}error:{reset} {}",
            crate::presentation::clean(&self.message)
        );
        for note in &self.notes {
            text.push_str(&format!("\n  {}", crate::presentation::clean(note)));
        }
        for (label, command) in &self.actions {
            text.push_str(&format!(
                "\n\n  {label}:\n    {bold}{}{reset}",
                crate::presentation::clean(command)
            ));
        }
        text
    }
}

/// The one-line form used by the TUI: the message and its first command.
impl fmt::Display for Guidance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)?;
        if let Some((label, command)) = self.actions.first() {
            write!(formatter, " {label} `{command}`.")?;
        }
        Ok(())
    }
}

impl std::error::Error for Guidance {}

pub fn find(error: &anyhow::Error) -> Option<&Guidance> {
    error.downcast_ref::<Guidance>().or_else(|| {
        error
            .chain()
            .find_map(|cause| cause.downcast_ref::<Guidance>())
    })
}

/// Formats an internal error message as a sentence for a guidance note.
pub fn sentence(value: &str) -> String {
    let mut characters = value.trim().chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    let mut text = first.to_uppercase().chain(characters).collect::<String>();
    if !text.ends_with(['.', '!', '?', ')']) {
        text.push('.');
    }
    text
}

/// Prints a guidance error as-is and any other error in anyhow's default form.
pub fn report(error: &anyhow::Error) {
    match find(error) {
        Some(guidance) => eprintln!("{}", guidance.render(STYLE.load(Ordering::Relaxed))),
        None => eprintln!("Error: {error:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guidance_renders_notes_and_commands_without_terminal_controls() {
        let guidance = Guidance::new("The service isn't running.")
            .note("Device transfers run through it.")
            .action("To start it, run", "beam service enable");
        assert_eq!(
            guidance.render(false),
            "error: The service isn't running.\n  Device transfers run through it.\n\n  To start it, run:\n    beam service enable"
        );
        assert!(
            !Guidance::new("named \x1b]52;c;bad\x07device")
                .action("Run", "beam to \x1b[2Jx")
                .render(false)
                .contains('\x1b')
        );
        assert_eq!(
            guidance.to_string(),
            "The service isn't running. To start it, run `beam service enable`."
        );
        let wrapped = anyhow::Error::new(guidance).context("outer");
        assert_eq!(
            find(&wrapped).map(Guidance::message),
            Some("The service isn't running.")
        );
    }
}
