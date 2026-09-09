mod state;
mod views;

use std::{
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{config::Config, presentation::Theme, protocol, terminal::Session};
use state::State;

pub fn run(config: &Config, instance: &str) -> Result<()> {
    let mut state = State::new(config, instance, std::env::current_dir()?)?;
    let theme = Theme::new(config);
    let results = {
        let session = Session::enter(true)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        let (sender, info) = mpsc::channel();
        let instance = instance.to_owned();
        thread::spawn(move || {
            let _ =
                sender.send(protocol::instance_info(&instance).map_err(|error| error.to_string()));
        });
        loop {
            if session
                .interrupted
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                if let Some(job) = &state.job {
                    job.control.cancel();
                } else {
                    break;
                }
            }
            if let Ok(info) = info.try_recv() {
                state.info = Some(info);
            }
            state.receive(theme);
            terminal.draw(|frame| {
                let cursor = views::render(frame.area(), frame.buffer_mut(), &mut state, theme);
                if let Some(cursor) = cursor {
                    frame.set_cursor_position(cursor);
                }
            })?;
            if let Some(link) = &state.hyperlink {
                crate::output::screen_link(&mut io::stdout(), link, theme)?;
            }
            if event::poll(Duration::from_millis(if theme.motion { 70 } else { 200 }))? {
                match event::read()? {
                    Event::Key(key) if key.kind != KeyEventKind::Release => {
                        if state.key(key)? {
                            break;
                        }
                    }
                    Event::Paste(value) => state.paste(&value),
                    Event::Resize(_, _) => terminal.clear()?,
                    _ => {}
                }
            }
            if state
                .notice
                .as_ref()
                .is_some_and(|(_, time)| time.elapsed() > Duration::from_secs(5))
            {
                state.notice = None;
            }
            state.now = Instant::now();
        }
        state.results
    };
    for value in results {
        crate::output::result(&value, false)?;
    }
    Ok(())
}
