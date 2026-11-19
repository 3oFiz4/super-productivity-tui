use crate::app::{
    ActiveModal, AddEntityField, AddEntityKind, AddTaskField, App, CompletionKind, FocusSection,
    NavFilter, NavItemKind, PropertiesMode, PropertiesTarget, SettingsField, ThemeEditorState,
};
use crate::config::KeybindMode;
use crate::keybinds::all_keybinds;
use crate::models::parse_hex_color;
use chrono::Local;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};

/// Underlines specific character indices when `hint` is enabled.
pub fn hint_indices(
    text: &str,
    hint: bool,
    indices: &[usize],
    base: Style,
    hint_color: Color,
) -> Vec<Span<'static>> {
    if !hint {
        return vec![Span::styled(text.to_string(), base)];
    }
    let mut spans = Vec::new();
    let mut buf = String::new();
    for (i, ch) in text.chars().enumerate() {
        if indices.contains(&i) {
            if !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), base));
            }
            spans.push(Span::styled(
                ch.to_string(),
                base.fg(hint_color).add_modifier(Modifier::UNDERLINED),
            ));
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, base));
    }
    spans
}

/// Underlines the first character of each word when `hint` is enabled.
pub fn hint_spans(text: &str, hint: bool, base: Style, hint_color: Color) -> Vec<Span<'static>> {
    if !hint {
        return vec![Span::styled(text.to_string(), base)];
    }
    let mut spans = Vec::new();
    let mut at_word_start = true;
    let mut buf = String::new();

    for ch in text.chars() {
        if ch.is_whitespace() {
            buf.push(ch);
            at_word_start = true;
        } else if at_word_start {
            if !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), base));
            }
            spans.push(Span::styled(
                ch.to_string(),
                base.fg(hint_color).add_modifier(Modifier::UNDERLINED),
            ));
            at_word_start = false;
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, base));
    }
    spans
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let root = frame.area();
    app.areas.nav_rows.clear();
    app.areas.task_rows.clear();

    let bg = app.config.theme.bg_color();
    let fg = app.config.theme.fg_color();
    frame.render_widget(Block::default().style(Style::default().bg(bg).fg(fg)), root);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(root);

    let body = outer[0];
    let status_bar = outer[1];

    if app.show_sidebar {
        let sidebar_w = (body.width * 20 / 100).max(22).min(body.width / 2);
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(sidebar_w),
                Constraint::Length(1),
                Constraint::Min(20),
            ])
            .split(body);

        app.areas.nav = cols[0];
        app.areas.divider = cols[1];
        app.areas.main = cols[2];

        render_sidebar(frame, app, cols[0]);
        render_divider(frame, app, cols[1]);
        render_main(frame, app, cols[2]);
    } else {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Min(20)])
            .split(body);

        app.areas.nav = Rect::default();
        app.areas.divider = cols[0];
        app.areas.main = cols[1];

        render_divider(frame, app, cols[0]);
        render_main(frame, app, cols[1]);
    }

    render_status_bar(frame, app, status_bar);

    if !matches!(app.active_modal, ActiveModal::None | ActiveModal::Theme) {
        dim_background(frame, root);
    }

    match app.active_modal {
        ActiveModal::None => {}
        ActiveModal::Properties => render_properties_modal(frame, app, root),
        ActiveModal::Help => render_help_modal(frame, app, root),
        ActiveModal::Settings => render_settings_modal(frame, app, root),
        ActiveModal::AddTask => render_add_task_modal(frame, app, root),
        ActiveModal::Theme => render_theme_modal(frame, app, root),
        ActiveModal::FullNote => render_full_note_modal(frame, app, root),
        ActiveModal::AddEntity => render_add_entity_modal(frame, app, root),
    }
}

fn dim_background(frame: &mut Frame, area: Rect) {
    let buf = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            cell.set_fg(Color::DarkGray);
            cell.set_bg(Color::Reset);
        }
    }
}

fn render_sidebar(frame: &mut Frame, app: &mut App, area: Rect) {
    frame.render_widget(
        Block::default().style(Style::default().bg(app.config.theme.sidebar_bg_color())),
        area,
    );
    let (top_rows, bottom_rows) = app.nav_rows();
    let bottom_h = bottom_rows.len() as u16 + 1;
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(bottom_h)])
        .split(area);

    let top_area = sections[0];
    let bottom_area = sections[1];

    let mut global_idx = 0;
    for (i, row) in top_rows.iter().enumerate() {
        let y = top_area.y + i as u16;
        if y >= top_area.bottom() {
            break;
        }
        let row_rect = Rect::new(top_area.x, y, top_area.width, 1);
        app.areas.nav_rows.push((row_rect, global_idx));
        render_nav_row(frame, app, row, global_idx, row_rect);
        global_idx += 1;
    }

    if bottom_area.height > 0 {
        let sep_rect = Rect::new(bottom_area.x, bottom_area.y, bottom_area.width, 1);
        let sep = "─".repeat(bottom_area.width as usize);
        frame.render_widget(
            Paragraph::new(sep).style(Style::default().fg(app.config.theme.border_color())),
            sep_rect,
        );
    }

    for (i, row) in bottom_rows.iter().enumerate() {
        let y = bottom_area.y + 1 + i as u16;
        if y >= bottom_area.bottom() {
            break;
        }
        let row_rect = Rect::new(bottom_area.x, y, bottom_area.width, 1);
        app.areas.nav_rows.push((row_rect, global_idx));
        render_nav_row(frame, app, row, global_idx, row_rect);
        global_idx += 1;
    }
}

fn render_nav_row(frame: &mut Frame, app: &App, row: &crate::app::NavRow, idx: usize, rect: Rect) {
    let is_selected = app.focus == FocusSection::Nav && app.nav_selected == idx;
    let is_active_filter = matches!(&row.kind, NavItemKind::Filter(f) if *f == app.active_filter);

    let fg = match &row.kind {
        NavItemKind::Filter(NavFilter::Project(pid)) => app
            .projects
            .iter()
            .find(|p| &p.id == pid)
            .map(|p| p.display_color())
            .unwrap_or(Color::Magenta),
        NavItemKind::Filter(NavFilter::Tag(tid)) => app
            .tags
            .iter()
            .find(|t| &t.id == tid)
            .map(|t| t.display_color())
            .unwrap_or(Color::Green),
        _ => app.config.theme.fg_color(),
    };

    let mut style = Style::default().fg(fg);
    if is_active_filter {
        style = style.add_modifier(Modifier::BOLD);
    }
    if is_selected {
        style = style
            .bg(app.config.theme.selection_bg_color())
            .fg(app.config.theme.fg_color());
    }

    let indent_str = " ".repeat(row.indent as usize);
    let left = format!("{indent_str}{} {}", row.icon, row.label);
    let right = row.count.map(|c| c.to_string()).unwrap_or_default();
    let pad =
        (rect.width as usize).saturating_sub(left.chars().count() + right.chars().count() + 2);
    let line_str = format!(" {left}{}{right} ", " ".repeat(pad));

    frame.render_widget(Paragraph::new(line_str).style(style), rect);
}

fn render_divider(frame: &mut Frame, app: &App, area: Rect) {
    let mid = area.height / 2;
    let hint_char = if app.config.keybind_mode == KeybindMode::Nvim {
        'z'
    } else {
        'h'
    };
    let accent = app.config.theme.accent_color();
    let border = app.config.theme.border_color();

    for offset in 0..area.height {
        let y = area.y + offset;
        let rect = Rect::new(area.x, y, 1, 1);
        if offset == mid {
            let style = if app.config.hint_keybind {
                Style::default()
                    .fg(accent)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(accent)
            };
            frame.render_widget(Paragraph::new(hint_char.to_string()).style(style), rect);
        } else {
            frame.render_widget(Paragraph::new("│").style(Style::default().fg(border)), rect);
        }
    }
}

fn render_main(frame: &mut Frame, app: &mut App, area: Rect) {
    let padded = Block::default().padding(Padding::symmetric(4, 2));
    let inner = padded.inner(area);
    frame.render_widget(padded, area);

    let tasks = app.filtered_tasks();
    if tasks.is_empty() {
        let msg = if app.connected {
            "No tasks in this view. Press 'a' to add one."
        } else {
            "Not connected to Super Productivity. Press ':s' for Settings."
        };
        frame.render_widget(
            Paragraph::new(msg)
                .style(Style::default().fg(app.config.theme.muted_color()))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let today_str = Local::now().format("%Y-%m-%d").to_string();
    let search_height = if app.search_active || !app.search_query.is_empty() {
        1
    } else {
        0
    };
    let note_height = if app.show_note { 6 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(search_height),
            Constraint::Min(3),
            Constraint::Length(note_height),
        ])
        .split(inner);

    let search_area = chunks[0];
    let list_area = chunks[1];
    let note_area = chunks[2];

    if search_height > 0 {
        let cursor = if app.search_active { "_" } else { "" };
        let search_line = Line::from(vec![
            Span::styled(
                "/ ",
                Style::default()
                    .fg(app.config.theme.accent_color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}{}", app.search_query, cursor),
                Style::default().fg(app.config.theme.fg_color()),
            ),
        ]);
        frame.render_widget(Paragraph::new(search_line), search_area);
    }

    let visible_h = list_area.height as usize;
    if visible_h > 0 {
        if app.task_selected < app.task_scroll {
            app.task_scroll = app.task_selected;
        } else if app.task_selected >= app.task_scroll + visible_h {
            app.task_scroll = app.task_selected + 1 - visible_h;
        }
    }
    let rows = app.visible_task_rows();

    let mut task_rows = Vec::new();

    for (row_y, (idx, (task, is_subtask))) in
        (list_area.y..list_area.bottom()).zip(rows.iter().enumerate().skip(app.task_scroll))
    {
        let row_rect = Rect::new(list_area.x, row_y, list_area.width, 1);
        task_rows.push((row_rect, idx));

        let is_selected = app.focus == FocusSection::Main && app.task_selected == idx;
        let is_renaming = matches!(
            &app.rename_target,
            Some(crate::app::RenameTarget::Task(id)) if id == &task.id
        );

        let checkbox = if task.is_done { "\u{f046}" } else { "\u{f096}" };

        if *is_subtask {
            let mut sub_style = Style::default().fg(app.config.theme.muted_color());
            if task.is_done {
                sub_style = sub_style.add_modifier(Modifier::CROSSED_OUT);
            }
            if is_selected {
                sub_style = sub_style
                    .fg(app.config.theme.fg_color())
                    .bg(app.config.theme.selection_bg_color());
            }
            if is_renaming {
                sub_style = Style::default()
                    .fg(app.config.theme.success_color())
                    .add_modifier(Modifier::BOLD);
            }
            let title_text = if is_renaming {
                format!("{}|", app.rename_buffer)
            } else {
                task.title.clone()
            };
            let line = format!("    {checkbox} {title_text}");
            let pad = (row_rect.width as usize).saturating_sub(line.chars().count());
            frame.render_widget(
                Paragraph::new(format!("{line}{}", " ".repeat(pad))).style(sub_style),
                row_rect,
            );
            continue;
        }

        let subtasks = app.subtasks_for(&task.id);
        let sub_expanded = app.expanded_subtasks.contains(&task.id);
        let sub_badge = if subtasks.is_empty() {
            String::new()
        } else if sub_expanded {
            format!(" ▾({})", subtasks.len())
        } else {
            format!(" ▸({})", subtasks.len())
        };

        let is_tracking = app.current_task_id.as_deref() == Some(task.id.as_str());
        let tracking_prefix = if is_tracking { "\u{f04b} " } else { "" };
        let title_text = if is_renaming {
            format!("{}|", app.rename_buffer)
        } else {
            format!("{tracking_prefix}{}{}", task.title, sub_badge)
        };

        let mut meta_parts = Vec::new();
        let spent = task.time_spent.unwrap_or(0);
        let est = task.time_estimate.unwrap_or(0);
        if spent > 0 || est > 0 {
            meta_parts.push(Span::styled(
                format!(" \u{f017} {}/{} ", format_ms(spent), format_ms(est)),
                Style::default().fg(if is_tracking {
                    app.config.theme.success_color()
                } else {
                    app.config.theme.muted_color()
                }),
            ));
        }
        if let Some(due) = &task.due_day {
            let due_color = if due.as_str() < today_str.as_str() {
                app.config.theme.danger_color()
            } else if due.as_str() == today_str.as_str() {
                app.config.theme.warning_color()
            } else {
                app.config.theme.muted_color()
            };
            meta_parts.push(Span::styled(
                format!(" \u{f073} {due} "),
                Style::default().fg(due_color),
            ));
        }
        if task.repeat_cfg_id.is_some() {
            meta_parts.push(Span::styled(
                " \u{f01e} Repeat ",
                Style::default().fg(app.config.theme.muted_color()),
            ));
        }
        if let Some(pid) = &task.project_id
            && let Some(proj) = app.projects.iter().find(|p| &p.id == pid)
        {
            meta_parts.push(Span::styled(
                format!(" \u{f07b} {} ", proj.title),
                Style::default().fg(proj.display_color()),
            ));
        }
        for tid in &task.tag_ids {
            if let Some(tag) = app.tags.iter().find(|t| &t.id == tid) {
                meta_parts.push(Span::styled(
                    format!(" \u{f292} {} ", tag.title),
                    Style::default().fg(tag.display_color()),
                ));
            }
        }

        let mut row_style = Style::default().fg(app.config.theme.fg_color());
        if task.is_done {
            row_style = row_style
                .fg(app.config.theme.muted_color())
                .add_modifier(Modifier::CROSSED_OUT);
        }
        if is_selected {
            row_style = row_style.bg(app.config.theme.selection_bg_color());
        }
        if is_renaming {
            row_style = Style::default()
                .fg(app.config.theme.success_color())
                .add_modifier(Modifier::BOLD);
        }

        let left_text = format!(" {checkbox} {title_text}");
        let meta_width: usize = meta_parts.iter().map(|s| s.content.chars().count()).sum();
        let pad = (row_rect.width as usize).saturating_sub(left_text.chars().count() + meta_width);

        let mut spans = vec![
            Span::styled(left_text, row_style),
            Span::styled(" ".repeat(pad), row_style),
        ];
        for part in meta_parts {
            let s = if is_selected {
                part.style.bg(app.config.theme.selection_bg_color())
            } else {
                part.style
            };
            spans.push(Span::styled(part.content, s));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), row_rect);
    }
    app.areas.task_rows = task_rows;

    if app.show_note && note_area.height > 0 {
        let note_text = app
            .selected_task()
            .and_then(|t| t.notes.as_deref())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or("(No note)");
        let block = Block::default()
            .title(" Note (N: full) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.config.theme.accent_color()));
        frame.render_widget(
            Paragraph::new(markdown_lines(note_text, &app.config.theme))
                .block(block)
                .wrap(Wrap { trim: true }),
            note_area,
        );
    }
}

fn format_ms(ms: u64) -> String {
    let mins = ms / 60_000;
    let h = mins / 60;
    let m = mins % 60;
    if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let nav_toggle_hint = if app.config.keybind_mode == KeybindMode::Nvim {
        "zn: sidebar"
    } else {
        "hn: sidebar"
    };
    let status_color = if app.connected {
        app.config.theme.success_color()
    } else {
        app.config.theme.danger_color()
    };

    let mut spans = Vec::new();
    if app.rename_target.is_some() {
        spans.push(Span::styled(
            " [INSERT (jk/Enter to save)] ",
            Style::default()
                .fg(Color::Black)
                .bg(app.config.theme.accent_color()),
        ));
    }

    if let Some(cid) = &app.current_task_id
        && let Some(t) = app.tasks.iter().find(|t| &t.id == cid)
    {
        let secs = app
            .timer_started_at
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0);
        spans.push(Span::styled(
            format!(" \u{f04b} {} ({:02}:{:02}) ", t.title, secs / 60, secs % 60),
            Style::default()
                .fg(Color::Black)
                .bg(app.config.theme.success_color()),
        ));
    }

    spans.push(Span::styled(
        format!(" {} ", app.status_message),
        Style::default().fg(status_color),
    ));
    spans.push(Span::styled(
        format!(" | {nav_toggle_hint} | :h help | :s settings | :c theme "),
        Style::default().fg(app.config.theme.muted_color()),
    ));

    let left_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let key_str = if app.typed_key_display.is_empty() {
        String::new()
    } else {
        format!(" {} ", app.typed_key_display)
    };
    let pad = (area.width as usize).saturating_sub(left_len + key_str.chars().count());
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
    }
    if !key_str.is_empty() {
        spans.push(Span::styled(
            key_str,
            Style::default()
                .fg(app.config.theme.warning_color())
                .add_modifier(Modifier::BOLD),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(app.config.theme.status_bg_color())),
        area,
    );
}

fn render_properties_modal(frame: &mut Frame, app: &App, area: Rect) {
    if let PropertiesMode::MoveToProject { selected } = app.properties_mode {
        let popup = centered_rect(46, (app.projects.len() as u16 + 6).min(18), area);
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .title(" Move to Project ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.config.theme.warning_color()))
            .padding(Padding::uniform(1));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let mut lines = vec![Line::from(Span::styled(
            "None (Inbox)",
            if selected == 0 {
                Style::default()
                    .fg(app.config.theme.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.config.theme.fg_color())
            },
        ))];
        for (i, p) in app.projects.iter().enumerate() {
            let is_sel = selected == i + 1;
            let style = if is_sel {
                Style::default()
                    .fg(p.display_color())
                    .bg(app.config.theme.selection_bg_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(p.display_color())
            };
            lines.push(Line::from(Span::styled(
                format!("\u{f07b} {}", p.title),
                style,
            )));
        }
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let hint = app.config.hint_keybind;
    let base = Style::default().fg(app.config.theme.fg_color());
    let hc = app.config.theme.hint_fg_color();

    let (title, lines) = match &app.properties_target {
        Some(PropertiesTarget::Project(_)) => (
            " Project Properties ",
            vec![
                Line::from(hint_spans("rename", hint, base, hc)),
                Line::from(hint_spans("delete", hint, base, hc)),
                Line::from(""),
                Line::from(Span::styled(
                    "Esc to close",
                    Style::default().fg(app.config.theme.muted_color()),
                )),
            ],
        ),
        Some(PropertiesTarget::Tag(_)) => (
            " Tag Properties ",
            vec![
                Line::from(hint_spans("rename", hint, base, hc)),
                Line::from(hint_spans("delete", hint, base, hc)),
                Line::from(""),
                Line::from(Span::styled(
                    "Esc to close",
                    Style::default().fg(app.config.theme.muted_color()),
                )),
            ],
        ),
        _ => (
            " Task Properties ",
            vec![
                Line::from(hint_spans("rename", hint, base, hc)),
                Line::from(hint_indices("schedule today", hint, &[0, 11], base, hc)),
                Line::from(hint_indices("schedule tommorow", hint, &[0, 11], base, hc)),
                Line::from(hint_spans("schedule next week", hint, base, hc)),
                Line::from(hint_spans("unschedule", hint, base, hc)),
                Line::from(hint_spans("Reschedule", hint, base, hc)),
                Line::from(hint_indices("move to top", hint, &[0, 8], base, hc)),
                Line::from(hint_indices("move to bottom", hint, &[0, 8], base, hc)),
                Line::from(hint_spans("add subtask", hint, base, hc)),
                Line::from(hint_spans("duplicate", hint, base, hc)),
                Line::from(hint_indices("move to project", hint, &[0, 8], base, hc)),
                Line::from(""),
                Line::from(Span::styled(
                    "Type underlined shortcut | Esc or jk to close",
                    Style::default().fg(app.config.theme.muted_color()),
                )),
            ],
        ),
    };

    let height = (lines.len() as u16) + 4;
    let width = 44.min(area.width);
    let popup = if let Some((ax, ay)) = app.properties_anchor {
        let x = ax.min(area.right().saturating_sub(width));
        let y = if ay + height <= area.bottom() {
            ay
        } else {
            area.bottom().saturating_sub(height)
        };
        Rect::new(x, y, width, height.min(area.height))
    } else {
        centered_rect(width, height, area)
    };
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.warning_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_help_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(76, 22, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Help / Keybinds (:h) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.accent_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let query = app.help_search.to_lowercase();
    let nvim_active = app.config.keybind_mode.is_nvim_active();
    let fg = app.config.theme.fg_color();
    let success = app.config.theme.success_color();

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "Search: ",
                Style::default().fg(app.config.theme.warning_color()),
            ),
            Span::styled(format!("{}_", app.help_search), Style::default().fg(fg)),
        ]),
        Line::from(""),
    ];

    for entry in all_keybinds() {
        for key_str in entry.display_keys(app.config.keybind_mode) {
            if !query.is_empty()
                && !key_str.to_lowercase().contains(&query)
                && !entry.description.to_lowercase().contains(&query)
            {
                continue;
            }
            let key_color = if entry.is_nvim_highlight && nvim_active {
                success
            } else {
                fg
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{key_str:<24}"),
                    Style::default().fg(key_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(entry.description, Style::default().fg(fg)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_settings_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(64, 14, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Settings (:s) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.accent_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let st = &app.settings_state;
    let accent = app.config.theme.accent_color();
    let fg = app.config.theme.fg_color();
    let field_style = |f: SettingsField| {
        if st.active_field == f {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        }
    };

    let masked_token = if st.api_token.is_empty() {
        "<empty>".to_string()
    } else if st.active_field == SettingsField::ApiToken {
        st.api_token.clone()
    } else {
        "*".repeat(st.api_token.len().min(16))
    };

    let lines = vec![
        Line::from(Span::styled(
            "CLI Settings",
            Style::default()
                .fg(app.config.theme.warning_color())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("API URL:       "),
            Span::styled(&st.api_url, field_style(SettingsField::ApiUrl)),
        ]),
        Line::from(vec![
            Span::raw("API Token:     "),
            Span::styled(masked_token, field_style(SettingsField::ApiToken)),
        ]),
        Line::from(vec![
            Span::raw("Keybind Mode:  "),
            Span::styled(
                st.keybind_mode.label(),
                field_style(SettingsField::KeybindMode),
            ),
        ]),
        Line::from(vec![
            Span::raw("Hint Keybind:  "),
            Span::styled(
                if st.hint_keybind { "true" } else { "false" },
                field_style(SettingsField::HintKeybind),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Tab/Up/Down: move | Space: toggle | Enter: save & connect | Esc: cancel",
            Style::default().fg(app.config.theme.muted_color()),
        )),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_add_task_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(72, 22, area);
    frame.render_widget(Clear, popup);

    let title_label = if app.add_task_state.parent_id.is_some() {
        " Add Subtask "
    } else {
        " Add Task (a) "
    };

    let block = Block::default()
        .title(title_label)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.success_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let st = &app.add_task_state;
    let success = app.config.theme.success_color();
    let fg = app.config.theme.fg_color();
    let muted = app.config.theme.muted_color();

    let field_style = |f: AddTaskField| {
        if st.active_field == f {
            Style::default().fg(success).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        }
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Title:    ", Style::default().fg(muted)),
            Span::styled(
                if st.title.is_empty() && st.active_field != AddTaskField::Title {
                    "<type title, #tag, @time>"
                } else {
                    &st.title
                },
                field_style(AddTaskField::Title),
            ),
            if st.active_field == AddTaskField::Title {
                Span::styled("_", Style::default().fg(success))
            } else {
                Span::raw("")
            },
        ]),
        Line::from(vec![
            Span::styled("Note:     ", Style::default().fg(muted)),
            Span::styled(&st.note, field_style(AddTaskField::Note)),
            if st.active_field == AddTaskField::Note {
                Span::styled("_", Style::default().fg(success))
            } else {
                Span::raw("")
            },
        ]),
        Line::from(vec![
            Span::styled("Position: ", Style::default().fg(muted)),
            Span::styled(
                if st.add_to_top { "Top" } else { "Bottom" },
                field_style(AddTaskField::Position),
            ),
            Span::styled("   Repeat: ", Style::default().fg(muted)),
            Span::styled(st.repeat.label(), field_style(AddTaskField::Repeat)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Project:", field_style(AddTaskField::Project))),
    ];

    let mut proj_spans = vec![Span::raw("  ")];
    let inbox_sel = st.project_idx == 0;
    proj_spans.push(Span::styled(
        if inbox_sel { "[Inbox] " } else { " Inbox  " },
        if inbox_sel {
            Style::default().fg(success).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted)
        },
    ));
    for (i, p) in app.projects.iter().enumerate() {
        let sel = st.project_idx == i + 1;
        let label = if sel {
            format!("[\u{f07b} {}] ", p.title)
        } else {
            format!(" \u{f07b} {}  ", p.title)
        };
        proj_spans.push(Span::styled(
            label,
            Style::default().fg(p.display_color()).add_modifier(if sel {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ));
    }
    lines.push(Line::from(proj_spans));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Tags:",
        field_style(AddTaskField::Tags),
    )));
    if app.tags.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (No tags available)",
            Style::default().fg(muted),
        )));
    } else {
        for (i, t) in app.tags.iter().enumerate().take(4) {
            let checked = st.selected_tag_ids.contains(&t.id);
            let cursor = st.active_field == AddTaskField::Tags && i == st.tag_cursor;
            let mark = if checked { "\u{f046}" } else { "\u{f096}" };
            let prefix = if cursor { "  > " } else { "    " };
            lines.push(Line::from(Span::styled(
                format!("{prefix}{mark} \u{f292} {}", t.title),
                Style::default()
                    .fg(t.display_color())
                    .add_modifier(if cursor {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Tab: field | Left/Right/Space: select | Enter: create | Esc: cancel",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);

    if let Some(comp) = &st.completion {
        let comp_h = (comp.items.len() as u16 + 2).min(8);
        let comp_w = 28.min(inner.width);
        let comp_rect = Rect::new(inner.x + 10, inner.y + 1, comp_w, comp_h);
        frame.render_widget(Clear, comp_rect);
        let comp_title = match comp.kind {
            CompletionKind::Tag => " Tags (#) ",
            CompletionKind::Time => " Time (@) ",
        };
        let comp_block = Block::default()
            .title(comp_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.config.theme.accent_color()));
        let comp_inner = comp_block.inner(comp_rect);
        frame.render_widget(comp_block, comp_rect);

        let comp_lines: Vec<Line> = comp
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let s = if i == comp.selected {
                    Style::default()
                        .fg(success)
                        .bg(app.config.theme.selection_bg_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(fg)
                };
                Line::from(Span::styled(item.clone(), s))
            })
            .collect();
        frame.render_widget(Paragraph::new(comp_lines), comp_inner);
    }
}

fn render_theme_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(ed) = &app.theme_editor else {
        return;
    };
    let popup = centered_rect(68, 21, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Colorscheme / Theme (:c) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.border_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let accent = app.config.theme.accent_color();
    let fg = app.config.theme.fg_color();
    let muted = app.config.theme.muted_color();

    let mut lines = Vec::new();
    for (i, label) in ThemeEditorState::FIELD_LABELS.iter().enumerate() {
        let val = ed.field_value(i);
        let is_sel = ed.field_idx == i;
        let val_style = if is_sel {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        };
        let swatch = if i > 0 {
            if let Some(c) = parse_hex_color(val) {
                Span::styled(" ██", Style::default().fg(c))
            } else {
                Span::styled(" ??", Style::default().fg(muted))
            }
        } else {
            Span::raw("  (Left/Right preset)")
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{label:<18}: "), Style::default().fg(muted)),
            Span::styled(val, val_style),
            swatch,
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Ctrl+S: save template | Enter: apply | Esc: revert",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_full_note_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(
        area.width.saturating_sub(8).max(40),
        area.height.saturating_sub(6).max(12),
        area,
    );
    frame.render_widget(Clear, popup);

    let title = app
        .selected_task()
        .map(|t| format!(" Note: {} (Esc / N to close) ", t.title))
        .unwrap_or_else(|| " Note ".to_string());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.accent_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let note_text = app
        .selected_task()
        .and_then(|t| t.notes.as_deref())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or("(No note)");

    frame.render_widget(
        Paragraph::new(markdown_lines(note_text, &app.config.theme)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_add_entity_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(54, 11, area);
    frame.render_widget(Clear, popup);

    let st = &app.add_entity_state;
    let title = match st.kind {
        AddEntityKind::Project => " Add Project (a) ",
        AddEntityKind::Tag => " Add Tag (a) ",
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.success_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let success = app.config.theme.success_color();
    let fg = app.config.theme.fg_color();
    let muted = app.config.theme.muted_color();

    let name_style = if st.active_field == AddEntityField::Name {
        Style::default().fg(success).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg)
    };
    let color_style = if st.active_field == AddEntityField::Color {
        Style::default().fg(success).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg)
    };
    let swatch = parse_hex_color(&st.color)
        .map(|c| Span::styled(" ██", Style::default().fg(c)))
        .unwrap_or_else(|| Span::styled(" ??", Style::default().fg(muted)));

    let lines = vec![
        Line::from(vec![
            Span::styled("Name:  ", Style::default().fg(muted)),
            Span::styled(&st.title, name_style),
        ]),
        Line::from(vec![
            Span::styled("Color: ", Style::default().fg(muted)),
            Span::styled(&st.color, color_style),
            swatch,
            Span::styled("  (Left/Right preset)", Style::default().fg(muted)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Tab: switch | Enter: create | Esc: cancel",
            Style::default().fg(muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn markdown_lines(text: &str, theme: &crate::theme::ThemeConfig) -> Vec<Line<'static>> {
    let fg = theme.fg_color();
    let accent = theme.accent_color();
    let warning = theme.warning_color();
    let success = theme.success_color();

    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(h) = trimmed.strip_prefix("### ") {
                Line::from(Span::styled(
                    h.to_string(),
                    Style::default().fg(success).add_modifier(Modifier::BOLD),
                ))
            } else if let Some(h) = trimmed.strip_prefix("## ") {
                Line::from(Span::styled(
                    h.to_string(),
                    Style::default().fg(warning).add_modifier(Modifier::BOLD),
                ))
            } else if let Some(h) = trimmed.strip_prefix("# ") {
                Line::from(Span::styled(
                    h.to_string(),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ))
            } else if let Some(b) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                let mut spans = vec![Span::styled("  • ", Style::default().fg(accent))];
                spans.extend(parse_inline_md(b, fg, warning, accent, success));
                Line::from(spans)
            } else {
                Line::from(parse_inline_md(line, fg, warning, accent, success))
            }
        })
        .collect()
}

fn parse_inline_md(
    input: &str,
    fg: Color,
    bold_fg: Color,
    italic_fg: Color,
    code_fg: Color,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = input;

    while !rest.is_empty() {
        let next_bold = rest.find("**");
        let next_code = rest.find('`');
        let next_italic = rest.find('*');

        let earliest = [
            next_bold.map(|i| (i, 0u8)),
            next_code.map(|i| (i, 1u8)),
            next_italic.map(|i| (i, 2u8)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(i, kind)| (*i, *kind));

        let Some((idx, kind)) = earliest else {
            spans.push(Span::styled(rest.to_string(), Style::default().fg(fg)));
            break;
        };

        if idx > 0 {
            spans.push(Span::styled(
                rest[..idx].to_string(),
                Style::default().fg(fg),
            ));
        }

        match kind {
            0 => {
                let after = &rest[idx + 2..];
                if let Some(end) = after.find("**") {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default().fg(bold_fg).add_modifier(Modifier::BOLD),
                    ));
                    rest = &after[end + 2..];
                } else {
                    spans.push(Span::styled(
                        rest[idx..].to_string(),
                        Style::default().fg(fg),
                    ));
                    break;
                }
            }
            1 => {
                let after = &rest[idx + 1..];
                if let Some(end) = after.find('`') {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default().fg(code_fg).add_modifier(Modifier::BOLD),
                    ));
                    rest = &after[end + 1..];
                } else {
                    spans.push(Span::styled(
                        rest[idx..].to_string(),
                        Style::default().fg(fg),
                    ));
                    break;
                }
            }
            _ => {
                let after = &rest[idx + 1..];
                if let Some(end) = after.find('*') {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default()
                            .fg(italic_fg)
                            .add_modifier(Modifier::ITALIC),
                    ));
                    rest = &after[end + 1..];
                } else {
                    spans.push(Span::styled(
                        rest[idx..].to_string(),
                        Style::default().fg(fg),
                    ));
                    break;
                }
            }
        }
    }

    spans
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tokio::sync::mpsc;

    #[test]
    fn hint_spans_underlines_first_letter_of_each_word() {
        let spans = hint_spans("abc def", true, Style::default(), Color::Yellow);
        assert_eq!(spans[0].content, "a");
        assert!(spans[0].style.add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(spans[1].content, "bc ");
        assert_eq!(spans[2].content, "d");
        assert!(spans[2].style.add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(spans[3].content, "ef");
    }

    #[test]
    fn renders_main_and_modals_without_panic() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), tx);
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        for modal in [
            ActiveModal::None,
            ActiveModal::Properties,
            ActiveModal::Help,
            ActiveModal::Settings,
            ActiveModal::AddTask,
            ActiveModal::Theme,
            ActiveModal::FullNote,
            ActiveModal::AddEntity,
        ] {
            if modal == ActiveModal::Theme {
                app.theme_editor = Some(ThemeEditorState::new(
                    &app.config.theme,
                    &app.config.saved_themes,
                ));
            }
            app.active_modal = modal;
            terminal.draw(|f| render(f, &mut app)).unwrap();
        }
    }
}
