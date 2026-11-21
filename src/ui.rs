use crate::app::{
    ActiveModal, AddEntityField, AddEntityKind, AddTaskField, App, CompletionKind, ConnectionState,
    FocusMode, FocusSection, HistoryTab, HistoryTimeframe, NavFilter, NavItemKind, PomodoroPhase,
    PropertiesMode, PropertiesTarget, SettingsField, ThemeEditorState, ThemePickerFocus,
};
use crate::config::{KeybindMode, SyncMode};
use crate::keybinds::all_keybinds;
use crate::models::parse_hex_color;
use crate::theme::map_coords_to_rgb;
use chrono::{Datelike, Local, NaiveDate};
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

    let local_h = if app.local_notice_until.is_some() {
        1
    } else {
        0
    };
    let update_h = if app.available_update.is_some() { 1 } else { 0 };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(local_h),
            Constraint::Length(update_h),
            Constraint::Length(1),
        ])
        .split(root);

    let body = outer[0];
    let local_bar = outer[1];
    let update_bar = outer[2];
    let status_bar = outer[3];

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

    if local_h > 0 {
        render_local_notice_banner(frame, app, local_bar);
    }
    if update_h > 0 {
        render_update_banner(frame, app, update_bar);
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
        ActiveModal::History => render_history_modal(frame, app, root),
        ActiveModal::Counters => render_counters_modal(frame, app, root),
        ActiveModal::Pomodoro => render_pomodoro_modal(frame, app, root),
        ActiveModal::Reschedule => render_reschedule_modal(frame, app, root),
        ActiveModal::DailySummary => render_daily_summary_modal(frame, app, root),
        ActiveModal::Changelog => render_changelog_modal(frame, app, root),
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
    let search_h = if app.nav_search_active || !app.nav_search_query.is_empty() {
        1
    } else {
        0
    };
    let bottom_h = bottom_rows.len() as u16 + 1;
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(search_h),
            Constraint::Min(3),
            Constraint::Length(bottom_h),
        ])
        .split(area);

    if search_h > 0 {
        let cursor = if app.nav_search_active { "_" } else { "" };
        let bar = Line::from(vec![
            Span::styled(
                " /",
                Style::default()
                    .fg(app.config.theme.accent_color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}{} ", app.nav_search_query, cursor),
                Style::default().fg(app.config.theme.fg_color()),
            ),
        ]);
        frame.render_widget(Paragraph::new(bar), sections[0]);
    }

    let top_area = sections[1];
    let bottom_area = sections[2];

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
    let is_active = match &row.kind {
        NavItemKind::Filter(f) => *f == app.active_filter,
        NavItemKind::History => app.active_modal == ActiveModal::History,
        NavItemKind::Counters => app.active_modal == ActiveModal::Counters,
        NavItemKind::Help => app.active_modal == ActiveModal::Help,
        NavItemKind::Settings => app.active_modal == ActiveModal::Settings,
        NavItemKind::ToggleProjects | NavItemKind::ToggleTags => false,
    };

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
    if is_active {
        style = style
            .bg(app.config.theme.selection_bg_color())
            .fg(app.config.theme.accent_color())
            .add_modifier(Modifier::BOLD);
    }
    if app.nav_row_matches(&row.label) {
        style = style
            .fg(app.config.theme.warning_color())
            .add_modifier(Modifier::BOLD);
    }
    if is_selected {
        let sel_fg = if is_active {
            app.config.theme.accent_color()
        } else {
            app.config.theme.fg_color()
        };
        style = style
            .bg(app.config.theme.selection_bg_color())
            .fg(sel_fg)
            .add_modifier(Modifier::BOLD);
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
    let width = list_area.width.max(1) as usize;
    let word_wrap = app.config.word_wrap;
    let rows = app.visible_task_rows();

    struct PreparedTaskRow {
        idx: usize,
        lines: Vec<Line<'static>>,
        is_renaming: bool,
    }

    let mut prepared_rows = Vec::with_capacity(rows.len());
    for (idx, (task, is_subtask)) in rows.iter().enumerate() {
        let is_selected = app.focus == FocusSection::Main && app.task_selected == idx;
        let is_multi_selected = app.visual_mode && app.selected_task_ids.contains(&task.id);
        let is_renaming = matches!(
            &app.rename_target,
            Some(crate::app::RenameTarget::Task(id)) if id == &task.id
        );

        let checkbox = if is_multi_selected {
            if task.is_done {
                "\u{25aa}\u{f046}"
            } else {
                "\u{25aa}\u{f096}"
            }
        } else if task.is_done {
            "\u{f046}"
        } else {
            "\u{f096}"
        };

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
            let prefix = format!("    {checkbox} ");
            let lines = if word_wrap {
                build_wrapped_task_lines(&prefix, &title_text, "      ", &[], width, sub_style, None)
            } else {
                let line = format!("{prefix}{title_text}");
                let pad = width.saturating_sub(line.chars().count());
                vec![Line::from(Span::styled(
                    format!("{line}{}", " ".repeat(pad)),
                    sub_style,
                ))]
            };
            prepared_rows.push(PreparedTaskRow {
                idx,
                lines,
                is_renaming,
            });
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
        if let Some(dl) = &task.deadline_day {
            let dl_color = if dl.as_str() < today_str.as_str() {
                app.config.theme.danger_color()
            } else if dl.as_str() == today_str.as_str() {
                app.config.theme.warning_color()
            } else {
                app.config.theme.accent_color()
            };
            meta_parts.push(Span::styled(
                format!(" \u{f06a} {dl} "),
                Style::default().fg(dl_color),
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
        if is_selected || is_multi_selected {
            row_style = row_style.bg(app.config.theme.selection_bg_color());
        }
        if is_multi_selected {
            row_style = row_style.fg(app.config.theme.accent_color());
        }
        if is_renaming {
            row_style = Style::default()
                .fg(app.config.theme.success_color())
                .add_modifier(Modifier::BOLD);
        }

        let selected_bg = (is_selected || is_multi_selected)
            .then_some(app.config.theme.selection_bg_color());
        let prefix = format!(" {checkbox} ");
        let lines = if word_wrap {
            build_wrapped_task_lines(
                &prefix,
                &title_text,
                "    ",
                &meta_parts,
                width,
                row_style,
                selected_bg,
            )
        } else {
            let left_text = format!("{prefix}{title_text}");
            let meta_width: usize = meta_parts.iter().map(|s| s.content.chars().count()).sum();
            let pad = width.saturating_sub(left_text.chars().count() + meta_width);
            let mut spans = vec![
                Span::styled(left_text, row_style),
                Span::styled(" ".repeat(pad), row_style),
            ];
            for part in meta_parts {
                let s = if let Some(bg) = selected_bg {
                    part.style.bg(bg)
                } else {
                    part.style
                };
                spans.push(Span::styled(part.content, s));
            }
            vec![Line::from(spans)]
        };
        prepared_rows.push(PreparedTaskRow {
            idx,
            lines,
            is_renaming,
        });
    }

    if visible_h > 0 && !prepared_rows.is_empty() {
        let sel = app.task_selected.min(prepared_rows.len() - 1);
        if sel < app.task_scroll {
            app.task_scroll = sel;
        }
        while app.task_scroll < sel {
            let used: usize = prepared_rows[app.task_scroll..=sel]
                .iter()
                .map(|r| r.lines.len())
                .sum();
            if used <= visible_h {
                break;
            }
            app.task_scroll += 1;
        }
    }

    let mut task_rows = Vec::new();
    let mut renaming_row_y: Option<u16> = None;
    let mut cur_y = list_area.y;

    for row in prepared_rows.into_iter().skip(app.task_scroll) {
        if cur_y >= list_area.bottom() {
            break;
        }
        let avail_h = (list_area.bottom() - cur_y) as usize;
        let render_h = row.lines.len().min(avail_h) as u16;
        let row_rect = Rect::new(list_area.x, cur_y, list_area.width, render_h);
        task_rows.push((row_rect, row.idx));
        if row.is_renaming {
            renaming_row_y = Some(cur_y + render_h.saturating_sub(1));
        }
        let visible_lines: Vec<Line<'static>> =
            row.lines.into_iter().take(render_h as usize).collect();
        frame.render_widget(Paragraph::new(visible_lines), row_rect);
        cur_y += render_h;
    }
    app.areas.task_rows = task_rows;

    if let (Some(comp), Some(ry)) = (&app.rename_completion, renaming_row_y) {
        let comp_h = (comp.items.len() as u16 + 2).min(8);
        let comp_w = 28.min(list_area.width);
        let comp_x = (list_area.x + 6).min(list_area.right().saturating_sub(comp_w));
        let comp_y = if ry + 1 + comp_h <= list_area.bottom() {
            ry + 1
        } else {
            ry.saturating_sub(comp_h)
        };
        let comp_rect = Rect::new(comp_x, comp_y, comp_w, comp_h);
        render_completion_popup(frame, comp, comp_rect, &app.config.theme);
    }

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

fn wrap_text_lines(text: &str, first_max: usize, cont_max: usize) -> Vec<String> {
    let first_max = first_max.max(1);
    let cont_max = cont_max.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_limit = first_max;

    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if cur.is_empty() {
            if word_len <= cur_limit {
                cur.push_str(word);
            } else {
                let mut chunk = String::new();
                for ch in word.chars() {
                    if chunk.chars().count() >= cur_limit {
                        lines.push(chunk);
                        chunk = String::new();
                        cur_limit = cont_max;
                    }
                    chunk.push(ch);
                }
                cur = chunk;
            }
        } else if cur.chars().count() + 1 + word_len <= cur_limit {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(cur);
            cur = String::new();
            cur_limit = cont_max;
            if word_len <= cur_limit {
                cur.push_str(word);
            } else {
                let mut chunk = String::new();
                for ch in word.chars() {
                    if chunk.chars().count() >= cur_limit {
                        lines.push(chunk);
                        chunk = String::new();
                    }
                    chunk.push(ch);
                }
                cur = chunk;
            }
        }
    }

    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

fn build_wrapped_task_lines(
    prefix: &str,
    title: &str,
    cont_indent: &str,
    meta_parts: &[Span<'static>],
    width: usize,
    row_style: Style,
    selected_bg: Option<Color>,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    let prefix_len = prefix.chars().count();
    let cont_len = cont_indent.chars().count();
    let first_max = width.saturating_sub(prefix_len).max(1);
    let cont_max = width.saturating_sub(cont_len).max(1);
    let title_lines = wrap_text_lines(title, first_max, cont_max);

    let apply_bg = |s: Style| -> Style {
        if let Some(bg) = selected_bg {
            s.bg(bg)
        } else {
            s
        }
    };

    let meta_width: usize = meta_parts.iter().map(|s| s.content.chars().count()).sum();
    let mut out: Vec<Line<'static>> = Vec::with_capacity(title_lines.len() + 1);
    let last_idx = title_lines.len() - 1;

    for (i, tline) in title_lines.into_iter().enumerate() {
        let lead = if i == 0 { prefix } else { cont_indent };
        let lead_len = if i == 0 { prefix_len } else { cont_len };
        let used = lead_len + tline.chars().count();

        if i == last_idx && !meta_parts.is_empty() && used + meta_width <= width {
            let pad = width - used - meta_width;
            let mut spans = vec![
                Span::styled(format!("{lead}{tline}"), row_style),
                Span::styled(" ".repeat(pad), row_style),
            ];
            for part in meta_parts {
                spans.push(Span::styled(part.content.clone(), apply_bg(part.style)));
            }
            out.push(Line::from(spans));
        } else {
            let pad = width.saturating_sub(used);
            out.push(Line::from(vec![
                Span::styled(format!("{lead}{tline}"), row_style),
                Span::styled(" ".repeat(pad), row_style),
            ]));
        }
    }

    // If meta_parts didn't fit on the last title line, wrap them onto continuation lines.
    if !meta_parts.is_empty() {
        let last_used_fits = {
            // Check whether meta_parts were already placed on the last line
            out.last().is_some_and(|l| l.spans.len() > 2)
        };
        if !last_used_fits {
            let mut cur_spans = vec![Span::styled(cont_indent.to_string(), row_style)];
            let mut cur_len = cont_len;
            for part in meta_parts {
                let plen = part.content.chars().count();
                if cur_len > cont_len && cur_len + plen > width {
                    let pad = width.saturating_sub(cur_len);
                    if pad > 0 {
                        cur_spans.push(Span::styled(" ".repeat(pad), row_style));
                    }
                    out.push(Line::from(cur_spans));
                    cur_spans = vec![Span::styled(cont_indent.to_string(), row_style)];
                    cur_len = cont_len;
                }
                cur_spans.push(Span::styled(part.content.clone(), apply_bg(part.style)));
                cur_len += plen;
            }
            let pad = width.saturating_sub(cur_len);
            if pad > 0 {
                cur_spans.push(Span::styled(" ".repeat(pad), row_style));
            }
            out.push(Line::from(cur_spans));
        }
    }

    out
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let nav_toggle_hint = if app.config.keybind_mode == KeybindMode::Nvim {
        "zn: sidebar"
    } else {
        "hn: sidebar"
    };
    let (conn_label, conn_color) = match app.conn_state {
        ConnectionState::Local => ("\u{25cf} Local", app.config.theme.accent_color()),
        ConnectionState::Online => ("\u{25cf} Online", app.config.theme.success_color()),
        ConnectionState::Syncing => ("\u{25cf} Sync...", Color::Rgb(96, 165, 250)),
        ConnectionState::Offline => ("\u{25cf} Offline", app.config.theme.danger_color()),
    };

    let mut spans = Vec::new();
    if app.visual_mode {
        spans.push(Span::styled(
            format!(" [VISUAL ({} selected)] ", app.selected_task_ids.len()),
            Style::default()
                .fg(Color::Black)
                .bg(app.config.theme.accent_color())
                .add_modifier(Modifier::BOLD),
        ));
    }

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

    if app.pomodoro.running || app.pomodoro.reminder.is_some() {
        let display_secs = if app.pomodoro.mode == FocusMode::Flowtime
            && app.pomodoro.phase == PomodoroPhase::Focus
        {
            app.pomodoro.elapsed_secs
        } else {
            app.pomodoro.remaining_secs
        };
        let m = display_secs / 60;
        let s = display_secs % 60;
        spans.push(Span::styled(
            format!(
                " \u{f017} {} {} {:02}:{:02} ",
                app.pomodoro.mode.label(),
                app.pomodoro.phase.label(),
                m,
                s
            ),
            Style::default()
                .fg(Color::Black)
                .bg(app.config.theme.warning_color()),
        ));
    }

    spans.push(Span::styled(
        format!(" {conn_label} "),
        Style::default().fg(conn_color).add_modifier(Modifier::BOLD),
    ));
    if !app.offline_cache.pending_ops.is_empty() {
        spans.push(Span::styled(
            format!("[Q:{}] ", app.offline_cache.pending_ops.len()),
            Style::default()
                .fg(app.config.theme.warning_color())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if !app.status_message.is_empty() {
        spans.push(Span::styled(
            format!("| {} ", app.status_message),
            Style::default().fg(app.config.theme.fg_color()),
        ));
    }
    spans.push(Span::styled(
        format!("| {nav_toggle_hint} | :h help | :s settings | :c theme "),
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
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
                Line::from(hint_spans("color", hint, base, hc)),
                Line::from(hint_spans("up (move)", hint, base, hc)),
                Line::from(hint_indices("next/down", hint, &[0], base, hc)),
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
                Line::from(hint_spans("color", hint, base, hc)),
                Line::from(hint_spans("up (move)", hint, base, hc)),
                Line::from(hint_indices("next/down", hint, &[0], base, hc)),
                Line::from(hint_spans("delete", hint, base, hc)),
                Line::from(""),
                Line::from(Span::styled(
                    "Esc to close",
                    Style::default().fg(app.config.theme.muted_color()),
                )),
            ],
        ),
        Some(PropertiesTarget::Batch(ids)) => (
            " Batch Properties ",
            vec![
                Line::from(Span::styled(
                    format!("{} task(s) selected", ids.len()),
                    Style::default()
                        .fg(app.config.theme.accent_color())
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(hint_spans("complete / toggle", hint, base, hc)),
                Line::from(hint_indices("schedule today", hint, &[0, 11], base, hc)),
                Line::from(hint_indices("schedule tommorow", hint, &[0, 11], base, hc)),
                Line::from(hint_spans("schedule next week", hint, base, hc)),
                Line::from(hint_spans("unschedule", hint, base, hc)),
                Line::from(hint_indices("move to project", hint, &[0, 8], base, hc)),
                Line::from(hint_spans("delete", hint, base, hc)),
                Line::from(""),
                Line::from(Span::styled(
                    "Type underlined shortcut | Esc or jk to close",
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
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_help_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(76, 24, area);
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
    let accent = app.config.theme.accent_color();
    let muted = app.config.theme.muted_color();

    let mut body_lines: Vec<Line<'static>> = Vec::new();
    let mut current_section = "";

    for entry in all_keybinds() {
        for key_str in entry.display_keys(app.config.keybind_mode) {
            if !query.is_empty()
                && !key_str.to_lowercase().contains(&query)
                && !entry.description.to_lowercase().contains(&query)
                && !entry.section.to_lowercase().contains(&query)
            {
                continue;
            }
            if entry.section != current_section {
                if !body_lines.is_empty() {
                    body_lines.push(Line::from(""));
                }
                current_section = entry.section;
                body_lines.push(Line::from(Span::styled(
                    format!("--- {current_section} ---"),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                )));
            }
            let key_color = if entry.is_nvim_highlight && nvim_active {
                success
            } else {
                fg
            };
            body_lines.push(Line::from(vec![
                Span::styled(
                    format!("{key_str:<24}"),
                    Style::default().fg(key_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(entry.description, Style::default().fg(fg)),
            ]));
        }
    }

    let max_body = (inner.height as usize).saturating_sub(4).max(1);
    let max_scroll = body_lines.len().saturating_sub(max_body);
    let skip = app.help_scroll.min(max_scroll);

    let search_cursor = if app.help_search_focused { "_" } else { "" };
    let search_label_color = if app.help_search_focused {
        success
    } else {
        app.config.theme.warning_color()
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                if app.help_search_focused {
                    "Search (typing): "
                } else {
                    "Search (/): "
                },
                Style::default().fg(search_label_color),
            ),
            Span::styled(
                format!("{}{search_cursor}", app.help_search),
                Style::default().fg(fg),
            ),
        ]),
        Line::from(""),
    ];

    lines.extend(body_lines.into_iter().skip(skip).take(max_body));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if app.help_search_focused {
            "Type to filter | Enter/Esc: done searching"
        } else {
            "j/k/g/G: scroll | /: search | Esc/q: close"
        },
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_settings_modal(frame: &mut Frame, app: &App, area: Rect) {
    let st = &app.settings_state;
    let is_ss = st.sync_mode == SyncMode::SuperSync;
    let height = if is_ss { 23 } else { 21 };
    let popup = centered_rect(76, height, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Settings (:s) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.accent_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let accent = app.config.theme.accent_color();
    let fg = app.config.theme.fg_color();
    let field_style = |f: SettingsField| {
        if st.active_field == f {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        }
    };

    let mask_secret = |val: &str, f: SettingsField| -> String {
        if val.is_empty() {
            "<empty>".to_string()
        } else if st.active_field == f {
            val.to_string()
        } else {
            "*".repeat(val.len().min(16))
        }
    };

    let mut lines = vec![
        Line::from(Span::styled(
            "CLI Settings",
            Style::default()
                .fg(app.config.theme.warning_color())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("Sync Mode:     "),
            Span::styled(
                format!("{} (Space to switch)", st.sync_mode.label()),
                field_style(SettingsField::SyncMode),
            ),
        ]),
    ];

    if is_ss {
        lines.push(Line::from(vec![
            Span::raw("SS URL:        "),
            Span::styled(&st.supersync_url, field_style(SettingsField::SuperSyncUrl)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("SS Token:      "),
            Span::styled(
                mask_secret(&st.supersync_token, SettingsField::SuperSyncToken),
                field_style(SettingsField::SuperSyncToken),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("SS Password:   "),
            Span::styled(
                mask_secret(
                    &st.supersync_encrypt_key,
                    SettingsField::SuperSyncEncryptKey,
                ),
                field_style(SettingsField::SuperSyncEncryptKey),
            ),
        ]));
        let browser_hint = st
            .browser_login_status
            .as_deref()
            .unwrap_or("Press Space/Enter to authorize via browser");
        lines.push(Line::from(vec![
            Span::raw("Browser Login: "),
            Span::styled(
                format!("[ Connect via Browser ] {browser_hint}"),
                field_style(SettingsField::SuperSyncBrowserConnect),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::raw("API URL:       "),
            Span::styled(&st.api_url, field_style(SettingsField::ApiUrl)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("API Token:     "),
            Span::styled(
                mask_secret(&st.api_token, SettingsField::ApiToken),
                field_style(SettingsField::ApiToken),
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::raw("Keybind Mode:  "),
        Span::styled(
            st.keybind_mode.label(),
            field_style(SettingsField::KeybindMode),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("Hint Keybind:  "),
        Span::styled(
            if st.hint_keybind { "true" } else { "false" },
            field_style(SettingsField::HintKeybind),
        ),
    ]));
    if !is_ss {
        lines.push(Line::from(vec![
            Span::raw("Bridge Port:   "),
            Span::styled(
                format!("{} (0 = off)", st.bridge_port),
                field_style(SettingsField::BridgePort),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::raw("Desktop Notif: "),
        Span::styled(
            if st.desktop_notifications {
                "true"
            } else {
                "false"
            },
            field_style(SettingsField::DesktopNotifications),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("Sound Alerts:  "),
        Span::styled(
            if st.sound_alerts { "true" } else { "false" },
            field_style(SettingsField::SoundAlerts),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("Word Wrap:     "),
        Span::styled(
            if st.word_wrap { "true" } else { "false" },
            field_style(SettingsField::WordWrap),
        ),
    ]));
    if !is_ss {
        lines.push(Line::from(vec![
            Span::raw("SP Plugin:     "),
            if app.bridge.is_plugin_active() {
                Span::styled(
                    "\u{25cf} Active",
                    Style::default().fg(app.config.theme.success_color()),
                )
            } else {
                Span::styled(
                    "\u{25cb} Waiting (upload plugin/sp-tui-bridge.zip in SP)",
                    Style::default().fg(app.config.theme.muted_color()),
                )
            },
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if is_ss {
            "SuperSync talks directly to the SuperSync server using E2EE (no desktop SP needed)."
        } else {
            "Note: Without SP installed, spt runs in Local mode (no sync). Or switch Sync Mode -> SuperSync."
        },
        Style::default().fg(app.config.theme.accent_color()),
    )));
    lines.push(Line::from(Span::styled(
        "Tab/Up/Down: move | Space: toggle | Enter: save & connect | Esc: cancel",
        Style::default().fg(app.config.theme.muted_color()),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_add_task_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(74, 24, area);
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
                    "<+proj #tag t1h5m3s @due !deadline>"
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
        "Tip: Use +project, #tag, t1h5m3s, @due, !deadline directly in Title for quick entry.",
        Style::default().fg(app.config.theme.warning_color()),
    )));
    lines.push(Line::from(Span::styled(
        "Tab: field | Left/Right/Space: select | Enter: create | Esc: cancel",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);

    if let Some(comp) = &st.completion {
        let comp_h = (comp.items.len() as u16 + 2).min(8);
        let comp_w = 28.min(inner.width);
        let comp_rect = Rect::new(inner.x + 10, inner.y + 1, comp_w, comp_h);
        render_completion_popup(frame, comp, comp_rect, &app.config.theme);
    }
}

fn render_completion_popup(
    frame: &mut Frame,
    comp: &crate::app::CompletionPopup,
    comp_rect: Rect,
    theme: &crate::theme::ThemeConfig,
) {
    frame.render_widget(Clear, comp_rect);
    let comp_title = match comp.kind {
        CompletionKind::Tag => " Tags (#) ",
        CompletionKind::Project => " Projects (+) ",
        CompletionKind::Time => " Time (t) ",
        CompletionKind::Due => " Due (@) ",
        CompletionKind::Deadline => " Deadline (!) ",
    };
    let comp_block = Block::default()
        .title(comp_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_color()));
    let comp_inner = comp_block.inner(comp_rect);
    frame.render_widget(comp_block, comp_rect);

    let success = theme.success_color();
    let fg = theme.fg_color();
    let sel_bg = theme.selection_bg_color();

    let comp_lines: Vec<Line> = comp
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let s = if i == comp.selected {
                Style::default()
                    .fg(success)
                    .bg(sel_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(fg)
            };
            Line::from(Span::styled(item.clone(), s))
        })
        .collect();
    frame.render_widget(Paragraph::new(comp_lines), comp_inner);
}

fn render_theme_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(ed) = &app.theme_editor else {
        return;
    };
    let popup = centered_rect(84.min(area.width), 22.min(area.height), area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Colorscheme & Color Picker (:c) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.border_color()))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(inner);

    let accent = app.config.theme.accent_color();
    let fg = app.config.theme.fg_color();
    let muted = app.config.theme.muted_color();
    let warning = app.config.theme.warning_color();

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
            Span::raw("  (◀/▶)")
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{label:<16}: "), Style::default().fg(muted)),
            Span::styled(val, val_style),
            swatch,
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "p/▶: Picker | Ctrl+S: Save | Enter: Apply | Esc: Revert",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines), cols[0]);

    // Right pane: 2D RGB Map + 3 stacked R/G/B inputs + HEX code
    let picker_block = Block::default()
        .title(if ed.picker_focus == ThemePickerFocus::FieldList {
            " Color Picker (press p) "
        } else {
            " Color Picker [ACTIVE] "
        })
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(if ed.picker_focus == ThemePickerFocus::FieldList {
                app.config.theme.border_color()
            } else {
                accent
            }),
        )
        .padding(Padding::horizontal(1));
    let p_inner = picker_block.inner(cols[1]);
    frame.render_widget(picker_block, cols[1]);

    if ed.field_idx == 0 {
        frame.render_widget(
            Paragraph::new("Select a color field (rows 2..13) to use the 2D RGB picker.")
                .style(Style::default().fg(muted))
                .wrap(Wrap { trim: true }),
            p_inner,
        );
        return;
    }

    let mut p_lines: Vec<Line<'static>> = Vec::new();
    for y in 0..ThemeEditorState::MAP_H {
        let mut row_spans = Vec::new();
        for x in 0..ThemeEditorState::MAP_W {
            let (r, g, b) =
                map_coords_to_rgb(x, y, ThemeEditorState::MAP_W, ThemeEditorState::MAP_H);
            let is_cursor = x == ed.map_x && y == ed.map_y;
            if is_cursor {
                let inv = if (r as u16 + g as u16 + b as u16) > 380 {
                    Color::Black
                } else {
                    Color::White
                };
                row_spans.push(Span::styled(
                    "+",
                    Style::default()
                        .fg(inv)
                        .bg(Color::Rgb(r, g, b))
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                row_spans.push(Span::styled("█", Style::default().fg(Color::Rgb(r, g, b))));
            }
        }
        p_lines.push(Line::from(row_spans));
    }

    p_lines.push(Line::from(""));

    let channel_line = |label: &str, val: u8, text: &str, focus: ThemePickerFocus, color: Color| {
        let active = ed.picker_focus == focus;
        let bars = (val as usize * 12) / 255;
        let bar_str = format!("{}{}", "█".repeat(bars), "░".repeat(12 - bars));
        Line::from(vec![
            Span::styled(
                format!("{label}: "),
                Style::default()
                    .fg(if active { warning } else { muted })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(bar_str, Style::default().fg(color)),
            Span::styled(
                format!(" {text:>3}"),
                Style::default()
                    .fg(if active { accent } else { fg })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ])
    };

    p_lines.push(channel_line(
        "R",
        ed.rgb.0,
        &ed.rgb_input[0],
        ThemePickerFocus::Red,
        Color::Red,
    ));
    p_lines.push(channel_line(
        "G",
        ed.rgb.1,
        &ed.rgb_input[1],
        ThemePickerFocus::Green,
        Color::Green,
    ));
    p_lines.push(channel_line(
        "B",
        ed.rgb.2,
        &ed.rgb_input[2],
        ThemePickerFocus::Blue,
        Color::Blue,
    ));

    let hex_active = ed.picker_focus == ThemePickerFocus::Hex;
    let cur_hex = ed.field_value(ed.field_idx);
    let preview = parse_hex_color(cur_hex).unwrap_or(Color::Rgb(ed.rgb.0, ed.rgb.1, ed.rgb.2));
    p_lines.push(Line::from(vec![
        Span::styled(
            "HEX: ",
            Style::default()
                .fg(if hex_active { warning } else { muted })
                .add_modifier(if hex_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled(
            cur_hex.to_string(),
            Style::default()
                .fg(if hex_active { accent } else { fg })
                .add_modifier(if hex_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled("   ███", Style::default().fg(preview)),
    ]));
    p_lines.push(Line::from(Span::styled(
        "Tab: Map/R/G/B/HEX | hjkl: move | Esc: back",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(p_lines), p_inner);
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
    let title = match (st.kind, st.editing_id.is_some()) {
        (AddEntityKind::Project, false) => " Add Project (a) ",
        (AddEntityKind::Project, true) => " Edit Project ",
        (AddEntityKind::Tag, false) => " Add Tag (a) ",
        (AddEntityKind::Tag, true) => " Edit Tag ",
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
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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

fn render_history_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(
        area.width.saturating_sub(8).max(52),
        area.height.saturating_sub(4).max(16),
        area,
    );
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Worklog / History (:w / H) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.border_color()))
        .padding(Padding::uniform(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let today = Local::now().date_naive();
    let today_str = today.format("%Y-%m-%d").to_string();
    let week_ago_str = (today - chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();

    let matches_timeframe = |t: &crate::models::Task| -> bool {
        match app.history_state.timeframe {
            HistoryTimeframe::All => true,
            HistoryTimeframe::Today => t.due_day.as_deref() == Some(today_str.as_str()),
            HistoryTimeframe::Week => t
                .due_day
                .as_deref()
                .is_some_and(|d| d >= week_ago_str.as_str() && d <= today_str.as_str()),
        }
    };

    let filtered: Vec<&crate::models::Task> =
        app.tasks.iter().filter(|t| matches_timeframe(t)).collect();
    let done_count = filtered.iter().filter(|t| t.is_done).count();
    let total_count = filtered.len();
    let total_spent: u64 = filtered.iter().filter_map(|t| t.time_spent).sum();
    let total_est: u64 = filtered.iter().filter_map(|t| t.time_estimate).sum();

    let accent = app.config.theme.accent_color();
    let fg = app.config.theme.fg_color();
    let muted = app.config.theme.muted_color();
    let success = app.config.theme.success_color();
    let warning = app.config.theme.warning_color();

    let tab_style = |t: HistoryTab| {
        if app.history_state.tab == t {
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        }
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" [1] Completed ", tab_style(HistoryTab::Completed)),
            Span::raw(" "),
            Span::styled(" [2] Projects ", tab_style(HistoryTab::Projects)),
            Span::raw(" "),
            Span::styled(" [3] Tags ", tab_style(HistoryTab::Tags)),
            Span::styled(
                format!("   Range (f): {}", app.history_state.timeframe.label()),
                Style::default().fg(warning).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                "Summary: {done_count}/{total_count} done | Spent: {} | Estimate: {}",
                format_ms(total_spent),
                format_ms(total_est)
            ),
            Style::default().fg(success),
        )),
        Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            Style::default().fg(app.config.theme.border_color()),
        )),
    ];

    let mut body_lines: Vec<Line<'static>> = Vec::new();
    match app.history_state.tab {
        HistoryTab::Completed => {
            let done_tasks: Vec<_> = filtered.iter().filter(|t| t.is_done).collect();
            if done_tasks.is_empty() {
                body_lines.push(Line::from(Span::styled(
                    "No completed tasks in this timeframe.",
                    Style::default().fg(muted),
                )));
            } else {
                for t in done_tasks {
                    let spent = t.time_spent.unwrap_or(0);
                    let est = t.time_estimate.unwrap_or(0);
                    let proj = t
                        .project_id
                        .as_ref()
                        .and_then(|pid| app.projects.iter().find(|p| &p.id == pid))
                        .map(|p| format!(" \u{f07b} {}", p.title))
                        .unwrap_or_default();
                    let due = t
                        .due_day
                        .as_deref()
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    body_lines.push(Line::from(vec![
                        Span::styled(" \u{f046} ", Style::default().fg(success)),
                        Span::styled(t.title.clone(), Style::default().fg(fg)),
                        Span::styled(
                            format!("  [{}/{}]{due}{proj}", format_ms(spent), format_ms(est)),
                            Style::default().fg(muted),
                        ),
                    ]));
                }
            }
        }
        HistoryTab::Projects => {
            let inbox_tasks: Vec<_> = filtered.iter().filter(|t| t.project_id.is_none()).collect();
            let i_done = inbox_tasks.iter().filter(|t| t.is_done).count();
            let i_spent: u64 = inbox_tasks.iter().filter_map(|t| t.time_spent).sum();
            let i_est: u64 = inbox_tasks.iter().filter_map(|t| t.time_estimate).sum();
            body_lines.push(Line::from(vec![
                Span::styled(
                    " \u{f01c} Inbox",
                    Style::default().fg(fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  — {i_done}/{} done | Spent: {} / Est: {}",
                        inbox_tasks.len(),
                        format_ms(i_spent),
                        format_ms(i_est)
                    ),
                    Style::default().fg(muted),
                ),
            ]));

            for p in &app.projects {
                let p_tasks: Vec<_> = filtered
                    .iter()
                    .filter(|t| t.project_id.as_deref() == Some(p.id.as_str()))
                    .collect();
                let p_done = p_tasks.iter().filter(|t| t.is_done).count();
                let p_spent: u64 = p_tasks.iter().filter_map(|t| t.time_spent).sum();
                let p_est: u64 = p_tasks.iter().filter_map(|t| t.time_estimate).sum();
                body_lines.push(Line::from(vec![
                    Span::styled(
                        format!(" \u{f07b} {}", p.title),
                        Style::default()
                            .fg(p.display_color())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(
                            "  — {p_done}/{} done | Spent: {} / Est: {}",
                            p_tasks.len(),
                            format_ms(p_spent),
                            format_ms(p_est)
                        ),
                        Style::default().fg(muted),
                    ),
                ]));
            }
        }
        HistoryTab::Tags => {
            if app.tags.is_empty() {
                body_lines.push(Line::from(Span::styled(
                    "No tags found.",
                    Style::default().fg(muted),
                )));
            } else {
                for tag in &app.tags {
                    let t_tasks: Vec<_> = filtered
                        .iter()
                        .filter(|t| t.tag_ids.iter().any(|id| id == &tag.id))
                        .collect();
                    let t_done = t_tasks.iter().filter(|t| t.is_done).count();
                    let t_spent: u64 = t_tasks.iter().filter_map(|t| t.time_spent).sum();
                    let t_est: u64 = t_tasks.iter().filter_map(|t| t.time_estimate).sum();
                    body_lines.push(Line::from(vec![
                        Span::styled(
                            format!(" \u{f292} {}", tag.title),
                            Style::default()
                                .fg(tag.display_color())
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(
                                "  — {t_done}/{} done | Spent: {} / Est: {}",
                                t_tasks.len(),
                                format_ms(t_spent),
                                format_ms(t_est)
                            ),
                            Style::default().fg(muted),
                        ),
                    ]));
                }
            }
        }
    }

    let max_body = inner.height.saturating_sub(5) as usize;
    let skip = app
        .history_state
        .scroll
        .min(body_lines.len().saturating_sub(1));
    for line in body_lines.into_iter().skip(skip).take(max_body.max(1)) {
        lines.push(line);
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "1/2/3 or Tab: switch view | f/Space: timeframe | j/k: scroll | Esc/q: close",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_counters_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(58, 16, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Counters & Daily Habits (:n / C) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.border_color()))
        .padding(Padding::uniform(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let fg = app.config.theme.fg_color();
    let accent = app.config.theme.accent_color();
    let muted = app.config.theme.muted_color();
    let success = app.config.theme.success_color();

    let mut lines = Vec::new();
    if app.counters.is_empty() {
        lines.push(Line::from(Span::styled(
            "No counters yet. Press 'a' to add one.",
            Style::default().fg(muted),
        )));
    } else {
        for (i, c) in app.counters.iter().enumerate() {
            let sel = i == app.counters_state.selected;
            let row_style = if sel {
                Style::default()
                    .fg(accent)
                    .bg(app.config.theme.selection_bg_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(fg)
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" \u{f080} {:<26}", c.title), row_style),
                Span::styled(
                    format!(" [-]  {}  [+] ", c.count),
                    Style::default().fg(success).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }

    if let Some(title) = &app.counters_state.adding {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("New counter: ", Style::default().fg(muted)),
            Span::styled(format!("{title}_"), Style::default().fg(success)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "+/- or h/l: adjust | r: reset | a: add | Esc/q: close",
        Style::default().fg(muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_pomodoro_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(64, 17, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Focus Timer (:p / P) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.border_color()))
        .padding(Padding::uniform(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let p = &app.pomodoro;
    let accent = app.config.theme.accent_color();
    let fg = app.config.theme.fg_color();
    let muted = app.config.theme.muted_color();
    let success = app.config.theme.success_color();
    let warning = app.config.theme.warning_color();

    let mode_tabs: Vec<Span<'static>> = FocusMode::all()
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            let active = *m == p.mode;
            let style = if active {
                Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(muted)
            };
            let mut v = Vec::new();
            if i > 0 {
                v.push(Span::styled("  ", Style::default().fg(muted)));
            }
            v.push(Span::styled(format!(" {} ", m.label()), style));
            v
        })
        .collect();

    let is_flowtime_focus = p.mode == FocusMode::Flowtime && p.phase == PomodoroPhase::Focus;
    let display_secs = if is_flowtime_focus {
        p.elapsed_secs
    } else {
        p.remaining_secs
    };
    let mins = display_secs / 60;
    let secs = display_secs % 60;

    let bar_w = (inner.width as usize).saturating_sub(4).min(36);
    let bar = if is_flowtime_focus {
        let pulse = (p.elapsed_secs as usize) % bar_w.max(1);
        (0..bar_w)
            .map(|i| if i <= pulse { '█' } else { '░' })
            .collect::<String>()
    } else {
        let total = match p.mode {
            FocusMode::Pomodoro => p.phase.duration_secs(),
            FocusMode::Flowtime => p.elapsed_secs.max(60),
            FocusMode::Countdown => p.countdown_mins * 60,
        }
        .max(1);
        let elapsed = total.saturating_sub(p.remaining_secs);
        let ratio = (elapsed as f64 / total as f64).clamp(0.0, 1.0);
        let filled = ((bar_w as f64) * ratio).round() as usize;
        format!(
            "{}{}",
            "█".repeat(filled),
            "░".repeat(bar_w.saturating_sub(filled))
        )
    };

    let linked_title = p
        .linked_task_id
        .as_ref()
        .and_then(|id| app.tasks.iter().find(|t| &t.id == id))
        .map(|t| t.title.as_str())
        .unwrap_or("(No task linked — press l to link selected)");

    let extra_info = match p.mode {
        FocusMode::Pomodoro => format!("(Sessions: {})", p.completed_sessions),
        FocusMode::Flowtime => {
            if is_flowtime_focus {
                format!("(Earned break: {}m)", (p.elapsed_secs / 300).max(1))
            } else {
                "(Earned break)".to_string()
            }
        }
        FocusMode::Countdown => format!("(Target: {}m, +/- to adjust)", p.countdown_mins),
    };

    let mut lines = vec![
        Line::from(mode_tabs),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(" {} ", p.phase.label()),
                Style::default()
                    .fg(Color::Black)
                    .bg(if p.running { success } else { warning })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   {:02}:{:02}   {}", mins, secs, extra_info),
                Style::default().fg(fg).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(bar, Style::default().fg(accent))),
        Line::from(""),
        Line::from(vec![
            Span::styled("Task: ", Style::default().fg(muted)),
            Span::styled(linked_title, Style::default().fg(fg)),
        ]),
    ];

    if let Some(rem) = &p.reminder {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            rem.clone(),
            Style::default().fg(warning).add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Space: start/pause | m/Tab: mode | +/-: mins | r: reset | n: next | l: link | Esc: hide",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_daily_summary_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(76, 26, area);
    frame.render_widget(Clear, popup);

    let today = Local::now().format("%Y-%m-%d").to_string();
    let block = Block::default()
        .title(format!(
            " Daily Summary & End-of-Day Review — {today} (:r / E) "
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.border_color()))
        .padding(Padding::uniform(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let fg = app.config.theme.fg_color();
    let accent = app.config.theme.accent_color();
    let muted = app.config.theme.muted_color();
    let success = app.config.theme.success_color();
    let warning = app.config.theme.warning_color();
    let sel_bg = app.config.theme.selection_bg_color();

    let (completed, unfinished, total_focus_ms) = app.daily_summary_lists();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Completed: ", Style::default().fg(muted)),
            Span::styled(
                format!("{}  ", completed.len()),
                Style::default().fg(success).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Focus Time: ", Style::default().fg(muted)),
            Span::styled(
                format!("{}  ", format_ms(total_focus_ms)),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Pomodoro Sessions: ", Style::default().fg(muted)),
            Span::styled(
                format!("{}  ", app.pomodoro.completed_sessions),
                Style::default().fg(warning).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Remaining: ", Style::default().fg(muted)),
            Span::styled(
                format!("{}", unfinished.len()),
                Style::default().fg(fg).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("Completed Today ({}):", completed.len()),
            Style::default().fg(success).add_modifier(Modifier::BOLD),
        )),
    ];

    if completed.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No tasks completed today yet.",
            Style::default().fg(muted),
        )));
    } else {
        for t in completed.iter().take(6) {
            let spent = t
                .time_spent
                .filter(|ms| *ms > 0)
                .map(|ms| format!(" ({})", format_ms(ms)))
                .unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled("  \u{2713} ", Style::default().fg(success)),
                Span::styled(format!("{}{}", t.title, spent), Style::default().fg(fg)),
            ]));
        }
        if completed.len() > 6 {
            lines.push(Line::from(Span::styled(
                format!("  ... and {} more", completed.len() - 6),
                Style::default().fg(muted),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "Carry-Over Review — Unfinished Today ({}):",
            unfinished.len()
        ),
        Style::default().fg(warning).add_modifier(Modifier::BOLD),
    )));

    if unfinished.is_empty() {
        lines.push(Line::from(Span::styled(
            "  All today's tasks are wrapped up! Great job.",
            Style::default().fg(success),
        )));
    } else {
        let sel = app
            .daily_summary
            .selected
            .min(unfinished.len().saturating_sub(1));
        let max_rows = inner.height.saturating_sub(lines.len() as u16 + 3).max(2) as usize;
        let start = sel.saturating_sub(max_rows.saturating_sub(1));
        for (idx, t) in unfinished.iter().enumerate().skip(start).take(max_rows) {
            let is_sel = idx == sel;
            let style = if is_sel {
                Style::default()
                    .bg(sel_bg)
                    .fg(fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(fg)
            };
            let prefix = if is_sel { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{prefix}[ ] {}", t.title),
                style,
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k: select | t/Enter: Tomorrow | w: Next Week | u: Unschedule | Space: Done | A: Carry All | Esc/q: close",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_reschedule_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(rs) = &app.reschedule_state else {
        return;
    };
    let popup = centered_rect(44, 16, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Reschedule Date Picker (R) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.config.theme.border_color()))
        .padding(Padding::uniform(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let fg = app.config.theme.fg_color();
    let accent = app.config.theme.accent_color();
    let muted = app.config.theme.muted_color();
    let warning = app.config.theme.warning_color();
    let sel_bg = app.config.theme.selection_bg_color();

    let sel = rs.selected_date;
    let first = NaiveDate::from_ymd_opt(sel.year(), sel.month(), 1).unwrap_or(sel);
    let offset = first.weekday().num_days_from_monday() as usize;
    let next_month = if sel.month() == 12 {
        NaiveDate::from_ymd_opt(sel.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(sel.year(), sel.month() + 1, 1)
    }
    .unwrap_or(sel);
    let days_in_month = (next_month - first).num_days() as u32;
    let today = Local::now().date_naive();

    let mut lines = vec![
        Line::from(Span::styled(
            format!("      {} ({})", sel.format("%B %Y"), sel.format("%Y-%m-%d")),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " Mo  Tu  We  Th  Fr  Sa  Su",
            Style::default().fg(muted),
        )),
    ];

    let mut row_spans: Vec<Span<'static>> = vec![Span::raw(" ".repeat(offset * 4))];
    for day in 1..=days_in_month {
        let d = NaiveDate::from_ymd_opt(sel.year(), sel.month(), day).unwrap_or(sel);
        let style = if d == sel {
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD)
        } else if d == today {
            Style::default()
                .fg(warning)
                .bg(sel_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        };
        row_spans.push(Span::styled(format!("{day:>2}  "), style));
        if (offset + day as usize).is_multiple_of(7) {
            lines.push(Line::from(std::mem::take(&mut row_spans)));
        }
    }
    if !row_spans.is_empty() {
        lines.push(Line::from(row_spans));
    }

    lines.push(Line::from(""));
    let time_style = if rs.focus_time {
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg)
    };
    lines.push(Line::from(vec![
        Span::styled("Remind at (HH:MM): ", Style::default().fg(muted)),
        Span::styled(
            if rs.time_input.is_empty() && !rs.focus_time {
                "<optional>"
            } else {
                &rs.time_input
            },
            time_style,
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "hjkl: day | [/]: month | Tab: time | Enter: save",
        Style::default().fg(muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_local_notice_banner(frame: &mut Frame, app: &App, area: Rect) {
    let Some(until) = app.local_notice_until else {
        return;
    };
    let secs_left = until
        .saturating_duration_since(std::time::Instant::now())
        .as_secs()
        .saturating_add(1)
        .min(10);
    let theme = &app.config.theme;
    let accent = theme.accent_color();
    let bg = theme.status_bg_color();

    let spans = vec![
        Span::styled(
            " [info] ",
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " Local mode: works locally, but spt is built for Super Productivity — install SP to sync! ",
            Style::default()
                .fg(accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("(closing in {secs_left}s) "),
            Style::default().fg(theme.muted_color()).bg(bg),
        ),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

fn render_update_banner(frame: &mut Frame, app: &App, area: Rect) {
    let Some(rel) = &app.available_update else {
        return;
    };
    let theme = &app.config.theme;
    let base = Style::default()
        .fg(theme.fg_color())
        .bg(theme.status_bg_color());
    let hint_style = Style::default()
        .fg(theme.hint_fg_color())
        .bg(theme.status_bg_color())
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

    let spans = vec![
        Span::styled(
            " [alert] ",
            Style::default()
                .fg(Color::Black)
                .bg(theme.warning_color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" new version {} released: click ", rel.tag_name),
            Style::default()
                .fg(theme.success_color())
                .bg(theme.status_bg_color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":vc", hint_style),
        Span::styled(" see version changelog, ", base),
        Span::styled(":vi", hint_style),
        Span::styled(" ignore until next release, or ", base),
        Span::styled(":vI", hint_style),
        Span::styled(" ignore forever ", base),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.status_bg_color())),
        area,
    );
}

fn render_changelog_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(76, 22, area);
    frame.render_widget(Clear, popup);

    let theme = &app.config.theme;
    let block = Block::default()
        .title(" Version Changelog (:vc) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.success_color()))
        .padding(Padding::uniform(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(rel) = &app.available_update {
        lines.push(Line::from(vec![
            Span::styled(
                format!("Release {} ", rel.tag_name),
                Style::default()
                    .fg(theme.success_color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("(current v{})", crate::update::CURRENT_VERSION),
                Style::default().fg(theme.muted_color()),
            ),
        ]));
        if !rel.html_url.is_empty() {
            lines.push(Line::from(Span::styled(
                rel.html_url.clone(),
                Style::default().fg(theme.accent_color()),
            )));
        }
        lines.push(Line::from(""));
        let body = if rel.body.trim().is_empty() {
            "(No release notes provided.)"
        } else {
            rel.body.as_str()
        };
        lines.extend(markdown_lines(body, theme));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                "You are up to date (v{}) or checking GitHub releases...",
                crate::update::CURRENT_VERSION
            ),
            Style::default().fg(theme.fg_color()),
        )));
    }

    let max_body = (inner.height as usize).saturating_sub(2).max(1);
    let max_scroll = lines.len().saturating_sub(max_body);
    let skip = app.changelog_scroll.min(max_scroll);

    let mut rendered: Vec<Line<'static>> = lines.into_iter().skip(skip).take(max_body).collect();
    rendered.push(Line::from(""));
    rendered.push(Line::from(Span::styled(
        "j/k/g/G: scroll | :vi: ignore this release | :vI: ignore forever | Esc/q: close",
        Style::default().fg(theme.muted_color()),
    )));
    frame.render_widget(Paragraph::new(rendered).wrap(Wrap { trim: true }), inner);
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
            ActiveModal::History,
            ActiveModal::Counters,
            ActiveModal::Pomodoro,
            ActiveModal::Reschedule,
            ActiveModal::DailySummary,
            ActiveModal::Changelog,
        ] {
            if modal == ActiveModal::Theme {
                app.theme_editor = Some(ThemeEditorState::new(
                    &app.config.theme,
                    &app.config.saved_themes,
                ));
            }
            if modal == ActiveModal::Reschedule {
                app.reschedule_state = Some(crate::app::RescheduleState {
                    task_id: "t1".into(),
                    selected_date: Local::now().date_naive(),
                    time_input: String::new(),
                    focus_time: false,
                });
            }
            app.active_modal = modal;
            terminal.draw(|f| render(f, &mut app)).unwrap();
        }
    }
}
