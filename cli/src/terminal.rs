use std::{
    io,
    sync::{
        Arc, Once,
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

pub struct Session {
    pub interrupted: Arc<AtomicBool>,
    signals: Vec<signal_hook::SigId>,
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
        let mut guard = Self {
            interrupted: Arc::new(AtomicBool::new(false)),
            signals: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            guard.signals.push(signal_hook::flag::register(
                signal,
                guard.interrupted.clone(),
            )?);
        }
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
        for signal in self.signals.drain(..) {
            signal_hook::low_level::unregister(signal);
        }
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
