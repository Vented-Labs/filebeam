use std::{
    io,
    sync::{
        Arc, Mutex, Once, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    style::ResetColor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

static RAW: AtomicBool = AtomicBool::new(false);
static FULLSCREEN: AtomicBool = AtomicBool::new(false);
static CONTROLS: AtomicBool = AtomicBool::new(false);
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
        enable_raw_mode()?;
        RAW.store(true, Ordering::Relaxed);
        CONTROLS.store(controls, Ordering::Relaxed);
        let interrupted = Arc::new(AtomicBool::new(false));
        let guard = Self {
            _interrupt: watch_interrupt(interrupted.clone())?,
            interrupted,
        };
        if fullscreen {
            FULLSCREEN.store(true, Ordering::Relaxed);
            execute!(io::stdout(), EnterAlternateScreen)?;
        }
        if controls {
            execute!(io::stderr(), EnableBracketedPaste)?;
        }
        Ok(guard)
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
            let _ = execute!(io::stderr(), DisableBracketedPaste, ResetColor, Show);
        }
    }
    if FULLSCREEN.swap(false, Ordering::Relaxed) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen, ResetColor, Show);
    }
}
