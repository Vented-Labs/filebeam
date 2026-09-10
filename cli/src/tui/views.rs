use ratatui::{
    buffer::Buffer,
    layout::{Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Sparkline, Widget, Wrap},
};

use super::state::{Entry, Focus, Mode, State};
use crate::{
    app::{Direction, Phase},
    input::Input,
    presentation::{self as paint, Theme, at, bytes, clean, clip, duration},
};

pub fn render(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut State,
    theme: Theme,
) -> Option<(u16, u16)> {
    state.hyperlink = None;
    Block::default().style(theme.base()).render(area, buffer);
    if area.width < 40 || area.height < 14 {
        Paragraph::new("Filebeam\n\nEnlarge your terminal to at least 40 × 14.\nCtrl+C to exit.")
            .style(theme.strong())
            .wrap(Wrap { trim: true })
            .render(area, buffer);
        return None;
    }
    let compact = area.height < 20;
    let outer = area.inner(Margin::new(
        if area.width >= 90 { 3 } else { 1 },
        if compact { 0 } else { 1 },
    ));
    let spacious = area.height >= 32;
    let header_height = if spacious {
        7
    } else if compact {
        1
    } else {
        3
    };
    header(at(outer, 0, header_height), buffer, state, theme, spacious);
    let tabs = at(outer, header_height, if compact { 1 } else { 2 });
    navigation(tabs, buffer, state, theme);
    let footer = Rect::new(outer.x, outer.bottom().saturating_sub(1), outer.width, 1);
    let body = Rect::new(
        outer.x,
        tabs.bottom(),
        outer.width,
        footer.y.saturating_sub(tabs.bottom() + 1),
    );
    let mut cursor = if state.transfer.is_some() {
        transfer(body, buffer, state, theme);
        None
    } else if state.mode == Mode::Send {
        send(body, buffer, state, theme)
    } else if state.mode == Mode::Receive {
        receive(body, buffer, state, theme)
    } else {
        transfers(body, buffer, state, theme)
    };
    footer_view(footer, buffer, state, theme);
    if state.help {
        state.hyperlink = None;
        dim(area, buffer, theme);
        help(area, buffer, theme);
        cursor = None;
    }
    if state.prompt.is_some() {
        state.hyperlink = None;
        dim(area, buffer, theme);
        cursor = secret(area, buffer, state, theme);
    }
    cursor
}

fn header(area: Rect, buffer: &mut Buffer, state: &State, theme: Theme, spacious: bool) {
    paint::ambient(
        area,
        buffer,
        theme,
        state.now.duration_since(state.launched),
    );
    let text_x = if spacious { area.x + 16 } else { area.x + 2 };
    if spacious {
        paint::mark(Rect::new(area.x + 1, area.y, 12, 7), buffer, theme);
    }
    let text = Rect::new(
        text_x,
        area.y + u16::from(spacious),
        area.right().saturating_sub(text_x),
        1,
    );
    paint::line(
        text,
        buffer,
        Line::from(vec![Span::styled(
            if spacious {
                "Filebeam"
            } else {
                theme.symbol("◆ Filebeam", "Filebeam")
            },
            theme.strong(),
        )]),
    );
    if spacious {
        paint::line(
            at(
                Rect::new(
                    text_x,
                    area.y,
                    area.right().saturating_sub(text_x),
                    area.height,
                ),
                3,
                1,
            ),
            buffer,
            Line::styled(
                "Good things are worth sharing.",
                theme.strong().fg(theme.accent()),
            ),
        );
        paint::line(
            at(
                Rect::new(
                    text_x,
                    area.y,
                    area.right().saturating_sub(text_x),
                    area.height,
                ),
                4,
                1,
            ),
            buffer,
            Line::styled("Encrypted on your device.", theme.dim()),
        );
    }
    let host = state
        .instance
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let status = match &state.info {
        Some(Ok(_)) => theme.success(),
        Some(Err(_)) => theme.danger(),
        None => theme.muted(),
    };
    let local =
        host.starts_with("localhost") || host.starts_with("127.") || host.starts_with("[::1]");
    let name = state
        .info
        .as_ref()
        .and_then(|info| info.as_ref().ok())
        .map(|info| info.name.as_str())
        .unwrap_or("INSTANCE");
    let label = format!("{} {}", if local { "LOCAL" } else { name }, host);
    let width = (host.len() as u16 + 13).min(area.width / 2);
    let badge = Rect::new(
        area.right().saturating_sub(width),
        area.y
            + if spacious {
                1
            } else if area.height > 1 {
                2
            } else {
                0
            },
        width,
        1,
    );
    paint::line(
        badge,
        buffer,
        Line::from(vec![
            Span::styled(
                format!("{} ", theme.symbol("●", "*")),
                Style::default().fg(status),
            ),
            Span::styled(clip(&label, width.saturating_sub(2)), theme.dim()),
        ]),
    );
}

fn navigation(area: Rect, buffer: &mut Buffer, state: &State, theme: Theme) {
    let mut spans = Vec::new();
    for (mode, label) in [
        (Mode::Send, " 1  Send files "),
        (Mode::Receive, " 2  Receive "),
        (Mode::Transfers, " 3  Transfers "),
    ] {
        let style = if state.mode == mode {
            theme.strong().fg(theme.accent()).bg(theme.selected())
        } else {
            theme.dim()
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw("  "));
    }
    paint::line(at(area, 0, 1), buffer, Line::from(spans));
}

fn send(area: Rect, buffer: &mut Buffer, state: &mut State, theme: Theme) -> Option<(u16, u16)> {
    if area.width >= 100 {
        let left_width = area.width * 63 / 100;
        let left = Rect::new(area.x, area.y, left_width, area.height);
        let right = Rect::new(
            left.right() + 2,
            area.y,
            area.width.saturating_sub(left_width + 2),
            area.height,
        );
        let cursor = browser(left, buffer, state, theme, false);
        queue(right, buffer, state, theme);
        cursor
    } else {
        let tray_height = if area.height > 10 { 3 } else { 2 };
        let browser_area = Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(tray_height + 1),
        );
        let cursor = browser(
            browser_area,
            buffer,
            state,
            theme,
            state.focus == Focus::Queue,
        );
        let tray = Rect::new(area.x, browser_area.bottom() + 1, area.width, tray_height);
        Block::default()
            .style(Style::default().bg(theme.raised()))
            .render(tray, buffer);
        let action_width = 26.min(tray.width / 2);
        paint::line(
            Rect::new(
                tray.x + 2,
                tray.y + u16::from(tray.height > 2),
                tray.width.saturating_sub(action_width + 3),
                1,
            ),
            buffer,
            Line::styled(
                format!(
                    "{} selected · {}",
                    state.selected.len(),
                    bytes(state.total())
                ),
                theme.strong(),
            ),
        );
        paint::button(
            Rect::new(
                tray.right() - action_width,
                tray.y,
                action_width,
                tray.height,
            ),
            buffer,
            theme,
            "Send encrypted  ↵",
            state.focus == Focus::Action,
        );
        cursor
    }
}

fn browser(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut State,
    theme: Theme,
    selected_only: bool,
) -> Option<(u16, u16)> {
    let inner = paint::card(
        area,
        buffer,
        theme,
        state.focus == Focus::Browser || selected_only,
    );
    if inner.is_empty() {
        return None;
    }
    paint::line(
        at(inner, 0, 1),
        buffer,
        Line::styled(
            if selected_only {
                "Your transfer"
            } else {
                "Choose what to share"
            },
            theme.strong(),
        ),
    );
    let mut cursor = None;
    let compact = inner.height < 10;
    let rows_start = if compact { 3 } else { 5 };
    if !selected_only {
        if !compact {
            paint::line(
                at(inner, 1, 1),
                buffer,
                Line::styled(
                    clip(&state.directory.display().to_string(), inner.width),
                    theme.dim(),
                ),
            );
        }
        let search = at(inner, if compact { 1 } else { 3 }, 1);
        let (value, column) = state.filter.visible(search.width.saturating_sub(3), false);
        let label = if state.filter.value.is_empty() && !state.searching {
            "/  Search this folder".to_owned()
        } else {
            format!("/  {value}")
        };
        paint::line(
            search,
            buffer,
            Line::styled(
                label,
                if state.searching {
                    theme.strong().bg(theme.selected())
                } else {
                    theme.dim()
                },
            ),
        );
        if state.searching && search.height > 0 {
            cursor = Some((
                search.x + (3 + column).min(search.width.saturating_sub(1)),
                search.y,
            ));
        }
    } else {
        paint::line(
            at(inner, 1, 1),
            buffer,
            Line::styled("Space removes a file from this transfer", theme.dim()),
        );
    }
    let row_area = at(inner, rows_start, inner.height.saturating_sub(rows_start));
    let double = row_area.height >= 12 && row_area.width >= 55;
    let row_height = if double { 2 } else { 1 };
    let capacity = (row_area.height / row_height) as usize;
    if selected_only {
        ensure_scroll(&mut state.queue_scroll, state.queue_cursor, capacity);
        for (offset, (_, entry)) in state
            .selected
            .iter()
            .skip(state.queue_scroll)
            .take(capacity)
            .enumerate()
        {
            file_row(
                at(row_area, offset as u16 * row_height, row_height),
                buffer,
                entry,
                true,
                state.queue_scroll + offset == state.queue_cursor,
                theme,
            );
        }
    } else {
        ensure_scroll(&mut state.scroll, state.cursor, capacity);
        for (offset, index) in state
            .filtered
            .iter()
            .skip(state.scroll)
            .take(capacity)
            .enumerate()
        {
            let entry = &state.entries[*index];
            file_row(
                at(row_area, offset as u16 * row_height, row_height),
                buffer,
                entry,
                state.selected.contains_key(&entry.path),
                state.scroll + offset == state.cursor && state.focus == Focus::Browser,
                theme,
            );
        }
    }
    if capacity > 0
        && if selected_only {
            state.selected.is_empty()
        } else {
            state.filtered.is_empty()
        }
    {
        paint::line(
            at(row_area, 0, 1),
            buffer,
            Line::styled(
                if selected_only {
                    "Your next transfer starts with a file."
                } else {
                    "No files match. Try another search or folder."
                },
                theme.dim(),
            ),
        );
    }
    cursor
}

fn file_row(
    area: Rect,
    buffer: &mut Buffer,
    entry: &Entry,
    selected: bool,
    focused: bool,
    theme: Theme,
) {
    let bg = if focused {
        theme.selected()
    } else {
        theme.surface()
    };
    Block::default()
        .style(Style::default().bg(bg))
        .render(area, buffer);
    let indicator = if selected {
        theme.symbol("✓", "x")
    } else if focused {
        theme.symbol("›", ">")
    } else {
        " "
    };
    let name_width = area.width.saturating_sub(20);
    let size = if entry.directory {
        theme.symbol("→", ">").to_owned()
    } else {
        bytes(entry.size)
    };
    let spans = vec![
        Span::styled(
            format!("{indicator} "),
            Style::default().fg(if selected {
                theme.success()
            } else {
                theme.accent()
            }),
        ),
        Span::styled(
            format!(" {:^4} ", entry.kind()),
            Style::default().fg(theme.accent()).bg(theme.raised()),
        ),
        Span::styled(
            format!("  {}", clip(&entry.name, name_width)),
            if focused {
                theme.strong()
            } else {
                Style::default().fg(theme.text())
            },
        ),
    ];
    paint::line(at(area, 0, 1), buffer, Line::from(spans));
    if area.width >= 26 {
        Paragraph::new(size)
            .right_aligned()
            .style(theme.dim())
            .render(Rect::new(area.right() - 10, area.y, 10, 1), buffer);
    }
}

fn ensure_scroll(scroll: &mut usize, cursor: usize, capacity: usize) {
    if cursor < *scroll {
        *scroll = cursor;
    } else if capacity > 0 && cursor >= *scroll + capacity {
        *scroll = cursor + 1 - capacity;
    }
}

fn queue(area: Rect, buffer: &mut Buffer, state: &mut State, theme: Theme) {
    let inner = paint::card(
        area,
        buffer,
        theme,
        matches!(state.focus, Focus::Queue | Focus::Action),
    );
    if inner.height < 5 {
        return;
    }
    paint::line(
        at(inner, 0, 1),
        buffer,
        Line::styled("Your transfer", theme.strong()),
    );
    paint::line(
        at(inner, 1, 1),
        buffer,
        Line::styled(
            format!("{} files · {}", state.selected.len(), bytes(state.total())),
            theme.dim(),
        ),
    );
    let queue_area = at(inner, 3, inner.height.saturating_sub(10));
    if state.selected.is_empty() && queue_area.height >= 4 {
        paint::line(
            at(queue_area, 1, 1),
            buffer,
            Line::styled("A little privacy.", theme.strong().fg(theme.accent())),
        );
        paint::line(
            at(queue_area, 2, 1),
            buffer,
            Line::styled("A lot of possibilities.", theme.strong()),
        );
        paint::line(
            at(queue_area, 4, 1),
            buffer,
            Line::styled("Space to add a file", theme.dim()),
        );
    } else {
        let capacity = queue_area.height as usize / 2;
        ensure_scroll(&mut state.queue_scroll, state.queue_cursor, capacity);
        for (index, entry) in state
            .selected
            .values()
            .skip(state.queue_scroll)
            .take(capacity)
            .enumerate()
        {
            let row = at(queue_area, index as u16 * 2, 2);
            let focused =
                state.focus == Focus::Queue && state.queue_scroll + index == state.queue_cursor;
            let label = format!(
                "{} {}",
                theme.symbol("✓", "+"),
                clip(&entry.name, row.width.saturating_sub(3))
            );
            paint::line(
                at(row, 0, 1),
                buffer,
                Line::styled(
                    label,
                    if focused {
                        theme.strong().bg(theme.selected())
                    } else {
                        theme.strong()
                    },
                ),
            );
            paint::line(
                at(row, 1, 1),
                buffer,
                Line::styled(format!("  {}", bytes(entry.size)), theme.dim()),
            );
        }
    }
    if let Some(Ok(info)) = &state.info {
        let limits = format!(
            "{} · {}h retention",
            info.maximum_transfer_bytes
                .map(bytes)
                .unwrap_or_else(|| "Unlimited size".into()),
            info.file_retention_hours
        );
        paint::line(
            at(inner, inner.height.saturating_sub(6), 1),
            buffer,
            Line::styled(limits, theme.dim()),
        );
    }
    paint::button(
        at(inner, inner.height.saturating_sub(4), 3),
        buffer,
        theme,
        "Send encrypted  ↵",
        state.focus == Focus::Action,
    );
    paint::line(
        at(inner, inner.height.saturating_sub(1), 1),
        buffer,
        Line::styled("Only your link can unlock it.", theme.dim()),
    );
}

fn field(
    area: Rect,
    buffer: &mut Buffer,
    input: &Input,
    theme: Theme,
    focused: bool,
    placeholder: &str,
    masked: bool,
) -> Option<(u16, u16)> {
    Block::default()
        .style(Style::default().bg(if focused {
            theme.selected()
        } else {
            theme.bg()
        }))
        .render(area, buffer);
    let inset = area.inner(Margin::new(1, 0));
    let row = at(inset, if inset.height >= 3 { 1 } else { 0 }, 1);
    let (value, column) = input.visible(row.width.saturating_sub(1), masked);
    paint::line(
        row,
        buffer,
        Line::styled(
            if value.is_empty() {
                clip(placeholder, row.width)
            } else {
                value
            },
            if focused { theme.strong() } else { theme.dim() },
        ),
    );
    if focused && !row.is_empty() {
        Some((row.x + column.min(row.width.saturating_sub(1)), row.y))
    } else {
        None
    }
}

fn receive(area: Rect, buffer: &mut Buffer, state: &State, theme: Theme) -> Option<(u16, u16)> {
    let panel = centered(area, 90, area.height);
    let inner = paint::card(panel, buffer, theme, false);
    if inner.height < 7 {
        return None;
    }
    let compact = inner.height < 17;
    let field_height = if compact { 1 } else { 3 };
    paint::line(
        at(inner, 0, 1),
        buffer,
        Line::styled("A link is all you need.", theme.strong().fg(theme.accent())),
    );
    if !compact {
        paint::line(
            at(inner, 1, 1),
            buffer,
            Line::styled(
                "Paste a Filebeam link. We will decrypt and verify it on this device.",
                theme.dim(),
            ),
        );
    }
    let link_row = if compact { 2 } else { 4 };
    paint::line(
        at(inner, link_row - 1, 1),
        buffer,
        Line::styled("Share link", theme.dim()),
    );
    let link_cursor = field(
        at(inner, link_row, field_height),
        buffer,
        &state.link,
        theme,
        state.focus == Focus::Link,
        "https://…/#k=v1.…",
        false,
    );
    let destination_row = link_row + field_height + 2;
    paint::line(
        at(inner, destination_row - 1, 1),
        buffer,
        Line::styled("Save to folder", theme.dim()),
    );
    let destination_cursor = field(
        at(inner, destination_row, field_height),
        buffer,
        &state.destination,
        theme,
        state.focus == Focus::Destination,
        "Choose a folder",
        false,
    );
    let action_height = if compact { 1 } else { 3 };
    paint::button(
        at(
            inner,
            inner.height.saturating_sub(action_height),
            action_height,
        ),
        buffer,
        theme,
        "Download securely  ↵",
        state.focus == Focus::Action,
    );
    link_cursor.or(destination_cursor)
}

fn transfers(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut State,
    theme: Theme,
) -> Option<(u16, u16)> {
    let inner = paint::card(centered(area, 90, area.height), buffer, theme, true);
    if inner.height < 4 {
        return None;
    }
    paint::line(
        at(inner, 0, 1),
        buffer,
        Line::styled("Saved transfers", theme.strong().fg(theme.accent())),
    );
    paint::line(
        at(inner, 1, 1),
        buffer,
        Line::styled("Enter resumes · x discards local state", theme.dim()),
    );
    let rows = at(inner, 3, inner.height.saturating_sub(4));
    let capacity = rows.height as usize;
    ensure_scroll(&mut state.queue_scroll, state.queue_cursor, capacity);
    if state.saved_transfers.is_empty() {
        paint::line(
            at(rows, 1, 1),
            buffer,
            Line::styled("No resumable transfers on this device.", theme.dim()),
        );
        return None;
    }
    for (offset, transfer) in state
        .saved_transfers
        .iter()
        .skip(state.queue_scroll)
        .take(capacity)
        .enumerate()
    {
        let row = at(rows, offset as u16, 1);
        let focused = state.queue_scroll + offset == state.queue_cursor;
        Block::default()
            .style(Style::default().bg(if focused {
                theme.selected()
            } else {
                theme.surface()
            }))
            .render(row, buffer);
        let total = if transfer.total == 0 {
            "unknown size".to_owned()
        } else {
            format!("{} / {}", bytes(transfer.done), bytes(transfer.total))
        };
        paint::line(
            row,
            buffer,
            Line::styled(
                clip(
                    &format!("{}  {}  {}", transfer.direction, transfer.state, total),
                    row.width,
                ),
                if focused { theme.strong() } else { theme.dim() },
            ),
        );
    }
    None
}

fn transfer(area: Rect, buffer: &mut Buffer, state: &mut State, theme: Theme) {
    let Some(view) = state.transfer.as_ref() else {
        return;
    };
    let height = if state.receipt.is_some() {
        area.height.min(14)
    } else {
        area.height
    };
    let inner = paint::card(centered(area, 100, height), buffer, theme, false);
    if inner.height < 4 {
        return;
    }
    if let Some(receipt) = &state.receipt {
        let (title, color) = match &receipt.result {
            Ok(_) => (
                match view.direction {
                    Direction::Upload => "Your encrypted link is ready",
                    Direction::Download => "Downloaded and verified",
                    Direction::Update => "Your update is ready",
                },
                theme.success(),
            ),
            Err(_) if receipt.cancelled => ("Transfer cancelled", theme.warning()),
            Err(_) => ("This transfer needs your attention", theme.danger()),
        };
        paint::line(
            at(inner, 0, 1),
            buffer,
            Line::styled(
                format!(
                    "{} {title}",
                    if receipt.result.is_ok() {
                        theme.symbol("✓", "+")
                    } else {
                        theme.symbol("!", "!")
                    }
                ),
                theme.strong().fg(color),
            ),
        );
        paint::line(
            at(inner, 1, 1),
            buffer,
            Line::styled(
                format!(
                    "{} files · {} · {}",
                    view.progress.files,
                    bytes(view.progress.total.unwrap_or(0)),
                    duration(view.elapsed)
                ),
                theme.dim(),
            ),
        );
        let content = match &receipt.result {
            Ok(values) => values.join("\n\n"),
            Err(message) => clean(message),
        };
        let output = at(inner, 3, inner.height.saturating_sub(7).max(1));
        Block::default()
            .style(Style::default().bg(theme.selected()))
            .render(output, buffer);
        let text = output.inner(Margin::new(1, 0));
        if receipt.result.is_ok() && view.direction == Direction::Upload {
            let row = at(text, u16::from(text.height >= 3), 1);
            state.hyperlink = crate::output::Hyperlink::new(row, &content);
            paint::line(
                row,
                buffer,
                Line::styled(
                    clip(&content, row.width),
                    Style::default().fg(theme.accent()),
                ),
            );
        } else {
            let lines = paint::wrap(&content, text.width);
            let rows = lines.len().min(u16::MAX as usize) as u16;
            let paragraph = Paragraph::new(lines.into_iter().map(Line::raw).collect::<Vec<_>>())
                .style(Style::default().fg(theme.text()));
            state.receipt_scroll = state.receipt_scroll.min(rows.saturating_sub(text.height));
            paragraph
                .scroll((state.receipt_scroll, 0))
                .render(text, buffer);
        }
        let action_height = if inner.height >= 10 { 3 } else { 1 };
        let actions = at(
            inner,
            inner.height.saturating_sub(action_height),
            action_height,
        );
        if receipt.result.is_ok() && actions.width >= 38 {
            let width = actions.width / 2 - 1;
            paint::button(
                Rect::new(actions.x, actions.y, width, actions.height),
                buffer,
                theme,
                if view.direction == Direction::Upload {
                    "Copy full link  c"
                } else {
                    "Copy result  c"
                },
                true,
            );
            paint::button(
                Rect::new(
                    actions.x + width + 2,
                    actions.y,
                    actions.width - width - 2,
                    actions.height,
                ),
                buffer,
                theme,
                "New transfer  ↵",
                false,
            );
            return;
        }
        paint::button(
            at(
                inner,
                inner.height.saturating_sub(action_height),
                action_height,
            ),
            buffer,
            theme,
            "New transfer  ↵",
            true,
        );
        return;
    }
    let context = if view.progress.name.is_empty() {
        view.phase_label().into()
    } else {
        format!(
            "{} · {} of {}",
            clean(&view.progress.name),
            view.progress.index,
            view.progress.files
        )
    };
    paint::line(
        at(inner, 0, 1),
        buffer,
        Line::styled(clip(&context, inner.width), theme.strong()),
    );
    let measured = view.progress.total.is_some()
        && !matches!(
            view.progress.phase,
            Phase::Connecting
                | Phase::Preparing
                | Phase::Archiving
                | Phase::Unlocking
                | Phase::Updating
        );
    let bar_row = if inner.height >= 12 { 3 } else { 1 };
    paint::meter(
        at(inner, bar_row, 1),
        buffer,
        theme,
        measured.then_some(view.ratio),
        view.elapsed,
    );
    let percent = if measured {
        format!("{}%", (view.ratio * 100.0).floor() as u64)
    } else {
        paint::spinner(theme, view.elapsed).into()
    };
    let amounts = view
        .progress
        .total
        .map(|total| format!("{} / {}", bytes(view.progress.done), bytes(total)))
        .unwrap_or_else(|| "End-to-end encrypted".into());
    paint::line(
        at(inner, bar_row + 1, 1),
        buffer,
        Line::from(vec![
            Span::styled(format!("{percent}   "), theme.strong()),
            Span::styled(amounts, theme.dim()),
        ]),
    );
    paint::line(
        at(inner, bar_row + 3, 1),
        buffer,
        Line::styled(view.phase_label(), theme.dim()),
    );
    if inner.height >= 10 {
        let stats = format!(
            "{}  elapsed     {}     {}",
            duration(view.elapsed),
            if view.stalled() || view.rate < 1.0 {
                "Measuring speed".into()
            } else {
                format!("{}/s", bytes(view.rate as u64))
            },
            view.eta()
                .map(|eta| format!("{} remaining", duration(eta)))
                .unwrap_or_default()
        );
        paint::line(
            at(inner, bar_row + 5, 1),
            buffer,
            Line::styled(stats, theme.dim()),
        );
    }
    if inner.height >= 15 {
        let data = view.history.iter().copied().collect::<Vec<_>>();
        Sparkline::default()
            .data(&data)
            .style(Style::default().fg(theme.gradient(0.2)))
            .render(at(inner, 10, 2), buffer);
    }
    if inner.height >= 18 {
        let acknowledged = match view.direction {
            Direction::Upload => "Acknowledged",
            _ => "Authenticated",
        };
        paint::line(
            at(inner, inner.height - 1, 1),
            buffer,
            Line::styled(
                format!(
                    "{acknowledged} {} · final verification before completion",
                    bytes(view.progress.committed)
                ),
                theme.dim(),
            ),
        );
    }
}

fn footer_view(area: Rect, buffer: &mut Buffer, state: &State, theme: Theme) {
    if let Some((notice, _)) = &state.notice {
        paint::line(
            area,
            buffer,
            Line::styled(
                clip(notice, area.width),
                Style::default().fg(theme.warning()),
            ),
        );
        return;
    }
    let hints: Vec<(&str, &str)> = if state.prompt.is_some() {
        vec![("Enter", "unlock"), ("Esc", "cancel")]
    } else if state.transfer.is_some() && state.receipt.is_none() {
        vec![("Ctrl+C", "cancel transfer"), ("?", "help")]
    } else if state.receipt.is_some() {
        vec![
            ("c", "copy"),
            ("Enter", "new transfer"),
            ("↑↓", "scroll"),
            ("q", "finish"),
        ]
    } else if state.searching {
        vec![("Enter", "apply search"), ("Esc", "clear")]
    } else if state.mode == Mode::Receive {
        vec![
            ("Tab", "next field"),
            ("Enter", "continue"),
            ("Esc", "actions"),
        ]
    } else if state.mode == Mode::Transfers {
        vec![("Enter", "resume"), ("x", "discard"), ("↑↓", "choose")]
    } else {
        vec![
            ("Space", "select"),
            ("u", "send"),
            ("Tab", "focus"),
            ("/", "search"),
            ("?", "help"),
        ]
    };
    let mut spans = Vec::new();
    let mut used = 0;
    for (name, label) in hints {
        let width = name.len() + label.len() + 6;
        if used + width > area.width as usize {
            break;
        }
        used += width;
        spans.extend(paint::key(theme, name, label));
    }
    paint::line(area, buffer, Line::from(spans));
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn dim(area: Rect, buffer: &mut Buffer, theme: Theme) {
    buffer.set_style(
        area,
        Style::default()
            .fg(theme.border())
            .bg(theme.bg())
            .remove_modifier(Modifier::BOLD),
    );
}

fn help(area: Rect, buffer: &mut Buffer, theme: Theme) {
    let inner = paint::card(centered(area, 76, 18), buffer, theme, true);
    let items = [
        ("Your keyboard, a little more powerful.", ""),
        ("", ""),
        ("1 / 2 / 3", "Send files / receive a link / saved transfers"),
        ("Tab / Shift+Tab", "Move between panels or form fields"),
        ("↑↓ / j k", "Move through files"),
        ("Space", "Add or remove a file"),
        ("Enter / Backspace", "Open folder / go to parent"),
        ("/  then type", "Search files (Enter applies, Esc clears)"),
        (". / Ctrl+R", "Hidden files / refresh directory"),
        ("u / U", "Send selected files / update beam"),
        ("c", "Copy a completed result via terminal clipboard"),
        ("Ctrl+C", "Cancel active transfer, or exit when idle"),
        ("Esc / Enter", "Close this help"),
    ];
    for (row, (key, text)) in items.iter().enumerate() {
        paint::line(
            at(inner, row as u16, 1),
            buffer,
            Line::from(vec![
                Span::styled(format!("{key:<20}"), theme.strong().fg(theme.accent())),
                Span::styled((*text).to_owned(), theme.dim()),
            ]),
        );
    }
}

fn secret(area: Rect, buffer: &mut Buffer, state: &State, theme: Theme) -> Option<(u16, u16)> {
    let prompt = state.prompt.as_ref()?;
    let inner = paint::card(centered(area, 72, 13), buffer, theme, true);
    paint::line(
        at(inner, 0, 1),
        buffer,
        Line::styled(
            "Only you can unlock this.",
            theme.strong().fg(theme.accent()),
        ),
    );
    paint::line(
        at(inner, 2, 1),
        buffer,
        Line::styled(prompt.kind.label(), theme.dim()),
    );
    let cursor = field(
        at(inner, 3, 3),
        buffer,
        &state.secret,
        theme,
        true,
        "",
        true,
    );
    paint::line(
        at(inner, 7, 1),
        buffer,
        Line::styled("Enter to unlock · Esc to cancel", theme.dim()),
    );
    cursor
}

#[cfg(test)]
mod tests {
    use super::super::state::Receipt;
    use super::*;
    use crate::{
        app::{Progress, TransferView},
        config::Config,
    };

    #[test]
    fn every_view_renders_at_small_and_large_sizes() {
        let directory = tempfile::tempdir().unwrap();
        let config = Config::default();
        let theme = Theme::new(&config);
        for (width, height) in [(1, 1), (40, 14), (60, 20), (80, 24), (120, 36), (160, 48)] {
            let mut state =
                State::new(&config, "http://localhost:8000", directory.path().into()).unwrap();
            for screen in 0..6 {
                let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
                state.mode = if screen == 1 {
                    Mode::Receive
                } else {
                    Mode::Send
                };
                if screen >= 2 {
                    let mut view = TransferView::new(Direction::Upload);
                    view.progress = Progress {
                        name: "日本語-long-file.zip".into(),
                        total: Some(1024),
                        done: 512,
                        files: 2,
                        index: 1,
                        phase: Phase::Sending,
                        ..Progress::default()
                    };
                    state.transfer = Some(view);
                }
                if screen >= 3 {
                    state.receipt = Some(Receipt {
                        result: Ok(vec!["http://localhost:8000/example#k=v1.example".into()]),
                        cancelled: false,
                    });
                }
                if screen == 4 {
                    state.receipt = Some(Receipt {
                        result: Err("Server unavailable".into()),
                        cancelled: false,
                    });
                }
                state.help = screen == 5;
                render(buffer.area, &mut buffer, &mut state, theme);
                assert_eq!(buffer.content.len(), width as usize * height as usize);
            }
        }
    }

    #[test]
    fn focused_file_stays_visible_when_scrolling() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..80 {
            std::fs::write(directory.path().join(format!("file-{index:02}.txt")), b"x").unwrap();
        }
        let config = Config::default();
        let mut state =
            State::new(&config, "http://localhost:8000", directory.path().into()).unwrap();
        state.cursor = 79;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
        render(buffer.area, &mut buffer, &mut state, Theme::fixture());
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("file-79.txt"));
        assert!(state.scroll > 0);
    }

    #[test]
    fn saved_transfers_render_without_a_transfer_job() {
        let directory = tempfile::tempdir().unwrap();
        let config = Config::default();
        let mut state =
            State::new(&config, "http://localhost:8000", directory.path().into()).unwrap();
        state.mode = Mode::Transfers;
        state.saved_transfers.push(crate::protocol::SavedTransfer {
            id: "job-1".into(),
            direction: "upload".into(),
            state: "retrying".into(),
            done: 10,
            total: 100,
        });
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
        render(buffer.area, &mut buffer, &mut state, Theme::fixture());
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("retrying"));
    }

    #[test]
    fn cancelled_download_receipt_has_a_stable_header() {
        let directory = tempfile::tempdir().unwrap();
        let config = Config::default();
        let mut state =
            State::new(&config, "http://localhost:8000", directory.path().into()).unwrap();
        state.transfer = Some(TransferView::new(Direction::Download));
        state.receipt = Some(Receipt {
            result: Err("Transfer cancelled".into()),
            cancelled: true,
        });
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
        render(buffer.area, &mut buffer, &mut state, Theme::fixture());
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("Transfer cancelled"));
    }

    /// Set BEAM_VISUAL_DIR when reviewing actual cell colors, hierarchy and wrapping in a browser.
    #[test]
    fn export_visual_fixtures() {
        let Some(output) = std::env::var_os("BEAM_VISUAL_DIR") else {
            return;
        };
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        let directory = tempfile::tempdir().unwrap();
        for (name, size) in [
            ("Brand assets", 0),
            ("Product brief.pdf", 2_500_000),
            ("Design references.png", 5_900_000),
            ("Release notes.md", 1230),
            ("Weekend photos.zip", 41_234_567),
            ("QA 日本語.txt", 253),
        ] {
            if size == 0 {
                std::fs::create_dir(directory.path().join(name)).unwrap();
            } else {
                std::fs::File::create(directory.path().join(name))
                    .unwrap()
                    .set_len(size)
                    .unwrap();
            }
        }
        for (width, height) in [(80, 24), (120, 36)] {
            for screen in ["send", "receive", "transfer", "receipt", "error"] {
                let config = Config::default();
                let mut state =
                    State::new(&config, "http://localhost:8000", directory.path().into()).unwrap();
                state.directory = "/home/you/Files".into();
                state.info = Some(Ok(crate::protocol::Info::fixture()));
                for entry in state
                    .entries
                    .iter()
                    .filter(|entry| entry.name.ends_with(".pdf") || entry.name.ends_with(".png"))
                {
                    state.selected.insert(entry.path.clone(), entry.clone());
                }
                state.cursor = 1;
                if screen == "receive" {
                    state.mode = Mode::Receive;
                    state.focus = Focus::Link;
                }
                if matches!(screen, "transfer" | "receipt" | "error") {
                    let mut view = TransferView::new(Direction::Upload);
                    view.progress = Progress {
                        name: "Product brief.pdf".into(),
                        total: Some(8_400_000),
                        done: 5_600_000,
                        committed: 4_000_000,
                        wire_bytes: 5_600_016,
                        files: 2,
                        index: 2,
                        phase: Phase::Sending,
                        ..Progress::default()
                    };
                    view.ratio = 0.667;
                    view.rate = 2_312_345.0;
                    view.elapsed = std::time::Duration::from_secs(4);
                    view.history = (0..48)
                        .map(|i| 500_000 + ((i as f64 / 3.0).sin().abs() * 2_000_000.0) as u64)
                        .collect();
                    state.transfer = Some(view);
                }
                if screen == "receipt" {
                    state.receipt = Some(Receipt { result: Ok(vec!["http://localhost:8000/01ARZ3NDEKTSV4RRFFQ69G5FAV#k=v1.O8NJb0nH6W6aF0rTnRt2LTVemIHjfFVjsGWgRq5edW0".into()]), cancelled: false });
                }
                if screen == "error" {
                    state.receipt = Some(Receipt {
                        result: Err(
                            "The transfer is unavailable. It may have expired or been deleted."
                                .into(),
                        ),
                        cancelled: false,
                    });
                }
                let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
                render(buffer.area, &mut buffer, &mut state, Theme::fixture());
                let mut html = "<!doctype html><meta charset=utf-8><style>body{margin:0;background:#0b0914;color:#f7f5ff;font:16px/20px 'DejaVu Sans Mono',monospace}.row{height:20px;white-space:pre}span{display:inline-block;vertical-align:top;height:20px}</style>".to_owned();
                for y in 0..height {
                    html.push_str("<div class=row>");
                    let mut x = 0;
                    while x < width {
                        use unicode_width::UnicodeWidthStr;
                        let cell = &buffer[(x, y)];
                        let cells = cell.symbol().width().max(1) as u16;
                        let symbol = cell
                            .symbol()
                            .replace('&', "&amp;")
                            .replace('<', "&lt;")
                            .replace('>', "&gt;");
                        let color = |value| match value {
                            ratatui::style::Color::Rgb(r, g, b) => {
                                format!("#{r:02x}{g:02x}{b:02x}")
                            }
                            _ => "inherit".into(),
                        };
                        html.push_str(&format!("<span style=\"width:{}px;color:{};background:{};font-weight:{}\">{symbol}</span>", cells * 10, color(cell.fg), color(cell.bg), if cell.modifier.contains(Modifier::BOLD) { 700 } else { 400 }));
                        x += cells;
                    }
                    html.push_str("</div>");
                }
                std::fs::write(output.join(format!("{screen}-{width}x{height}.html")), html)
                    .unwrap();
            }
        }
    }
}
