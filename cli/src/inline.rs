use std::{
    io::{self, IsTerminal, Write},
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use crossterm::{
    cursor::{Hide, MoveToColumn, MoveUp, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    queue,
    style::{Attribute, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType, size},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    low_level::unregister,
};
use unicode_width::UnicodeWidthStr;
use zeroize::Zeroizing;

use crate::{
    app::{Cancelled, Direction, Job, Phase, Prompt, PromptKind, Request, TransferView},
    config::Config,
    input::Input,
    presentation::{self as paint, Theme, bytes, clean, clip, duration},
    terminal::Session,
};

struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            unregister(id);
        }
    }
}

pub fn run(config: &Config, instance: &str, request: Request, plain: bool) -> Result<Vec<String>> {
    let theme = Theme::new(config);
    let label = match &request {
        Request::Upload(files, _) if files.len() == 1 => files[0]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        Request::Upload(files, _) => format!("{} files", files.len()),
        Request::Download { .. } => "Encrypted transfer".into(),
        Request::Update => "beam".into(),
    };
    let mut view = TransferView::new(request.direction());
    let job = Job::start(instance.to_owned(), config.clone(), request);
    let mut signals = Signals(Vec::new());
    for signal in [SIGINT, SIGTERM] {
        signals.0.push(signal_hook::flag::register(
            signal,
            job.control.cancelled.clone(),
        )?);
    }
    let interactive =
        !plain && io::stderr().is_terminal() && std::env::var("TERM").unwrap_or_default() != "dumb";
    let mut surface = InlineSurface::default();
    let mut seen = Vec::new();
    loop {
        view.tick(job.control.snapshot(), !theme.motion, Instant::now());
        view.cancelling = job.control.cancelled.load(Ordering::Relaxed);
        if let Some(outcome) = job.poll() {
            view.tick(job.control.snapshot(), true, Instant::now());
            view.finish(outcome.is_ok());
            surface.clear()?;
            let values = outcome?;
            let target = if view.progress.files > 1 {
                format!("{} files", view.progress.files)
            } else if view.progress.name.is_empty() {
                label.clone()
            } else {
                view.progress.name.clone()
            };
            let summary = format!(
                "{} {} · {} · {}",
                view.direction.completed(),
                clean(&target),
                bytes(view.progress.total.unwrap_or(0)),
                duration(view.elapsed)
            );
            if interactive {
                let mut stderr = io::stderr();
                queue!(
                    stderr,
                    SetForegroundColor(theme.success().into()),
                    Print(format!("  {} ", theme.symbol("✓", "+"))),
                    ResetColor,
                    Print(format!("{}\n", summary))
                )?;
                stderr.flush()?;
            } else {
                eprintln!("{summary}");
            }
            return Ok(values);
        }
        if let Ok(prompt) = job.prompts.try_recv() {
            surface.clear()?;
            if !io::stdin().is_terminal() {
                job.control.cancel();
                bail!(
                    "{} required; run beam down in an interactive terminal",
                    prompt.kind.label()
                );
            }
            if matches!(prompt.kind, PromptKind::Directory { .. }) {
                prompt_directory(&prompt, &job, theme)?;
            } else {
                prompt_secret(&prompt, &job, plain)?;
            }
            continue;
        }
        if interactive {
            let width = size()
                .map(|size| size.0.saturating_sub(1))
                .unwrap_or(79)
                .max(1);
            let mut buffer = Buffer::empty(Rect::new(0, 0, width, 2));
            render(&mut buffer, theme, &view, &label);
            surface.draw(&buffer)?;
        } else if !seen.contains(&view.progress.phase) {
            seen.push(view.progress.phase);
            if !matches!(
                view.progress.phase,
                Phase::Encrypting | Phase::Verifying | Phase::Unlocking
            ) {
                eprintln!("{} · {}", view.phase_label(), clean(&label));
            }
        }
        thread::sleep(Duration::from_millis(if theme.motion { 70 } else { 200 }));
    }
}

pub fn render(buffer: &mut Buffer, theme: Theme, view: &TransferView, fallback: &str) {
    let area = buffer.area;
    let name = if view.progress.name.is_empty() {
        fallback
    } else {
        &view.progress.name
    };
    let direction = match view.direction {
        Direction::Upload => theme.symbol("↑", "up"),
        Direction::Download => theme.symbol("↓", "down"),
        Direction::Update => "",
    };
    let prefix = format!("  filebeam  {direction} ");
    let context = if view.progress.files > 1 {
        format!(
            "{} · {} of {}",
            name, view.progress.index, view.progress.files
        )
    } else {
        name.to_owned()
    };
    let heading = format!("{} {}", view.phase_label(), context);
    paint::line(
        paint::at(area, 0, 1),
        buffer,
        Line::from(vec![
            Span::styled(prefix.clone(), theme.strong().fg(theme.accent())),
            Span::styled(
                clip(&heading, area.width.saturating_sub(prefix.width() as u16)),
                theme.strong(),
            ),
        ]),
    );
    let measured = view.progress.total.is_some()
        && !matches!(
            view.progress.phase,
            Phase::Preparing
                | Phase::Archiving
                | Phase::Connecting
                | Phase::Unlocking
                | Phase::Updating
        );
    let percent = if measured {
        format!("{:>2}%", (view.ratio * 100.0).floor() as u64)
    } else {
        paint::spinner(theme, view.elapsed).into()
    };
    let mut stats = percent;
    if area.width >= 52
        && let Some(total) = view.progress.total
    {
        stats.push_str(&format!("  {}/{}", bytes(view.progress.done), bytes(total)));
    }
    if area.width >= 76 && view.rate >= 1.0 && !view.stalled() {
        stats.push_str(&format!("  {}/s", bytes(view.rate as u64)));
    }
    if area.width >= 90
        && let Some(eta) = view.eta()
    {
        stats.push_str(&format!("  eta {}", duration(eta)));
    }
    let width = area.width.saturating_sub(stats.width() as u16 + 5).min(48);
    paint::meter(
        Rect::new(2.min(area.width), 1, width, 1).intersection(area),
        buffer,
        theme,
        measured.then_some(view.ratio),
        view.elapsed,
    );
    paint::line(
        Rect::new(
            (width + 4).min(area.width),
            1,
            area.width.saturating_sub(width + 4),
            1,
        )
        .intersection(area),
        buffer,
        Line::styled(stats, theme.dim()),
    );
}

/// Relative repainting keeps the shell's scrollback and works without a cursor-position query.
#[derive(Default)]
struct InlineSurface {
    visible: bool,
}

impl InlineSurface {
    fn draw(&mut self, buffer: &Buffer) -> io::Result<()> {
        let mut output = io::stderr();
        if self.visible {
            queue!(output, MoveUp(1))?;
        }
        queue!(output, Hide)?;
        for y in 0..2 {
            queue!(
                output,
                MoveToColumn(0),
                ResetColor,
                SetAttribute(Attribute::Reset),
                Clear(ClearType::CurrentLine)
            )?;
            let mut x = 0;
            let mut previous = None;
            while x < buffer.area.width {
                let cell = &buffer[(x, y)];
                let style = (cell.fg, cell.bg, cell.modifier);
                if previous != Some(style) {
                    queue!(
                        output,
                        SetForegroundColor(cell.fg.into()),
                        SetBackgroundColor(cell.bg.into()),
                        SetAttribute(if cell.modifier.contains(Modifier::BOLD) {
                            Attribute::Bold
                        } else {
                            Attribute::NormalIntensity
                        })
                    )?;
                    previous = Some(style);
                }
                queue!(output, Print(cell.symbol()))?;
                x += cell.symbol().width().max(1) as u16;
            }
            queue!(output, ResetColor, SetAttribute(Attribute::Reset))?;
            if y == 0 {
                queue!(output, Print("\r\n"))?;
            }
        }
        self.visible = true;
        output.flush()
    }

    fn clear(&mut self) -> io::Result<()> {
        if self.visible {
            let mut output = io::stderr();
            queue!(
                output,
                ResetColor,
                SetAttribute(Attribute::Reset),
                MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                MoveUp(1),
                Clear(ClearType::CurrentLine),
                Show
            )?;
            self.visible = false;
            output.flush()?;
        }
        Ok(())
    }
}
impl Drop for InlineSurface {
    fn drop(&mut self) {
        let _ = self.clear();
    }
}

fn prompt_secret(prompt: &Prompt, job: &Job, plain: bool) -> Result<()> {
    let _session = if plain {
        Session::plain_input()?
    } else {
        Session::enter(false)?
    };
    let mut input = Input::default();
    if plain {
        eprint!("  {}: ", prompt.kind.label());
        io::stderr().flush()?;
    }
    loop {
        if job.control.cancelled.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        let mut output = io::stderr();
        let width = size().map(|s| s.0).unwrap_or(80);
        let prefix = format!("  {}: ", prompt.kind.label());
        let (masked, _) = input.visible(width.saturating_sub(prefix.width() as u16 + 1), true);
        if !plain {
            queue!(
                output,
                MoveToColumn(0),
                Clear(ClearType::CurrentLine),
                Print(&prefix),
                Print(masked),
                Show
            )?;
        }
        output.flush()?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Enter => {
                        let _ = prompt.reply.send(Zeroizing::new(input.take()));
                        queue!(output, Print("\r\n"))?;
                        output.flush()?;
                        return Ok(());
                    }
                    KeyCode::Esc => {
                        job.control.cancel();
                        return Err(Cancelled.into());
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        job.control.cancel();
                        return Err(Cancelled.into());
                    }
                    _ => input.handle(key),
                },
                Event::Paste(value) => input.insert(value.trim()),
                _ => {}
            }
        }
    }
}

fn prompt_directory(prompt: &Prompt, job: &Job, theme: Theme) -> Result<()> {
    let PromptKind::Directory {
        files,
        bytes: total,
        maximum_files,
    } = prompt.kind
    else {
        return Ok(());
    };
    let _session = Session::enter(false)?;
    let mut output = io::stderr();
    queue!(
        output,
        Print(format!(
            "  Directory upload · {files} files · {}\r\n  Choose a mode, then press Enter\r\n",
            bytes(total)
        ))
    )?;
    output.flush()?;
    let mut surface = InlineSurface::default();
    let mut choice = 0;
    loop {
        job.control.check()?;
        let width = size()
            .map(|size| size.0.saturating_sub(1))
            .unwrap_or(79)
            .max(1);
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, 2));
        let individual = if let Some(maximum) = maximum_files {
            format!(
                "Individual files   {files}/{maximum} files{}",
                if files > maximum {
                    " · over limit"
                } else {
                    ""
                }
            )
        } else {
            format!("Individual files   {files} files")
        };
        for (index, label) in ["ZIP and send       1 file".to_owned(), individual]
            .into_iter()
            .enumerate()
        {
            let style = if index == choice {
                theme.strong().fg(theme.accent()).bg(theme.selected())
            } else {
                theme.dim()
            };
            paint::line(
                Rect::new(0, index as u16, width, 1),
                &mut buffer,
                Line::styled(
                    clip(
                        &format!(
                            "  {} {}  {label}",
                            if index == choice {
                                theme.symbol("›", ">")
                            } else {
                                " "
                            },
                            index + 1
                        ),
                        width,
                    ),
                    style,
                ),
            );
        }
        surface.draw(&buffer)?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Char('1') | KeyCode::Char('z') | KeyCode::Up | KeyCode::Left => choice = 0,
                KeyCode::Char('2') | KeyCode::Char('i') | KeyCode::Down | KeyCode::Right => {
                    choice = 1
                }
                KeyCode::Tab => choice = 1 - choice,
                KeyCode::Enter => {
                    let _ = prompt.reply.send(Zeroizing::new(
                        if choice == 0 { "zip" } else { "individual" }.to_owned(),
                    ));
                    surface.clear()?;
                    return Ok(());
                }
                KeyCode::Esc => {
                    job.control.cancel();
                    return Err(Cancelled.into());
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    job.control.cancel();
                    return Err(Cancelled.into());
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_layout_handles_narrow_and_wide_terminals() {
        for width in [1, 20, 40, 80, 120] {
            let mut buffer = Buffer::empty(Rect::new(0, 0, width, 2));
            render(
                &mut buffer,
                Theme::new(&Config::default()),
                &TransferView::new(Direction::Upload),
                "日本語-long-filename.zip",
            );
            assert_eq!(buffer.area.width, width);
        }
    }
}
