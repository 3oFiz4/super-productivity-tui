use crate::api::SuperProductivityClient;
use crate::config::AppConfig;
use crate::models::{CreateTaskPayload, Project, Tag, Task};
use crate::offline::{OfflineCache, QueuedOp, sanitize_create_payload_for_sp};
use crate::short_syntax::{parse_duration_ms, parse_short_syntax};
use crate::theme::ThemeConfig;
use anyhow::{Context, Result, anyhow, bail};
use chrono::{Duration as ChronoDuration, Local, NaiveDate, NaiveTime};
use ratatui::style::Color;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::net::TcpStream;
use std::time::Duration;

pub async fn run(args: Vec<String>) -> Result<()> {
    let Some((cmd, rest)) = args.split_first() else {
        print_help(&AppConfig::load().theme, true);
        return Ok(());
    };

    let color_on =
        !has_any_flag(&args, &["--no-color", "-C"]) && std::env::var_os("NO_COLOR").is_none();

    match cmd.as_str() {
        "-h" | "--help" | "help" => {
            print_help(&AppConfig::load().theme, color_on);
            Ok(())
        }
        "-V" | "--version" | "version" => {
            println!("spt {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "add" => cmd_add(rest, color_on).await,
        "ls" | "list" => cmd_ls(rest, color_on).await,
        "done" => cmd_done(rest, false, color_on).await,
        "undo-done" | "reopen" => cmd_done(rest, true, color_on).await,
        "edit" => cmd_edit(rest, color_on).await,
        "schedule" | "sched" => cmd_schedule(rest, color_on).await,
        "rm" | "delete" => cmd_rm(rest, color_on).await,
        "start" => cmd_start(rest, color_on).await,
        "stop" => cmd_stop(rest, color_on).await,
        "log" => cmd_log(rest, color_on).await,
        "status" => cmd_status(rest, color_on).await,
        "current" => cmd_current(rest, color_on).await,
        "projects" => cmd_projects(rest, color_on).await,
        "tags" => cmd_tags(rest, color_on).await,
        "counter" | "counters" => cmd_counter(rest, color_on),
        "summary" => cmd_summary(rest, color_on).await,
        "worklog" => cmd_worklog(rest, color_on).await,
        "sync" => cmd_sync(rest, color_on).await,
        "doctor" => cmd_doctor(rest, color_on).await,
        other => bail!("Unknown subcommand '{other}'. Run `spt --help` for usage."),
    }
}

fn paint(c: Color, text: &str, enabled: bool) -> String {
    if !enabled {
        return text.to_string();
    }
    let rgb = color_to_rgb(c);
    format!("\x1b[38;2;{};{};{}m{text}\x1b[0m", rgb[0], rgb[1], rgb[2])
}

fn paint_bold(c: Color, text: &str, enabled: bool) -> String {
    if !enabled {
        return text.to_string();
    }
    let rgb = color_to_rgb(c);
    format!("\x1b[1;38;2;{};{};{}m{text}\x1b[0m", rgb[0], rgb[1], rgb[2])
}

fn color_to_rgb(c: Color) -> [u8; 3] {
    match c {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Magenta => [203, 166, 247],
        Color::Green => [166, 227, 161],
        Color::Yellow => [249, 226, 175],
        Color::Red => [243, 139, 168],
        Color::Blue => [137, 180, 250],
        Color::Cyan => [148, 226, 213],
        _ => [205, 214, 244],
    }
}

fn print_help(theme: &ThemeConfig, color_on: bool) {
    let hdr = paint_bold(theme.accent_color(), "spt", color_on);
    let ver = env!("CARGO_PKG_VERSION");
    println!(
        r#"{hdr} {ver} — Super Productivity TUI & CLI

USAGE:
    spt                     Launch interactive TUI
    spt <COMMAND> [ARGS]    Run non-interactive CLI command

GLOBAL FLAGS:
    -j, --json              Output structured JSON
    -C, --no-color          Disable ANSI theme colors

COMMANDS:
    add <title...>          Add task with short syntax (+proj #tag t1h @due !deadline) & typo auto-fix
                            Flags: -t/--top, -n/--note <text>, -P/--parent <id>, -j/--json
    ls [view]               Tree-formatted task list (all, today, inbox, planned, overdue)
                            Flags: -p/--project <p>, -g/--tag <t>, -s/--search <q>, -d/--done, -l/--limit <n>
    done <id|pattern|q>     Mark task done (-u/--undo to reopen)
    undo-done <id|pat|q>    Reopen a completed task
    edit <id|pat|q> <...>   Edit task inline with short syntax & typo auto-fix
    schedule <id|p> <when>  Schedule (today, tomorrow, next-week, YYYY-MM-DD, clear) [-r/--remind HH:MM]
    rm <id|pattern|q>       Delete a task
    start <id|pattern|q>    Start timer on task
    stop                    Stop active task timer
    log <id|pat|q> <dur>    Add time spent (e.g. 25m, 1h30m)
    status                  Single-line status (-w/--waybar, -T/--tmux, -j/--json)
    current                 Show currently tracked task (exits 1 if idle)
    projects                List projects with theme colors
    tags                    List tags with theme colors
    counter [act] [name]    Manage counters: list, inc, dec, set <name> [val]
    summary [today]         Show today's completed/unfinished summary
    worklog                 Time-tracking breakdown (-D/--day, -w/--week, -m/--month, -b/--by project|tag)
    sync                    Flush offline queue and refresh local cache
    doctor                  Check API, companion bridge, config, and cache health

ID PATTERNS:
    Supports SQL-like dot patterns: 'G2.' (starts with G2), '._g' (ends with _g), 'G2.k_g' (G2...k_g)"#
    );
}

fn has_any_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter().any(|a| flags.contains(&a.as_str()))
}

fn flag_any_value(args: &[String], flags: &[&str]) -> Option<String> {
    args.windows(2)
        .find(|w| flags.contains(&w[0].as_str()))
        .map(|w| w[1].clone())
}

fn positional_args(args: &[String], value_flags: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for a in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if value_flags.contains(&a.as_str()) {
            skip_next = true;
            continue;
        }
        if a.starts_with("--") || (a.starts_with('-') && a.len() == 2) {
            continue;
        }
        out.push(a.clone());
    }
    out
}

struct Snapshot {
    theme: ThemeConfig,
    client: SuperProductivityClient,
    cache: OfflineCache,
    online: bool,
}

async fn load_snapshot(refresh: bool) -> Snapshot {
    let config = AppConfig::load();
    let theme = config.theme.clone();
    let client = SuperProductivityClient::new(&config.api_url, &config.api_token);
    let mut cache = OfflineCache::load();
    let mut online = false;

    if refresh
        && let Ok(h) = client.health().await
        && h.server == "up"
        && h.renderer_ready
        && let Ok(tasks) = client.get_tasks().await
    {
        online = true;
        cache.tasks = tasks;
        if let Ok(projects) = client.get_projects().await {
            cache.projects = projects;
        }
        if let Ok(tags) = client.get_tags().await {
            cache.tags = tags;
        }
        if let Ok(Some(c)) = client.get_counters().await.map(Some) {
            cache.counters = c;
        }
        cache.current_task_id = client.get_current_task().await.ok().flatten().map(|t| t.id);
        let _ = cache.save();
    }

    Snapshot {
        theme,
        client,
        cache,
        online,
    }
}

fn all_projects(cache: &OfflineCache) -> Vec<Project> {
    let mut merged = cache.projects.clone();
    for lp in &cache.local_projects {
        if !merged.iter().any(|p| p.id == lp.id) {
            merged.push(lp.clone());
        }
    }
    merged
}

fn all_tags(cache: &OfflineCache) -> Vec<Tag> {
    let mut merged = cache.tags.clone();
    for lt in &cache.local_tags {
        if !merged.iter().any(|t| t.id == lt.id) {
            merged.push(lt.clone());
        }
    }
    merged
}

/// SQL-like dot pattern matching on IDs:
/// - `G2.` matches IDs starting with `G2` (`G2%`)
/// - `._g` matches IDs ending with `_g` (`%_g`)
/// - `G2.k_g` matches IDs starting with `G2` and ending with `k_g` (`G2%k_g`)
/// - `.Yuk.` matches IDs containing `Yuk` (`%Yuk%`)
pub fn match_dot_pattern(id: &str, pattern: &str) -> bool {
    if !pattern.contains('.') {
        return id == pattern;
    }
    let parts: Vec<&str> = pattern.split('.').collect();
    let first = parts[0];
    let last = parts[parts.len() - 1];

    if !first.is_empty() && !id.starts_with(first) {
        return false;
    }
    if !last.is_empty() && !id.ends_with(last) {
        return false;
    }
    if parts.len() == 2 && first.len() + last.len() > id.len() {
        return false;
    }

    let mut cursor = first.len();
    let end_limit = id.len().saturating_sub(last.len());
    for mid in &parts[1..parts.len() - 1] {
        if mid.is_empty() {
            continue;
        }
        let Some(pos) = id[cursor..end_limit].find(mid) else {
            return false;
        };
        cursor += pos + mid.len();
    }
    true
}

pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut d = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(dp[i - 2][j - 2] + cost);
            }
            dp[i][j] = d;
        }
    }
    dp[n][m]
}

fn best_fuzzy_match<'a>(needle: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let n_low = needle.to_lowercase();
    if n_low.is_empty() {
        return None;
    }
    if let Some(&exact) = candidates.iter().find(|c| c.to_lowercase() == n_low) {
        return Some(exact);
    }
    let max_dist = if n_low.len() >= 6 { 3 } else { 2 };
    let mut best: Option<(&str, usize)> = None;
    for &cand in candidates {
        let c_low = cand.to_lowercase();
        let dist = damerau_levenshtein(&n_low, &c_low);
        if dist <= max_dist && best.is_none_or(|(_, bd)| dist < bd) {
            best = Some((cand, dist));
        }
    }
    best.map(|(c, _)| c)
}

/// Auto-corrects typos in `#tag`, `+project`, `@date_keyword`, and `!date_keyword`.
pub fn autocorrect_short_syntax(
    raw: &str,
    projects: &[Project],
    tags: &[Tag],
) -> (String, Vec<String>) {
    const DATE_KEYWORDS: &[&str] = &[
        "today",
        "tomorrow",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "next_week",
        "next_month",
    ];
    let tag_names: Vec<&str> = tags.iter().map(|t| t.title.as_str()).collect();
    let proj_names: Vec<&str> = projects.iter().map(|p| p.title.as_str()).collect();
    let mut corrections = Vec::new();
    let mut out_tokens = Vec::new();

    for tok in raw.split_whitespace() {
        if let Some(name) = tok.strip_prefix('#')
            && !name.is_empty()
            && !tag_names.iter().any(|t| t.eq_ignore_ascii_case(name))
            && let Some(fixed) = best_fuzzy_match(name, &tag_names)
        {
            let replacement = format!("#{fixed}");
            corrections.push(format!("{tok} -> {replacement}"));
            out_tokens.push(replacement);
            continue;
        }
        if let Some(name) = tok.strip_prefix('+')
            && !name.is_empty()
            && !proj_names.iter().any(|p| p.eq_ignore_ascii_case(name))
            && let Some(fixed) = best_fuzzy_match(name, &proj_names)
        {
            let replacement = format!("+{fixed}");
            corrections.push(format!("{tok} -> {replacement}"));
            out_tokens.push(replacement);
            continue;
        }
        if let Some(kw) = tok.strip_prefix('@')
            && kw.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
            && !DATE_KEYWORDS.iter().any(|d| d.eq_ignore_ascii_case(kw))
            && let Some(fixed) = best_fuzzy_match(kw, DATE_KEYWORDS)
        {
            let replacement = format!("@{fixed}");
            corrections.push(format!("{tok} -> {replacement}"));
            out_tokens.push(replacement);
            continue;
        }
        if let Some(kw) = tok.strip_prefix('!')
            && kw.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
            && !DATE_KEYWORDS.iter().any(|d| d.eq_ignore_ascii_case(kw))
            && let Some(fixed) = best_fuzzy_match(kw, DATE_KEYWORDS)
        {
            let replacement = format!("!{fixed}");
            corrections.push(format!("{tok} -> {replacement}"));
            out_tokens.push(replacement);
            continue;
        }
        out_tokens.push(tok.to_string());
    }

    (out_tokens.join(" "), corrections)
}

pub fn resolve_task<'a>(tasks: &'a [Task], query: &str) -> Result<&'a Task> {
    if let Some(exact) = tasks.iter().find(|t| t.id == query) {
        return Ok(exact);
    }
    if query.contains('.') {
        let dot_matches: Vec<&Task> = tasks
            .iter()
            .filter(|t| match_dot_pattern(&t.id, query))
            .collect();
        match dot_matches.len() {
            0 => bail!("No task ID matching pattern '{query}'."),
            1 => return Ok(dot_matches[0]),
            n => {
                let preview: Vec<String> = dot_matches
                    .iter()
                    .take(5)
                    .map(|t| format!("  {}  {}", t.id, t.title))
                    .collect();
                bail!(
                    "Pattern '{query}' matched {n} tasks. Be more specific:\n{}",
                    preview.join("\n")
                );
            }
        }
    }
    let prefix_matches: Vec<&Task> = tasks.iter().filter(|t| t.id.starts_with(query)).collect();
    if prefix_matches.len() == 1 {
        return Ok(prefix_matches[0]);
    }
    let q_low = query.to_lowercase();
    let title_matches: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.title.to_lowercase().contains(&q_low))
        .collect();
    match title_matches.len() {
        0 => bail!("No task matching '{query}'."),
        1 => Ok(title_matches[0]),
        n => {
            let preview: Vec<String> = title_matches
                .iter()
                .take(5)
                .map(|t| format!("  {}  {}", t.id, t.title))
                .collect();
            bail!(
                "Ambiguous query '{query}' matched {n} tasks. Use an ID or dot pattern (e.g. G2.):\n{}",
                preview.join("\n")
            );
        }
    }
}

fn format_ms(ms: u64) -> String {
    let total_mins = ms / 60_000;
    let h = total_mins / 60;
    let m = total_mins % 60;
    if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

fn format_task_meta(
    t: &Task,
    projects: &[Project],
    tags: &[Tag],
    today: &str,
    theme: &ThemeConfig,
    color_on: bool,
) -> String {
    let mut parts = Vec::new();
    let spent = t.time_spent.unwrap_or(0);
    let est = t.time_estimate.unwrap_or(0);
    if spent > 0 || est > 0 {
        parts.push(paint(
            theme.muted_color(),
            &format!("⏱ {}/{}", format_ms(spent), format_ms(est)),
            color_on,
        ));
    }
    if let Some(pid) = &t.project_id
        && let Some(p) = projects.iter().find(|p| &p.id == pid)
    {
        parts.push(paint(p.display_color(), &format!("+{}", p.title), color_on));
    }
    for tid in &t.tag_ids {
        if let Some(tag) = tags.iter().find(|tg| &tg.id == tid) {
            parts.push(paint(
                tag.display_color(),
                &format!("#{}", tag.title),
                color_on,
            ));
        }
    }
    if let Some(due) = &t.due_day {
        let c = if due.as_str() < today {
            theme.danger_color()
        } else if due.as_str() == today {
            theme.warning_color()
        } else {
            theme.muted_color()
        };
        parts.push(paint(c, &format!("@{due}"), color_on));
    }
    if let Some(dl) = &t.deadline_day {
        let c = if dl.as_str() <= today {
            theme.danger_color()
        } else {
            theme.accent_color()
        };
        parts.push(paint(c, &format!("!{dl}"), color_on));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("  {}", parts.join(" "))
    }
}

async fn cmd_add(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let to_top = has_any_flag(args, &["--top", "-t"]);
    let note = flag_any_value(args, &["--note", "-n"]);
    let parent_id = flag_any_value(args, &["--parent", "-P"]);
    let pos = positional_args(args, &["--note", "-n", "--parent", "-P"]);
    let raw = pos.join(" ").trim().to_string();
    if raw.is_empty() {
        bail!(
            "Usage: spt add \"<title + short syntax>\" [-t|--top] [-n|--note <text>] [-P|--parent <id>]"
        );
    }

    let mut snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    let tags = all_tags(&snap.cache);
    let (corrected_raw, corrections) = autocorrect_short_syntax(&raw, &projects, &tags);
    let parsed = parse_short_syntax(&corrected_raw, &projects, &tags, None, Vec::new());
    let payload = CreateTaskPayload {
        title: if parsed.title.is_empty() {
            corrected_raw.clone()
        } else {
            parsed.title
        },
        project_id: parsed.project_id,
        parent_id,
        tag_ids: parsed.tag_ids,
        notes: note,
        due_day: parsed.due_day,
        deadline_day: parsed.deadline_day,
        time_estimate: parsed.time_estimate,
        time_spent: parsed.time_spent,
        is_add_to_top: Some(to_top),
    };

    if snap.online {
        let clean = sanitize_create_payload_for_sp(
            payload.clone(),
            &HashMap::new(),
            &snap.cache.project_id_map,
            &snap.cache.tag_id_map,
        );
        if let Ok(created) = snap.client.create_task(clean).await {
            if to_top {
                snap.cache.tasks.insert(0, created.clone());
            } else {
                snap.cache.tasks.push(created.clone());
            }
            let _ = snap.cache.save();
            if json_out {
                println!("{}", serde_json::to_string_pretty(&created)?);
            } else {
                if !corrections.is_empty() {
                    println!(
                        "{}",
                        paint(
                            snap.theme.warning_color(),
                            &format!("Auto-corrected: {}", corrections.join(", ")),
                            color_on
                        )
                    );
                }
                let today = Local::now().format("%Y-%m-%d").to_string();
                let meta =
                    format_task_meta(&created, &projects, &tags, &today, &snap.theme, color_on);
                println!(
                    "{} {} {}{meta}",
                    paint_bold(snap.theme.success_color(), "✓ Created", color_on),
                    paint_bold(snap.theme.fg_color(), &created.title, color_on),
                    paint(
                        snap.theme.muted_color(),
                        &format!("({})", created.id),
                        color_on
                    )
                );
            }
            return Ok(());
        }
    }

    let temp_id = format!("offline-task-{}", Local::now().timestamp_millis());
    let task = Task {
        id: temp_id.clone(),
        title: payload.title.clone(),
        is_done: false,
        notes: payload.notes.clone(),
        project_id: payload.project_id.clone(),
        parent_id: payload.parent_id.clone(),
        tag_ids: payload.tag_ids.clone(),
        due_day: payload.due_day.clone(),
        deadline_day: payload.deadline_day.clone(),
        remind_at: None,
        done_on: None,
        time_estimate: payload.time_estimate,
        time_spent: payload.time_spent,
        repeat_cfg_id: None,
    };
    if to_top {
        snap.cache.tasks.insert(0, task.clone());
    } else {
        snap.cache.tasks.push(task.clone());
    }
    snap.cache.pending_ops.push(QueuedOp::CreateTask {
        temp_id,
        payload,
        to_top,
    });
    snap.cache.save()?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else {
        if !corrections.is_empty() {
            println!(
                "{}",
                paint(
                    snap.theme.warning_color(),
                    &format!("Auto-corrected: {}", corrections.join(", ")),
                    color_on
                )
            );
        }
        println!(
            "{} {} {}",
            paint_bold(snap.theme.warning_color(), "⏳ Queued offline", color_on),
            paint_bold(snap.theme.fg_color(), &task.title, color_on),
            paint(
                snap.theme.muted_color(),
                &format!("({})", task.id),
                color_on
            )
        );
    }
    Ok(())
}

async fn cmd_ls(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let include_done = has_any_flag(args, &["--done", "-d"]);
    let proj_filter = flag_any_value(args, &["--project", "-p"]).map(|s| s.to_lowercase());
    let tag_filter = flag_any_value(args, &["--tag", "-g"]).map(|s| s.to_lowercase());
    let search_filter = flag_any_value(args, &["--search", "-s"]).map(|s| s.to_lowercase());
    let limit = flag_any_value(args, &["--limit", "-l"]).and_then(|s| s.parse::<usize>().ok());
    let pos = positional_args(
        args,
        &[
            "--project",
            "-p",
            "--tag",
            "-g",
            "--search",
            "-s",
            "--limit",
            "-l",
        ],
    );
    let view = pos.first().map(|s| s.as_str()).unwrap_or("all");

    let snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    let tags = all_tags(&snap.cache);
    let today = Local::now().format("%Y-%m-%d").to_string();

    let proj_id_filter = proj_filter.as_ref().and_then(|q| {
        projects
            .iter()
            .find(|p| p.title.to_lowercase().contains(q) || p.id == *q)
            .map(|p| p.id.clone())
    });
    let tag_id_filter = tag_filter.as_ref().and_then(|q| {
        tags.iter()
            .find(|t| t.title.to_lowercase().contains(q) || t.id == *q)
            .map(|t| t.id.clone())
    });

    let mut filtered: Vec<&Task> = snap
        .cache
        .tasks
        .iter()
        .filter(|t| t.parent_id.is_none())
        .filter(|t| include_done || !t.is_done)
        .filter(|t| match view {
            "today" => t.due_day.as_deref() == Some(today.as_str()),
            "inbox" => t.project_id.is_none(),
            "planned" | "upcoming" => t.due_day.is_some(),
            "overdue" => {
                t.due_day.as_deref().is_some_and(|d| d < today.as_str())
                    || t.deadline_day
                        .as_deref()
                        .is_some_and(|d| d < today.as_str())
            }
            _ => true,
        })
        .filter(|t| match (&proj_filter, &proj_id_filter) {
            (Some(_), Some(pid)) => t.project_id.as_deref() == Some(pid.as_str()),
            (Some(_), None) => false,
            _ => true,
        })
        .filter(|t| match (&tag_filter, &tag_id_filter) {
            (Some(_), Some(tid)) => t.tag_ids.contains(tid),
            (Some(_), None) => false,
            _ => true,
        })
        .filter(|t| {
            search_filter
                .as_ref()
                .is_none_or(|q| t.title.to_lowercase().contains(q))
        })
        .collect();

    if let Some(lim) = limit {
        filtered.truncate(lim);
    }

    if json_out {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
        return Ok(());
    }

    if filtered.is_empty() {
        println!(
            "{}",
            paint(snap.theme.muted_color(), "No tasks found.", color_on)
        );
        return Ok(());
    }

    let status_dot = if snap.online {
        paint(snap.theme.success_color(), "● online", color_on)
    } else {
        paint(snap.theme.warning_color(), "○ offline", color_on)
    };
    println!(
        "{} {} {}",
        paint_bold(snap.theme.accent_color(), &format!("╭─ {view}"), color_on),
        paint(
            snap.theme.muted_color(),
            &format!("({} tasks)", filtered.len()),
            color_on
        ),
        status_dot
    );

    let last_idx = filtered.len() - 1;
    for (idx, t) in filtered.iter().enumerate() {
        let is_last = idx == last_idx;
        let branch = if is_last { "╰─" } else { "├─" };
        let child_indent = if is_last { "   " } else { "│  " };
        let is_tracking = snap.cache.current_task_id.as_deref() == Some(t.id.as_str());
        let icon = if t.is_done {
            paint(snap.theme.success_color(), "✓", color_on)
        } else if is_tracking {
            paint_bold(snap.theme.success_color(), "▶", color_on)
        } else {
            paint(snap.theme.accent_color(), "○", color_on)
        };
        let title_str = if t.is_done {
            paint(snap.theme.muted_color(), &t.title, color_on)
        } else if is_tracking {
            paint_bold(snap.theme.success_color(), &t.title, color_on)
        } else {
            paint(snap.theme.fg_color(), &t.title, color_on)
        };
        let id_str = paint(snap.theme.muted_color(), &format!("({})", t.id), color_on);
        let meta = format_task_meta(t, &projects, &tags, &today, &snap.theme, color_on);

        println!(
            "{} {icon} {title_str}  {id_str}{meta}",
            paint(snap.theme.border_color(), branch, color_on)
        );

        let subs: Vec<&Task> = snap
            .cache
            .tasks
            .iter()
            .filter(|st| st.parent_id.as_deref() == Some(t.id.as_str()))
            .filter(|st| include_done || !st.is_done)
            .collect();
        for (s_idx, sub) in subs.iter().enumerate() {
            let sub_branch = if s_idx + 1 == subs.len() {
                "╰─"
            } else {
                "├─"
            };
            let sub_icon = if sub.is_done {
                paint(snap.theme.success_color(), "✓", color_on)
            } else {
                paint(snap.theme.muted_color(), "○", color_on)
            };
            let sub_title = paint(
                if sub.is_done {
                    snap.theme.muted_color()
                } else {
                    snap.theme.fg_color()
                },
                &sub.title,
                color_on,
            );
            let sub_id = paint(snap.theme.muted_color(), &format!("({})", sub.id), color_on);
            println!(
                "{}{} {sub_icon} {sub_title}  {sub_id}",
                paint(snap.theme.border_color(), child_indent, color_on),
                paint(snap.theme.border_color(), sub_branch, color_on)
            );
        }
    }
    Ok(())
}

async fn apply_patch(
    snap: &mut Snapshot,
    task_id: &str,
    patch: serde_json::Value,
    json_out: bool,
    human_msg: &str,
    color_on: bool,
) -> Result<()> {
    if snap.online
        && !task_id.starts_with("offline-task-")
        && let Ok(updated) = snap.client.update_task(task_id, patch.clone()).await
    {
        if let Some(slot) = snap.cache.tasks.iter_mut().find(|t| t.id == task_id) {
            *slot = updated.clone();
        }
        let _ = snap.cache.save();
        if json_out {
            println!("{}", serde_json::to_string_pretty(&updated)?);
        } else {
            println!("{}", paint(snap.theme.success_color(), human_msg, color_on));
        }
        return Ok(());
    }

    snap.cache.pending_ops.push(QueuedOp::UpdateTask {
        id: task_id.to_string(),
        patch,
    });
    snap.cache.save()?;
    if json_out {
        if let Some(t) = snap.cache.tasks.iter().find(|t| t.id == task_id) {
            println!("{}", serde_json::to_string_pretty(t)?);
        }
    } else {
        println!(
            "{}",
            paint(
                snap.theme.warning_color(),
                &format!("{human_msg} (queued offline)"),
                color_on
            )
        );
    }
    Ok(())
}

async fn cmd_done(args: &[String], force_undo: bool, color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let undo = force_undo || has_any_flag(args, &["--undo", "-u"]);
    let pos = positional_args(args, &[]);
    let query = pos.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("Usage: spt done <id|pattern|query> [-u|--undo] [-j|--json]");
    }
    let mut snap = load_snapshot(true).await;
    let task = resolve_task(&snap.cache.tasks, &query)?.clone();
    let target_done = !undo;
    if let Some(t) = snap.cache.tasks.iter_mut().find(|t| t.id == task.id) {
        t.is_done = target_done;
    }
    let verb = if target_done {
        "✓ Completed"
    } else {
        "↺ Reopened"
    };
    apply_patch(
        &mut snap,
        &task.id,
        json!({ "isDone": target_done }),
        json_out,
        &format!("{verb}: {} ({})", task.title, task.id),
        color_on,
    )
    .await
}

async fn cmd_edit(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let pos = positional_args(args, &[]);
    if pos.len() < 2 {
        bail!("Usage: spt edit <id|pattern|query> \"<title + short syntax>\" [-j|--json]");
    }
    let query = &pos[0];
    let raw = pos[1..].join(" ").trim().to_string();
    let mut snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    let tags = all_tags(&snap.cache);
    let task = resolve_task(&snap.cache.tasks, query)?.clone();
    let (corrected_raw, corrections) = autocorrect_short_syntax(&raw, &projects, &tags);
    if !json_out && !corrections.is_empty() {
        println!(
            "{}",
            paint(
                snap.theme.warning_color(),
                &format!("Auto-corrected: {}", corrections.join(", ")),
                color_on
            )
        );
    }
    let parsed = parse_short_syntax(&corrected_raw, &projects, &tags, None, Vec::new());

    let mut patch = serde_json::Map::new();
    let final_title = if parsed.title.is_empty() {
        task.title.clone()
    } else {
        parsed.title.clone()
    };
    patch.insert("title".to_string(), json!(final_title));
    if let Some(pid) = &parsed.project_id {
        patch.insert("projectId".to_string(), json!(pid));
    }
    let mut merged_tags = task.tag_ids.clone();
    if !parsed.tag_ids.is_empty() {
        for tid in &parsed.tag_ids {
            if !merged_tags.contains(tid) {
                merged_tags.push(tid.clone());
            }
        }
        patch.insert("tagIds".to_string(), json!(merged_tags));
    }
    if let Some(due) = &parsed.due_day {
        patch.insert("dueDay".to_string(), json!(due));
    }
    if let Some(dl) = &parsed.deadline_day {
        patch.insert("deadlineDay".to_string(), json!(dl));
    }
    if let Some(ts) = parsed.time_spent {
        patch.insert("timeSpent".to_string(), json!(ts));
    }
    if let Some(te) = parsed.time_estimate {
        patch.insert("timeEstimate".to_string(), json!(te));
    }

    if let Some(t) = snap.cache.tasks.iter_mut().find(|t| t.id == task.id) {
        t.title = final_title.clone();
        if let Some(pid) = parsed.project_id {
            t.project_id = Some(pid);
        }
        t.tag_ids = merged_tags;
        if let Some(due) = parsed.due_day {
            t.due_day = Some(due);
        }
        if let Some(dl) = parsed.deadline_day {
            t.deadline_day = Some(dl);
        }
        if let Some(ts) = parsed.time_spent {
            t.time_spent = Some(ts);
        }
        if let Some(te) = parsed.time_estimate {
            t.time_estimate = Some(te);
        }
    }

    apply_patch(
        &mut snap,
        &task.id,
        serde_json::Value::Object(patch),
        json_out,
        &format!("✎ Updated: {final_title} ({})", task.id),
        color_on,
    )
    .await
}

pub fn parse_schedule_when(when: &str) -> Result<Option<String>> {
    let today = Local::now().date_naive();
    match when.to_lowercase().as_str() {
        "today" => Ok(Some(today.format("%Y-%m-%d").to_string())),
        "tomorrow" | "tmr" => Ok(Some(
            (today + ChronoDuration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        )),
        "next-week" | "next_week" | "nextweek" => Ok(Some(
            (today + ChronoDuration::days(7))
                .format("%Y-%m-%d")
                .to_string(),
        )),
        "clear" | "none" | "unschedule" => Ok(None),
        other => {
            NaiveDate::parse_from_str(other, "%Y-%m-%d").with_context(|| {
                format!(
                    "Invalid date '{other}'. Use today, tomorrow, next-week, clear, or YYYY-MM-DD."
                )
            })?;
            Ok(Some(other.to_string()))
        }
    }
}

async fn cmd_schedule(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let remind = flag_any_value(args, &["--remind", "-r"]);
    let pos = positional_args(args, &["--remind", "-r"]);
    if pos.len() < 2 {
        bail!(
            "Usage: spt schedule <id|pattern|query> <today|tomorrow|next-week|YYYY-MM-DD|clear> [-r|--remind HH:MM]"
        );
    }
    let due_day = parse_schedule_when(&pos[1])?;
    let remind_at = if let (Some(day), Some(time_str)) = (&due_day, &remind) {
        let d = NaiveDate::parse_from_str(day, "%Y-%m-%d")?;
        let t = NaiveTime::parse_from_str(time_str, "%H:%M")
            .with_context(|| format!("Invalid time '{time_str}', expected HH:MM"))?;
        d.and_time(t)
            .and_local_timezone(Local)
            .single()
            .map(|ldt| ldt.timestamp_millis().max(0) as u64)
    } else {
        None
    };

    let mut snap = load_snapshot(true).await;
    let task = resolve_task(&snap.cache.tasks, &pos[0])?.clone();
    if let Some(t) = snap.cache.tasks.iter_mut().find(|t| t.id == task.id) {
        t.due_day = due_day.clone();
        if let Some(ms) = remind_at {
            t.remind_at = Some(ms);
        }
    }
    let patch = if let Some(ms) = remind_at {
        json!({ "dueDay": due_day, "remindAt": ms })
    } else {
        json!({ "dueDay": due_day })
    };
    let label = due_day.as_deref().unwrap_or("unscheduled");
    apply_patch(
        &mut snap,
        &task.id,
        patch,
        json_out,
        &format!("📅 Scheduled '{}' -> {label}", task.title),
        color_on,
    )
    .await
}

async fn cmd_rm(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let pos = positional_args(args, &[]);
    let query = pos.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("Usage: spt rm <id|pattern|query> [-j|--json]");
    }
    let mut snap = load_snapshot(true).await;
    let task = resolve_task(&snap.cache.tasks, &query)?.clone();
    snap.cache.tasks.retain(|t| t.id != task.id);

    if snap.online
        && !task.id.starts_with("offline-task-")
        && snap.client.delete_task(&task.id).await.is_ok()
    {
        let _ = snap.cache.save();
        if json_out {
            println!("{}", json!({ "deleted": task.id }));
        } else {
            println!(
                "{} {} {}",
                paint_bold(snap.theme.danger_color(), "✗ Deleted", color_on),
                paint(snap.theme.fg_color(), &task.title, color_on),
                paint(
                    snap.theme.muted_color(),
                    &format!("({})", task.id),
                    color_on
                )
            );
        }
        return Ok(());
    }

    snap.cache.pending_ops.push(QueuedOp::DeleteTask {
        id: task.id.clone(),
    });
    snap.cache.save()?;
    if json_out {
        println!("{}", json!({ "deleted": task.id, "queued": true }));
    } else {
        println!(
            "{} {} {} (queued offline)",
            paint_bold(snap.theme.warning_color(), "✗ Deleted", color_on),
            paint(snap.theme.fg_color(), &task.title, color_on),
            paint(
                snap.theme.muted_color(),
                &format!("({})", task.id),
                color_on
            )
        );
    }
    Ok(())
}

async fn cmd_start(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let pos = positional_args(args, &[]);
    let query = pos.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("Usage: spt start <id|pattern|query> [-j|--json]");
    }
    let mut snap = load_snapshot(true).await;
    let task = resolve_task(&snap.cache.tasks, &query)?.clone();
    snap.cache.current_task_id = Some(task.id.clone());

    if snap.online
        && !task.id.starts_with("offline-task-")
        && snap.client.set_current_task(Some(&task.id)).await.is_ok()
    {
        let _ = snap.cache.save();
        if json_out {
            println!("{}", json!({ "tracking": task.id, "title": task.title }));
        } else {
            println!(
                "{} {} {}",
                paint_bold(snap.theme.success_color(), "▶ Tracking", color_on),
                paint(snap.theme.fg_color(), &task.title, color_on),
                paint(
                    snap.theme.muted_color(),
                    &format!("({})", task.id),
                    color_on
                )
            );
        }
        return Ok(());
    }

    snap.cache.pending_ops.push(QueuedOp::SetCurrentTask {
        task_id: Some(task.id.clone()),
    });
    snap.cache.save()?;
    if json_out {
        println!(
            "{}",
            json!({ "tracking": task.id, "title": task.title, "queued": true })
        );
    } else {
        println!(
            "{} {} {} (queued offline)",
            paint_bold(snap.theme.warning_color(), "▶ Tracking", color_on),
            paint(snap.theme.fg_color(), &task.title, color_on),
            paint(
                snap.theme.muted_color(),
                &format!("({})", task.id),
                color_on
            )
        );
    }
    Ok(())
}

async fn cmd_stop(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let mut snap = load_snapshot(true).await;
    snap.cache.current_task_id = None;

    if snap.online && snap.client.set_current_task(None).await.is_ok() {
        let _ = snap.cache.save();
        if json_out {
            println!("{}", json!({ "tracking": null }));
        } else {
            println!(
                "{}",
                paint(
                    snap.theme.muted_color(),
                    "■ Stopped active task timer.",
                    color_on
                )
            );
        }
        return Ok(());
    }

    snap.cache
        .pending_ops
        .push(QueuedOp::SetCurrentTask { task_id: None });
    snap.cache.save()?;
    if json_out {
        println!("{}", json!({ "tracking": null, "queued": true }));
    } else {
        println!(
            "{}",
            paint(
                snap.theme.warning_color(),
                "■ Stopped active task timer (queued offline).",
                color_on
            )
        );
    }
    Ok(())
}

async fn cmd_log(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let pos = positional_args(args, &[]);
    if pos.len() < 2 {
        bail!("Usage: spt log <id|pattern|query> <duration> (e.g. 25m, 1h30m)");
    }
    let add_ms = parse_duration_ms(&pos[1])
        .ok_or_else(|| anyhow!("Invalid duration '{}'. Use e.g. 25m, 1h30m.", pos[1]))?;
    let mut snap = load_snapshot(true).await;
    let task = resolve_task(&snap.cache.tasks, &pos[0])?.clone();
    let new_spent = task.time_spent.unwrap_or(0).saturating_add(add_ms);
    if let Some(t) = snap.cache.tasks.iter_mut().find(|t| t.id == task.id) {
        t.time_spent = Some(new_spent);
    }
    apply_patch(
        &mut snap,
        &task.id,
        json!({ "timeSpent": new_spent }),
        json_out,
        &format!(
            "⏱ Logged +{} on '{}' (total {})",
            format_ms(add_ms),
            task.title,
            format_ms(new_spent)
        ),
        color_on,
    )
    .await
}

async fn cmd_status(args: &[String], color_on: bool) -> Result<()> {
    let waybar = has_any_flag(args, &["--waybar", "-w"]);
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let snap = load_snapshot(true).await;
    let today = Local::now().format("%Y-%m-%d").to_string();

    let today_remaining = snap
        .cache
        .tasks
        .iter()
        .filter(|t| t.parent_id.is_none() && !t.is_done && t.due_day.as_deref() == Some(&today))
        .count();
    let overdue_deadlines = snap
        .cache
        .tasks
        .iter()
        .filter(|t| {
            t.parent_id.is_none()
                && !t.is_done
                && t.deadline_day
                    .as_deref()
                    .is_some_and(|d| d <= today.as_str())
        })
        .count();
    let current = snap
        .cache
        .current_task_id
        .as_deref()
        .and_then(|id| snap.cache.tasks.iter().find(|t| t.id == id));

    if waybar {
        let class = if !snap.online {
            "offline"
        } else if current.is_some() {
            "tracking"
        } else {
            "idle"
        };
        let text = match current {
            Some(t) => format!("▶ {} ({today_remaining})", t.title),
            None => format!("✓ {today_remaining} today"),
        };
        let tooltip = format!(
            "Today: {today_remaining} remaining\nDeadlines: {overdue_deadlines}\nPending sync: {}\nStatus: {}",
            snap.cache.pending_ops.len(),
            if snap.online { "Online" } else { "Offline" }
        );
        println!(
            "{}",
            json!({ "text": text, "tooltip": tooltip, "class": class })
        );
        return Ok(());
    }

    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "online": snap.online,
                "current_task": current.map(|t| json!({ "id": t.id, "title": t.title, "time_spent": t.time_spent })),
                "today_remaining": today_remaining,
                "deadlines": overdue_deadlines,
                "pending_ops": snap.cache.pending_ops.len(),
            }))?
        );
        return Ok(());
    }

    let conn_dot = if snap.online {
        paint(snap.theme.success_color(), "●", color_on)
    } else {
        paint(snap.theme.warning_color(), "○ offline", color_on)
    };
    match current {
        Some(t) => println!(
            "{} | {} | {conn_dot}",
            paint_bold(
                snap.theme.success_color(),
                &format!("▶ {}", t.title),
                color_on
            ),
            paint(
                snap.theme.accent_color(),
                &format!("{today_remaining} today"),
                color_on
            )
        ),
        None => println!(
            "{} | {conn_dot}",
            paint(
                snap.theme.accent_color(),
                &format!("{today_remaining} today"),
                color_on
            )
        ),
    }
    Ok(())
}

async fn cmd_current(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let snap = load_snapshot(true).await;
    let current = snap
        .cache
        .current_task_id
        .as_deref()
        .and_then(|id| snap.cache.tasks.iter().find(|t| t.id == id));
    match current {
        Some(t) => {
            if json_out {
                println!("{}", serde_json::to_string_pretty(t)?);
            } else {
                println!(
                    "{} {} {}",
                    paint_bold(snap.theme.success_color(), &t.title, color_on),
                    paint(snap.theme.muted_color(), &format!("({})", t.id), color_on),
                    paint(
                        snap.theme.accent_color(),
                        &format!("[{}]", format_ms(t.time_spent.unwrap_or(0))),
                        color_on
                    )
                );
            }
            Ok(())
        }
        None => {
            if json_out {
                println!("null");
            }
            std::process::exit(1);
        }
    }
}

async fn cmd_projects(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&projects)?);
    } else {
        println!(
            "{}",
            paint_bold(
                snap.theme.accent_color(),
                &format!("Projects ({})", projects.len()),
                color_on
            )
        );
        for (i, p) in projects.iter().enumerate() {
            let branch = if i + 1 == projects.len() {
                "╰─"
            } else {
                "├─"
            };
            let rgb = p.display_color();
            println!(
                "{} {} {}",
                paint(snap.theme.border_color(), branch, color_on),
                paint_bold(rgb, &p.title, color_on),
                paint(snap.theme.muted_color(), &format!("({})", p.id), color_on)
            );
        }
    }
    Ok(())
}

async fn cmd_tags(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let snap = load_snapshot(true).await;
    let tags = all_tags(&snap.cache);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&tags)?);
    } else {
        println!(
            "{}",
            paint_bold(
                snap.theme.accent_color(),
                &format!("Tags ({})", tags.len()),
                color_on
            )
        );
        for (i, t) in tags.iter().enumerate() {
            let branch = if i + 1 == tags.len() {
                "╰─"
            } else {
                "├─"
            };
            let rgb = t.display_color();
            println!(
                "{} {} {}",
                paint(snap.theme.border_color(), branch, color_on),
                paint_bold(rgb, &format!("#{}", t.title), color_on),
                paint(snap.theme.muted_color(), &format!("({})", t.id), color_on)
            );
        }
    }
    Ok(())
}

fn cmd_counter(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let pos = positional_args(args, &[]);
    let mut config = AppConfig::load();
    let theme = config.theme.clone();
    let mut cache = OfflineCache::load();
    let mut counters = if cache.counters.is_empty() {
        config.counters.clone()
    } else {
        cache.counters.clone()
    };

    let action = pos.first().map(|s| s.as_str()).unwrap_or("list");
    if action == "list" {
        if json_out {
            println!("{}", serde_json::to_string_pretty(&counters)?);
        } else {
            for c in &counters {
                println!(
                    "{}: {} {}",
                    paint(theme.fg_color(), &c.title, color_on),
                    paint_bold(theme.accent_color(), &c.count.to_string(), color_on),
                    paint(theme.muted_color(), &format!("({})", c.id), color_on)
                );
            }
        }
        return Ok(());
    }

    let query = pos
        .get(1)
        .ok_or_else(|| anyhow!("Usage: spt counter <inc|dec|set> <name_or_id> [value]"))?;
    let q_low = query.to_lowercase();
    let c = counters
        .iter_mut()
        .find(|c| c.id == *query || c.title.to_lowercase().contains(&q_low))
        .ok_or_else(|| anyhow!("No counter matching '{query}'."))?;

    match action {
        "inc" => c.count += 1,
        "dec" => c.count = (c.count - 1).max(0),
        "set" => {
            let val: i64 = pos
                .get(2)
                .ok_or_else(|| anyhow!("Missing value for `set`"))?
                .parse()?;
            c.count = val.max(0);
        }
        _ => bail!("Unknown counter action '{action}'. Use list, inc, dec, or set."),
    }

    let updated = c.clone();
    config.counters = counters.clone();
    cache.counters = counters;
    let _ = config.save();
    let _ = cache.save();

    if json_out {
        println!("{}", serde_json::to_string_pretty(&updated)?);
    } else {
        println!(
            "{}: {}",
            paint(theme.fg_color(), &updated.title, color_on),
            paint_bold(theme.success_color(), &updated.count.to_string(), color_on)
        );
    }
    Ok(())
}

async fn cmd_summary(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let snap = load_snapshot(true).await;
    let today = Local::now().format("%Y-%m-%d").to_string();
    let start_of_today = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_local_timezone(Local).single())
        .map(|dt| dt.timestamp_millis() as u64)
        .unwrap_or(0);

    let completed: Vec<&Task> = snap
        .cache
        .tasks
        .iter()
        .filter(|t| {
            t.parent_id.is_none()
                && t.is_done
                && (t.due_day.as_deref() == Some(today.as_str())
                    || t.done_on.is_some_and(|ms| ms >= start_of_today))
        })
        .collect();
    let unfinished: Vec<&Task> = snap
        .cache
        .tasks
        .iter()
        .filter(|t| {
            t.parent_id.is_none() && !t.is_done && t.due_day.as_deref() == Some(today.as_str())
        })
        .collect();
    let focus_ms: u64 = completed
        .iter()
        .chain(unfinished.iter())
        .filter_map(|t| t.time_spent)
        .sum();

    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "date": today,
                "completed_count": completed.len(),
                "unfinished_count": unfinished.len(),
                "focus_ms": focus_ms,
                "focus_formatted": format_ms(focus_ms),
            }))?
        );
    } else {
        println!(
            "{}",
            paint_bold(
                snap.theme.accent_color(),
                &format!("Daily Summary ({today})"),
                color_on
            )
        );
        println!(
            "├─ {} completed  |  {} unfinished  |  {} focus time",
            paint_bold(
                snap.theme.success_color(),
                &completed.len().to_string(),
                color_on
            ),
            paint_bold(
                snap.theme.warning_color(),
                &unfinished.len().to_string(),
                color_on
            ),
            paint_bold(snap.theme.accent_color(), &format_ms(focus_ms), color_on)
        );
    }
    Ok(())
}

async fn cmd_worklog(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let by = flag_any_value(args, &["--by", "-b"]).unwrap_or_else(|| "project".to_string());
    let days = if has_any_flag(args, &["--month", "-m"]) {
        30
    } else if has_any_flag(args, &["--week", "-w"]) {
        7
    } else {
        1
    };
    let cutoff = (Local::now() - ChronoDuration::days(days - 1))
        .format("%Y-%m-%d")
        .to_string();

    let snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    let tags = all_tags(&snap.cache);
    let mut totals: HashMap<String, u64> = HashMap::new();

    for t in &snap.cache.tasks {
        let spent = t.time_spent.unwrap_or(0);
        if spent == 0 {
            continue;
        }
        if let Some(d) = &t.due_day
            && d.as_str() < cutoff.as_str()
        {
            continue;
        }
        if by == "tag" {
            if t.tag_ids.is_empty() {
                *totals.entry("(Untagged)".to_string()).or_default() += spent;
            } else {
                for tid in &t.tag_ids {
                    let name = tags
                        .iter()
                        .find(|tg| &tg.id == tid)
                        .map(|tg| tg.title.clone())
                        .unwrap_or_else(|| tid.clone());
                    *totals.entry(name).or_default() += spent;
                }
            }
        } else {
            let name = t
                .project_id
                .as_ref()
                .and_then(|pid| projects.iter().find(|p| &p.id == pid))
                .map(|p| p.title.clone())
                .unwrap_or_else(|| "Inbox".to_string());
            *totals.entry(name).or_default() += spent;
        }
    }

    let mut rows: Vec<(String, u64)> = totals.into_iter().collect();
    rows.sort_by_key(|b| std::cmp::Reverse(b.1));

    if json_out {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else if rows.is_empty() {
        println!(
            "{}",
            paint(
                snap.theme.muted_color(),
                "No time logged in this period.",
                color_on
            )
        );
    } else {
        println!(
            "{}",
            paint_bold(
                snap.theme.accent_color(),
                &format!("Worklog (by {by})"),
                color_on
            )
        );
        for (i, (name, ms)) in rows.iter().enumerate() {
            let branch = if i + 1 == rows.len() {
                "╰─"
            } else {
                "├─"
            };
            println!(
                "{} {:<24} {}",
                paint(snap.theme.border_color(), branch, color_on),
                paint(snap.theme.fg_color(), name, color_on),
                paint_bold(snap.theme.success_color(), &format_ms(*ms), color_on)
            );
        }
    }
    Ok(())
}

async fn cmd_sync(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let config = AppConfig::load();
    let client = SuperProductivityClient::new(&config.api_url, &config.api_token);
    let mut cache = OfflineCache::load();

    let health = client.health().await.context("Failed to reach SP API")?;
    if health.server != "up" || !health.renderer_ready {
        bail!(
            "SP API not ready (server={}, ready={})",
            health.server,
            health.renderer_ready
        );
    }

    let ops = std::mem::take(&mut cache.pending_ops);
    let mut replayed = 0usize;
    let mut remaining = Vec::new();
    let mut task_id_map: HashMap<String, String> = HashMap::new();
    let mut deleted_ids: HashSet<String> = HashSet::new();

    for op in ops {
        let ok = match &op {
            QueuedOp::CreateTask {
                temp_id, payload, ..
            } => {
                let clean = sanitize_create_payload_for_sp(
                    payload.clone(),
                    &task_id_map,
                    &cache.project_id_map,
                    &cache.tag_id_map,
                );
                if let Ok(created) = client.create_task(clean).await {
                    task_id_map.insert(temp_id.clone(), created.id);
                    true
                } else {
                    false
                }
            }
            QueuedOp::UpdateTask { id, patch } => {
                let real_id = task_id_map.get(id).unwrap_or(id);
                if deleted_ids.contains(real_id) {
                    true
                } else {
                    client.update_task(real_id, patch.clone()).await.is_ok()
                }
            }
            QueuedOp::DeleteTask { id } => {
                let real_id = task_id_map.get(id).unwrap_or(id);
                if client.delete_task(real_id).await.is_ok() {
                    deleted_ids.insert(real_id.clone());
                    true
                } else {
                    false
                }
            }
            QueuedOp::SetCurrentTask { task_id } => {
                let real = task_id
                    .as_ref()
                    .map(|id| task_id_map.get(id).unwrap_or(id).as_str());
                client.set_current_task(real).await.is_ok()
            }
        };
        if ok {
            replayed += 1;
        } else {
            remaining.push(op);
        }
    }

    cache.pending_ops = remaining;
    cache.tasks = client.get_tasks().await?;
    if let Ok(p) = client.get_projects().await {
        cache.projects = p;
    }
    if let Ok(t) = client.get_tags().await {
        cache.tags = t;
    }
    cache.save()?;

    if json_out {
        println!(
            "{}",
            json!({
                "replayed": replayed,
                "remaining": cache.pending_ops.len(),
                "tasks": cache.tasks.len(),
            })
        );
    } else {
        println!(
            "{} {} tasks ({replayed} replayed, {} pending)",
            paint_bold(config.theme.success_color(), "✓ Synced:", color_on),
            cache.tasks.len(),
            cache.pending_ops.len()
        );
    }
    Ok(())
}

async fn cmd_doctor(args: &[String], color_on: bool) -> Result<()> {
    let json_out = has_any_flag(args, &["--json", "-j"]);
    let config = AppConfig::load();
    let client = SuperProductivityClient::new(&config.api_url, &config.api_token);
    let cache = OfflineCache::load();
    let api_ok = matches!(client.health().await, Ok(h) if h.server == "up" && h.renderer_ready);
    let bridge_ok = config.bridge_port > 0
        && TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", config.bridge_port).parse()?,
            Duration::from_millis(300),
        )
        .is_ok();

    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "api_url": config.api_url,
                "api_online": api_ok,
                "bridge_port": config.bridge_port,
                "bridge_listening": bridge_ok,
                "config_path": AppConfig::config_path(),
                "cache_path": OfflineCache::cache_path(),
                "cached_tasks": cache.tasks.len(),
                "pending_ops": cache.pending_ops.len(),
            }))?
        );
    } else {
        println!(
            "{}",
            paint_bold(config.theme.accent_color(), "spt doctor", color_on)
        );
        println!(
            "├─ API ({}): {}",
            config.api_url,
            if api_ok {
                paint_bold(config.theme.success_color(), "OK", color_on)
            } else {
                paint_bold(config.theme.danger_color(), "OFFLINE", color_on)
            }
        );
        println!(
            "├─ Bridge (127.0.0.1:{}): {}",
            config.bridge_port,
            if bridge_ok {
                paint_bold(config.theme.success_color(), "LISTENING", color_on)
            } else {
                paint(config.theme.warning_color(), "NOT RUNNING", color_on)
            }
        );
        println!(
            "├─ Config: {}",
            paint(
                config.theme.muted_color(),
                &AppConfig::config_path().display().to_string(),
                color_on
            )
        );
        println!(
            "╰─ Cache:  {} ({} tasks, {} pending ops)",
            paint(
                config.theme.muted_color(),
                &OfflineCache::cache_path().display().to_string(),
                color_on
            ),
            cache.tasks.len(),
            cache.pending_ops.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(id: &str, title: &str) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            is_done: false,
            notes: None,
            project_id: None,
            parent_id: None,
            tag_ids: vec![],
            due_day: None,
            deadline_day: None,
            remind_at: None,
            done_on: None,
            time_estimate: None,
            time_spent: None,
            repeat_cfg_id: None,
        }
    }

    #[test]
    fn resolves_task_by_id_dot_patterns_and_title() {
        let tasks = vec![
            sample_task("G2P4Yuklt0bNZdtJPMk_g", "Write docs"),
            sample_task("def456xyz", "Fix login bug"),
        ];
        assert_eq!(
            resolve_task(&tasks, "G2.").unwrap().id,
            "G2P4Yuklt0bNZdtJPMk_g"
        );
        assert_eq!(
            resolve_task(&tasks, "._g").unwrap().id,
            "G2P4Yuklt0bNZdtJPMk_g"
        );
        assert_eq!(
            resolve_task(&tasks, "G2.k_g").unwrap().id,
            "G2P4Yuklt0bNZdtJPMk_g"
        );
        assert_eq!(
            resolve_task(&tasks, ".Yuk.").unwrap().id,
            "G2P4Yuklt0bNZdtJPMk_g"
        );
        assert_eq!(resolve_task(&tasks, "def").unwrap().id, "def456xyz");
        assert_eq!(resolve_task(&tasks, "login").unwrap().id, "def456xyz");
        assert!(resolve_task(&tasks, "G2.zzz").is_err());
    }

    #[test]
    fn autocorrects_typos_in_tags_projects_and_dates() {
        let projects = vec![Project {
            id: "p1".into(),
            title: "Work".into(),
            theme: None,
            color: None,
        }];
        let tags = vec![Tag {
            id: "t1".into(),
            title: "priority".into(),
            color: None,
            theme: None,
        }];
        let (fixed, notes) =
            autocorrect_short_syntax("quite #prioriyt +Wrok @tomorow !todqy", &projects, &tags);
        assert_eq!(fixed, "quite #priority +Work @tomorrow !today");
        assert_eq!(notes.len(), 4);
    }

    #[test]
    fn parses_schedule_when_keywords_and_dates() {
        assert!(parse_schedule_when("today").unwrap().is_some());
        assert!(parse_schedule_when("tomorrow").unwrap().is_some());
        assert!(parse_schedule_when("next-week").unwrap().is_some());
        assert_eq!(parse_schedule_when("clear").unwrap(), None);
        assert_eq!(
            parse_schedule_when("2026-12-31").unwrap().as_deref(),
            Some("2026-12-31")
        );
        assert!(parse_schedule_when("invalid-date").is_err());
    }

    #[test]
    fn positional_args_skips_short_and_long_value_flags() {
        let args: Vec<String> = vec!["add", "-t", "-n", "hello", "Buy", "milk", "-j"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            positional_args(&args[1..], &["--note", "-n", "--parent", "-P"]),
            vec!["Buy", "milk"]
        );
    }
}
