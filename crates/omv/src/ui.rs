use omv_config::LineNumbers;
use omv_core::Mode;
use omv_lsp::lsp_types::DiagnosticSeverity;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthChar;

use crate::app::{App, Focus};
use crate::theme::Theme;

const EXPLORER_WIDTH: u16 = 30;
const DIAGNOSTICS_HEIGHT: u16 = 8;
const SIGN_WIDTH: usize = 2;

/// Severity order for "worst on this line wins". `DiagnosticSeverity`'s inner
/// value is private, so the ranking is spelled out rather than derived.
fn severity_rank(severity: DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::ERROR => 0,
        DiagnosticSeverity::WARNING => 1,
        DiagnosticSeverity::INFORMATION => 2,
        _ => 3,
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let [body, status, command] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let [explorer_area, main_area] = if app.explorer.visible {
        Layout::horizontal([Constraint::Length(EXPLORER_WIDTH), Constraint::Min(1)]).areas(body)
    } else {
        [Rect::ZERO, body]
    };

    let [text_area, diagnostics_area] = if app.diagnostics_visible {
        Layout::vertical([Constraint::Min(1), Constraint::Length(DIAGNOSTICS_HEIGHT)])
            .areas(main_area)
    } else {
        [main_area, Rect::ZERO]
    };

    // The core needs the real viewport height for half-page motions and scrolling.
    app.text_height = text_area.height as usize;
    app.editor.viewport_height = app.text_height;

    if app.explorer.visible {
        render_explorer(frame, app, explorer_area);
    }
    render_text(frame, app, text_area);
    if app.diagnostics_visible {
        render_diagnostics(frame, app, diagnostics_area);
    }
    render_status(frame, app, status);
    render_command(frame, app, command);

    if app.picker.is_some() {
        render_picker(frame, app, body);
    }
    if app.hover.is_some() {
        render_hover(frame, app, text_area);
    }
}

/// Width of a character on screen, with tabs advancing to the next tab stop.
fn char_width(c: char, column: usize, tab_width: usize) -> usize {
    match c {
        '\t' => tab_width - (column % tab_width),
        _ => UnicodeWidthChar::width(c).unwrap_or(0),
    }
}

fn render_text(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let theme = &app.theme;
    let buffer = app.editor.buffer();
    let tab_width = app.editor.indent_width;
    let cursor_line = buffer.cursor_position().line;

    let total_lines = buffer.line_count();
    let number_width = match app.config.line_numbers {
        LineNumbers::None => 0,
        _ => total_lines.to_string().len().max(3) + 1,
    };
    let gutter_width = number_width + SIGN_WIDTH;

    let selection = buffer.selection_range(app.editor.mode == Mode::VisualLine);
    let diagnostics = app.current_diagnostics();

    let mut lines: Vec<Line> = Vec::with_capacity(area.height as usize);
    let mut cursor_screen: Option<(u16, u16)> = None;

    for row in 0..area.height as usize {
        let line_index = app.scroll + row;
        if line_index >= total_lines {
            lines.push(Line::from(Span::styled(
                "~",
                Style::default().fg(theme.gutter),
            )));
            continue;
        }

        let mut spans: Vec<Span> = Vec::new();

        // Sign column: the worst diagnostic on this line wins.
        let severity = diagnostics
            .iter()
            .filter(|d| d.range.start.line as usize == line_index)
            .map(|d| d.severity.unwrap_or(DiagnosticSeverity::ERROR))
            .min_by_key(|s| severity_rank(*s));
        let sign = match severity {
            Some(s) => Span::styled("● ", Style::default().fg(theme.severity_color(Some(s)))),
            None => Span::raw("  "),
        };
        spans.push(sign);

        if number_width > 0 {
            let is_current = line_index == cursor_line;
            let shown = match app.config.line_numbers {
                LineNumbers::Absolute => line_index + 1,
                LineNumbers::Relative if is_current => line_index + 1,
                LineNumbers::Relative => cursor_line.abs_diff(line_index),
                LineNumbers::None => 0,
            };
            let style = if is_current {
                Style::default()
                    .fg(theme.gutter_current)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.gutter)
            };
            // Current line flush-left under relative numbering, like vim.
            let text = if is_current && app.config.line_numbers == LineNumbers::Relative {
                format!("{:<width$}", shown, width = number_width)
            } else {
                format!("{:>width$}", shown, width = number_width - 1)
            };
            spans.push(Span::styled(text, style));
            if !(is_current && app.config.line_numbers == LineNumbers::Relative) {
                spans.push(Span::raw(" "));
            }
        }

        let start = buffer.line_start(line_index);
        let end = buffer.line_end(line_index);
        let line_start_byte = buffer.rope.char_to_byte(start);

        // Style runs, so a line becomes a handful of spans rather than one per char.
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        let mut column = 0usize;

        for char_idx in start..end {
            let c = buffer.rope.char(char_idx);
            let byte = line_start_byte + buffer.rope.slice(start..char_idx).len_bytes();

            let mut style = app
                .highlights
                .binary_search_by(|s| {
                    if s.end <= byte {
                        std::cmp::Ordering::Less
                    } else if s.start > byte {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .ok()
                .map(|i| theme.style_for(app.highlights[i].kind))
                .unwrap_or_else(|| Style::default().fg(theme.foreground));

            if selection.is_some_and(|(lo, hi)| (lo..hi).contains(&char_idx)) {
                style = style.bg(theme.selection);
            } else if line_index == cursor_line {
                style = style.bg(theme.cursor_line);
            }

            if char_idx == buffer.cursor {
                let x = area.x + (gutter_width + column) as u16;
                let y = area.y + row as u16;
                cursor_screen = Some((x, y));
            }

            if run_style != Some(style) {
                if let Some(previous) = run_style.take() {
                    spans.push(Span::styled(std::mem::take(&mut run), previous));
                }
                run_style = Some(style);
            }
            let width = char_width(c, column, tab_width);
            if c == '\t' {
                run.push_str(&" ".repeat(width));
            } else {
                run.push(c);
            }
            column += width;
        }
        if let Some(style) = run_style {
            spans.push(Span::styled(run, style));
        }

        // The cursor may sit one past the last character (end of line, insert mode).
        if buffer.cursor == end && line_index == cursor_line {
            cursor_screen = Some((area.x + (gutter_width + column) as u16, area.y + row as u16));
        }

        lines.push(Line::from(spans));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(theme.background)),
        area,
    );

    if let Some((x, y)) = cursor_screen
        && app.focus == Focus::Editor
        && x < area.right()
        && y < area.bottom()
    {
        frame.set_cursor_position((x, y));
    }
}

fn render_explorer(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let focused = app.focus == Focus::Explorer;
    let block = panel_block(theme, " Explorer ", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let height = inner.height as usize;
    let offset = app
        .explorer
        .selected
        .saturating_sub(height.saturating_sub(1));

    let lines: Vec<Line> = app
        .explorer
        .rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(index, row)| {
            let marker = if row.entry.is_dir {
                if row.expanded { "▾ " } else { "▸ " }
            } else {
                "  "
            };
            let indent = "  ".repeat(row.depth);
            let mut style = if row.entry.is_dir {
                Style::default()
                    .fg(theme.directory)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.foreground)
            };
            if index == app.explorer.selected {
                style = style.bg(theme.selection);
            }
            Line::from(Span::styled(
                format!("{indent}{marker}{}", row.entry.name),
                style,
            ))
        })
        .collect();

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_diagnostics(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let diagnostics = app.current_diagnostics();
    let title = format!(" Diagnostics ({}) ", diagnostics.len());
    let block = panel_block(theme, &title, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = diagnostics
        .iter()
        .take(inner.height as usize)
        .map(|d| {
            let color = theme.severity_color(d.severity);
            Line::from(vec![
                Span::styled(
                    format!("{:>5}  ", d.range.start.line + 1),
                    Style::default().fg(theme.gutter),
                ),
                Span::styled(d.message.replace('\n', " "), Style::default().fg(color)),
            ])
        })
        .collect();

    let body = if lines.is_empty() {
        Text::from(Line::from(Span::styled(
            "  no diagnostics",
            Style::default().fg(theme.gutter),
        )))
    } else {
        Text::from(lines)
    };
    frame.render_widget(Paragraph::new(body), inner);
}

fn render_picker(frame: &mut Frame, app: &App, area: Rect) {
    let Some(picker) = &app.picker else { return };
    let theme = &app.theme;

    let width = area.width.saturating_sub(8).clamp(20, 100);
    let height = area.height.saturating_sub(6).clamp(6, 22);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let count = picker.matches.len();
    let title = if picker.loading {
        format!("{}… ", picker.kind.title())
    } else if picker.truncated {
        format!("{}{count}+ ", picker.kind.title())
    } else {
        format!("{}{count} ", picker.kind.title())
    };
    let block = panel_block(theme, &title, true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [query_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.panel_title)),
            Span::styled(picker.query.clone(), Style::default().fg(theme.foreground)),
        ])),
        query_area,
    );

    let height = list_area.height as usize;
    let offset = picker.selected.saturating_sub(height.saturating_sub(1));

    let rows: Vec<Line> = picker
        .visible()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(index, (display, positions))| {
            let selected = index == picker.selected;
            let base = if selected {
                Style::default().fg(theme.foreground).bg(theme.selection)
            } else {
                Style::default().fg(theme.foreground)
            };
            // Per-char spans only for matched rows; picker lists are short.
            let spans: Vec<Span> = display
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let style = if positions.contains(&(i as u32)) {
                        base.fg(theme.match_highlight).add_modifier(Modifier::BOLD)
                    } else {
                        base
                    };
                    Span::styled(c.to_string(), style)
                })
                .collect();
            Line::from(spans)
        })
        .collect();

    let body = if rows.is_empty() {
        let text = if picker.loading {
            "  searching…"
        } else {
            "  no matches"
        };
        Text::from(Line::from(Span::styled(
            text,
            Style::default().fg(theme.gutter),
        )))
    } else {
        Text::from(rows)
    };
    frame.render_widget(Paragraph::new(body), list_area);
}

fn render_hover(frame: &mut Frame, app: &App, area: Rect) {
    let Some(text) = &app.hover else { return };
    let theme = &app.theme;

    let width = area.width.saturating_sub(4).clamp(20, 80);
    let lines = text.lines().count() as u16 + 2;
    let height = lines.min(area.height.saturating_sub(2)).max(3);
    let cursor_row = (app
        .editor
        .buffer()
        .cursor_position()
        .line
        .saturating_sub(app.scroll)) as u16;

    // Prefer below the cursor, flip above when there isn't room.
    let y = if cursor_row + 1 + height <= area.height {
        area.y + cursor_row + 1
    } else {
        area.y + cursor_row.saturating_sub(height)
    };

    let popup = Rect {
        x: area.x + 2,
        y,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let block = panel_block(theme, " Hover ", true);
    frame.render_widget(
        Paragraph::new(text.as_str())
            .style(Style::default().fg(theme.foreground))
            .wrap(Wrap { trim: false })
            .block(block),
        popup,
    );
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let buffer = app.editor.buffer();
    let position = buffer.cursor_position();
    let mode = app.editor.mode;

    let diagnostics = app.current_diagnostics();
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity.is_none_or(|s| s == DiagnosticSeverity::ERROR))
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .count();

    let mut left = vec![
        Span::styled(
            format!(" {} ", mode.label()),
            Style::default()
                .fg(theme.status_bg)
                .bg(theme.mode_color(mode))
                .bold(),
        ),
        Span::styled(
            format!(
                " {}{} ",
                buffer.name(),
                if buffer.modified { " [+]" } else { "" }
            ),
            Style::default().fg(theme.status_fg).bg(theme.status_bg),
        ),
    ];
    if errors > 0 {
        left.push(Span::styled(
            format!(" E{errors} "),
            Style::default().fg(theme.error).bg(theme.status_bg),
        ));
    }
    if warnings > 0 {
        left.push(Span::styled(
            format!(" W{warnings} "),
            Style::default().fg(theme.warning).bg(theme.status_bg),
        ));
    }

    let pending = app.resolver.pending_display();
    let right = format!(
        " {}{}:{} ",
        if pending.is_empty() {
            String::new()
        } else {
            format!("{pending}  ")
        },
        position.line + 1,
        position.column + 1
    );

    let used: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let padding = (area.width as usize).saturating_sub(used + right.chars().count());
    left.push(Span::styled(
        " ".repeat(padding),
        Style::default().bg(theme.status_bg),
    ));
    left.push(Span::styled(
        right,
        Style::default().fg(theme.status_fg).bg(theme.status_bg),
    ));

    frame.render_widget(Paragraph::new(Line::from(left)), area);
}

fn render_command(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let line = if app.editor.mode == Mode::Command {
        let prefix = if app.editor.command_line.starts_with('/') {
            ""
        } else {
            ":"
        };
        Line::from(Span::styled(
            format!("{prefix}{}", app.editor.command_line),
            Style::default().fg(theme.foreground),
        ))
    } else {
        Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(theme.gutter_current),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);

    if app.editor.mode == Mode::Command {
        let column = app.editor.command_line.chars().count()
            + usize::from(!app.editor.command_line.starts_with('/'));
        frame.set_cursor_position((area.x + column as u16, area.y));
    }
}

fn panel_block<'a>(theme: &Theme, title: &'a str, focused: bool) -> Block<'a> {
    let border = if focused {
        theme.panel_border
    } else {
        theme.gutter
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme.panel_title)
                .add_modifier(Modifier::BOLD),
        ))
}
