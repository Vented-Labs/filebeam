use std::{
    io,
    sync::{
        Arc, Mutex, Once, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use crossterm::{
    cursor::Show,
    event::{
        DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    style::ResetColor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

static RAW: AtomicBool = AtomicBool::new(false);
static FULLSCREEN: AtomicBool = AtomicBool::new(false);
static CONTROLS: AtomicBool = AtomicBool::new(false);
static BRACKETED_PASTE: AtomicBool = AtomicBool::new(false);
static ENHANCED_KEYBOARD: AtomicBool = AtomicBool::new(false);
static HOOK: Once = Once::new();
static INTERRUPT_FLAGS: OnceLock<Mutex<Vec<Weak<AtomicBool>>>> = OnceLock::new();
static INTERRUPT_HANDLER: OnceLock<Result<(), String>> = OnceLock::new();

pub struct Session {
    pub interrupted: Arc<AtomicBool>,
    _interrupt: InterruptGuard,
}

pub struct InterruptGuard(Arc<AtomicBool>);

pub fn watch_interrupt(flag: Arc<AtomicBool>) -> io::Result<InterruptGuard> {
    let handler = INTERRUPT_HANDLER.get_or_init(|| {
        ctrlc::set_handler(|| {
            let flags = INTERRUPT_FLAGS.get_or_init(|| Mutex::new(Vec::new()));
            if let Ok(mut flags) = flags.lock() {
                flags.retain(|flag| {
                    if let Some(flag) = flag.upgrade() {
                        flag.store(true, Ordering::Relaxed);
                        true
                    } else {
                        false
                    }
                });
            }
        })
        .map_err(|error| error.to_string())
    });
    if let Err(error) = handler {
        return Err(io::Error::other(error.clone()));
    }
    INTERRUPT_FLAGS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map_err(|error| io::Error::other(error.to_string()))?
        .push(Arc::downgrade(&flag));
    Ok(InterruptGuard(flag))
}

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        if let Some(flags) = INTERRUPT_FLAGS.get()
            && let Ok(mut flags) = flags.lock()
        {
            flags.retain(|flag| !flag.ptr_eq(&Arc::downgrade(&self.0)) && flag.strong_count() > 0);
        }
    }
}

impl Session {
    pub fn enter(fullscreen: bool) -> io::Result<Self> {
        Self::setup(fullscreen, true)
    }

    pub fn plain_input() -> io::Result<Self> {
        Self::setup(false, false)
    }

    fn setup(fullscreen: bool, controls: bool) -> io::Result<Self> {
        HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore();
                previous(info);
            }));
        });
        let interrupted = Arc::new(AtomicBool::new(false));
        let guard = Self {
            _interrupt: watch_interrupt(interrupted.clone())?,
            interrupted,
        };
        enable_raw_mode()?;
        RAW.store(true, Ordering::Relaxed);
        CONTROLS.store(controls, Ordering::Relaxed);
        if fullscreen {
            FULLSCREEN.store(true, Ordering::Relaxed);
            execute!(io::stdout(), EnterAlternateScreen)?;
        }
        if controls {
            enable_optional(&BRACKETED_PASTE, || {
                execute!(io::stderr(), EnableBracketedPaste)
            })?;
            enable_optional(&ENHANCED_KEYBOARD, || {
                execute!(
                    io::stderr(),
                    PushKeyboardEnhancementFlags(
                        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES,
                    )
                )
            })?;
        }
        Ok(guard)
    }
}

fn enable_optional(
    enabled: &AtomicBool,
    enable: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    match enable() {
        Ok(()) => {
            enabled.store(true, Ordering::Relaxed);
            Ok(())
        }
        // Legacy Windows consoles lack these protocols; ordinary key events still work.
        Err(error) if error.kind() == io::ErrorKind::Unsupported => Ok(()),
        Err(error) => Err(error),
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        restore();
    }
}

fn restore() {
    if RAW.swap(false, Ordering::Relaxed) {
        let _ = disable_raw_mode();
        if CONTROLS.swap(false, Ordering::Relaxed) {
            if ENHANCED_KEYBOARD.swap(false, Ordering::Relaxed) {
                let _ = execute!(io::stderr(), PopKeyboardEnhancementFlags);
            }
            if BRACKETED_PASTE.swap(false, Ordering::Relaxed) {
                let _ = execute!(io::stderr(), DisableBracketedPaste);
            }
            let _ = execute!(io::stderr(), ResetColor, Show);
        }
    }
    if FULLSCREEN.swap(false, Ordering::Relaxed) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen, ResetColor, Show);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_controls_track_success_and_allow_unsupported_protocols() {
        let paste = AtomicBool::new(false);
        let keyboard = AtomicBool::new(false);
        enable_optional(&paste, || Ok(())).unwrap();
        enable_optional(&keyboard, || Err(io::ErrorKind::Unsupported.into())).unwrap();
        assert!(paste.load(Ordering::Relaxed));
        assert!(!keyboard.load(Ordering::Relaxed));
    }

    #[test]
    fn optional_controls_preserve_io_failures() {
        let enabled = AtomicBool::new(false);
        let error =
            enable_optional(&enabled, || Err(io::ErrorKind::BrokenPipe.into())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(!enabled.load(Ordering::Relaxed));
    }

    #[cfg(windows)]
    #[test]
    fn legacy_windows_bracketed_paste_does_not_abort_startup() {
        use crossterm::Command;

        let enabled = AtomicBool::new(false);
        enable_optional(&enabled, || EnableBracketedPaste.execute_winapi()).unwrap();
        assert!(!enabled.load(Ordering::Relaxed));
    }

    #[cfg(windows)]
    #[test]
    fn windows_keyboard_enhancement_does_not_abort_startup() {
        let enabled = AtomicBool::new(false);
        enable_optional(&enabled, || {
            execute!(
                io::sink(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )
        })
        .unwrap();
        assert!(!enabled.load(Ordering::Relaxed));
    }
}
