use crate::app::{ActiveModal, AddTaskField, App, FocusSection, NavItemKind, SettingsField};
use crate::config::KeybindMode;
use crate::keybinds::all_keybinds;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};

/// Underlines the first character of each word when `hint` is enabled.
pub fn hint_spans(text: &str, hint: bool, base: Style) -> Vec<Span<'static>> {
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
                base.add_modifier(Modifier::UNDERLINED),
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

    if app.active_modal != ActiveModal::None {
        dim_background(frame, root);
    }

    match app.active_modal {
        ActiveModal::None => {}
        ActiveModal::Properties => render_properties_modal(frame, app, root),
        ActiveModal::Help => render_help_modal(frame, app, root),
        ActiveModal::Settings => render_settings_modal(frame, app, root),
        ActiveModal::AddTask => render_add_task_modal(frame, app, root),
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
            Paragraph::new(sep).style(Style::default().fg(Color::DarkGray)),
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
        NavItemKind::Filter(crate::app::NavFilter::Project(_)) => Color::Magenta,
        NavItemKind::Filter(crate::app::NavFilter::Tag(_)) => Color::Green,
        _ => Color::Gray,
    };

    let mut style = Style::default().fg(fg);
    if is_active_filter {
        style = style.add_modifier(Modifier::BOLD);
    }
    if is_selected {
        style = style.bg(Color::DarkGray).fg(Color::White);
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

    for offset in 0..area.height {
        let y = area.y + offset;
        let rect = Rect::new(area.x, y, 1, 1);
        if offset == mid {
            let style = if app.config.hint_keybind {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::Cyan)
            };
            frame.render_widget(Paragraph::new(hint_char.to_string()).style(style), rect);
        } else {
            frame.render_widget(
                Paragraph::new("│").style(Style::default().fg(Color::DarkGray)),
                rect,
            );
        }
    }
}

fn render_main(frame: &mut Frame, app: &mut App, area: Rect) {
    let padded = Block::default()
        .padding(Padding::symmetric(4, 2))
        .style(Style::default());
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
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let note_height = if app.show_note { 5 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(note_height)])
        .split(inner);

    let list_area = chunks[0];
    let note_area = chunks[1];
    let mut task_rows = Vec::new();

    for (idx, task) in tasks.iter().enumerate() {
        let y = list_area.y + idx as u16;
        if y >= list_area.bottom() {
            break;
        }
        let row_rect = Rect::new(list_area.x, y, list_area.width, 1);
        task_rows.push((row_rect, idx));

        let is_selected = app.focus == FocusSection::Main && app.task_selected == idx;
        let is_renaming = app.renaming_task_id.as_deref() == Some(task.id.as_str());

        let checkbox = if task.is_done { "\u{f046}" } else { "\u{f096}" };
        let title_text = if is_renaming {
            format!("{}|", app.rename_buffer)
        } else {
            task.title.clone()
        };

        let mut meta_parts = Vec::new();
        if task.repeat_cfg_id.is_some() {
            meta_parts.push(Span::styled(
                " \u{f01e} Repeat ",
                Style::default().fg(Color::DarkGray),
            ));
        }
        if let Some(pid) = &task.project_id
            && let Some(proj) = app.projects.iter().find(|p| &p.id == pid)
        {
            meta_parts.push(Span::styled(
                format!(" \u{f07b} {} ", proj.title),
                Style::default().fg(Color::Magenta),
            ));
        }
        for tid in &task.tag_ids {
            if let Some(tag) = app.tags.iter().find(|t| &t.id == tid) {
                meta_parts.push(Span::styled(
                    format!(" \u{f292} #{} ", tag.title),
                    Style::default().fg(Color::Green),
                ));
            }
        }

        let mut row_style = Style::default().fg(Color::White);
        if task.is_done {
            row_style = row_style
                .fg(Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT);
        }
        if is_selected {
            row_style = row_style.bg(Color::DarkGray);
        }
        if is_renaming {
            row_style = Style::default().bg(Color::Blue).fg(Color::White);
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
                part.style.bg(Color::DarkGray)
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
            .title(" Note ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        frame.render_widget(
            Paragraph::new(note_text)
                .block(block)
                .wrap(Wrap { trim: true }),
            note_area,
        );
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mode_label = if app.renaming_task_id.is_some() {
        "INSERT (jk/Enter to save)"
    } else {
        app.config.keybind_mode.label()
    };
    let nav_toggle_hint = if app.config.keybind_mode == KeybindMode::Nvim {
        "zn: sidebar"
    } else {
        "hn: sidebar"
    };
    let status_color = if app.connected {
        Color::Green
    } else {
        Color::Red
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" [{mode_label}] "),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::styled(
            format!(" {} ", app.status_message),
            Style::default().fg(status_color),
        ),
        Span::styled(
            format!(" | {nav_toggle_hint} | :h help | :s settings "),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_properties_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(36, 7, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Properties ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = vec![Line::from(hint_spans(
        "rename",
        app.config.hint_keybind,
        Style::default().fg(Color::White),
    ))];
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Esc to close",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_help_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(72, 20, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Help / Keybinds (:h) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let query = app.help_search.to_lowercase();
    let nvim_active = app.config.keybind_mode.is_nvim_active();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}_", app.help_search),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
    ];

    for entry in all_keybinds() {
        let key_str = entry.display_key(app.config.keybind_mode);
        if !query.is_empty()
            && !key_str.to_lowercase().contains(&query)
            && !entry.description.to_lowercase().contains(&query)
        {
            continue;
        }
        let color = if entry.is_nvim_highlight && nvim_active {
            Color::Green
        } else {
            Color::White
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{key_str:<22}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(entry.description, Style::default().fg(color)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_settings_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(64, 14, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Settings (:s) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let st = &app.settings_state;
    let field_style = |f: SettingsField| {
        if st.active_field == f {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
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
                .fg(Color::Yellow)
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
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_add_task_modal(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(64, 15, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Add Task (a) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let st = &app.add_task_state;
    let field_style = |f: AddTaskField| {
        if st.active_field == f {
            Style::default().fg(Color::Black).bg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        }
    };

    let proj_name = if st.project_idx == 0 {
        "None (Inbox)".to_string()
    } else {
        app.projects
            .get(st.project_idx - 1)
            .map(|p| p.title.clone())
            .unwrap_or_else(|| "None".to_string())
    };

    let tags_display = if app.tags.is_empty() {
        "No tags available".to_string()
    } else {
        app.tags
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let checked = st.selected_tag_ids.contains(&t.id);
                let mark = if checked { "[x]" } else { "[ ]" };
                if st.active_field == AddTaskField::Tags && i == st.tag_cursor {
                    format!(">{mark}#{}<", t.title)
                } else {
                    format!("{mark}#{}", t.title)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    let lines = vec![
        Line::from(vec![
            Span::raw("Title:    "),
            Span::styled(&st.title, field_style(AddTaskField::Title)),
        ]),
        Line::from(vec![
            Span::raw("Note:     "),
            Span::styled(&st.note, field_style(AddTaskField::Note)),
        ]),
        Line::from(vec![
            Span::raw("Position: "),
            Span::styled(
                if st.add_to_top { "Top" } else { "Bottom" },
                field_style(AddTaskField::Position),
            ),
        ]),
        Line::from(vec![
            Span::raw("Project:  "),
            Span::styled(proj_name, field_style(AddTaskField::Project)),
        ]),
        Line::from(vec![
            Span::raw("Tags:     "),
            Span::styled(tags_display, field_style(AddTaskField::Tags)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Tab: next | Left/Right/Space: select | Enter: create | Esc: cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
