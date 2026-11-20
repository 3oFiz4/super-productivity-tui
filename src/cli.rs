use crate::api::SuperProductivityClient;
use crate::config::AppConfig;
use crate::models::{CreateTaskPayload, Task};
use crate::offline::{OfflineCache, QueuedOp, sanitize_create_payload_for_sp};
use crate::short_syntax::{parse_duration_ms, parse_short_syntax};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{Duration as ChronoDuration, Local, NaiveDate, NaiveTime};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::net::TcpStream;
use std::time::Duration;

pub async fn run(args: Vec<String>) -> Result<()> {
    let Some((cmd, rest)) = args.split_first() else {
        print_help();
        return Ok(());
    };

    match cmd.as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            Ok(())
        }
        "-V" | "--version" | "version" => {
            println!("spt {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "add" => cmd_add(rest).await,
        "ls" | "list" => cmd_ls(rest).await,
        "done" => cmd_done(rest, false).await,
        "undo-done" | "reopen" => cmd_done(rest, true).await,
        "edit" => cmd_edit(rest).await,
        "schedule" | "sched" => cmd_schedule(rest).await,
        "rm" | "delete" => cmd_rm(rest).await,
        "start" => cmd_start(rest).await,
        "stop" => cmd_stop(rest).await,
        "log" => cmd_log(rest).await,
        "status" => cmd_status(rest).await,
        "current" => cmd_current(rest).await,
        "projects" => cmd_projects(rest).await,
        "tags" => cmd_tags(rest).await,
        "counter" | "counters" => cmd_counter(rest),
        "summary" => cmd_summary(rest).await,
        "worklog" => cmd_worklog(rest).await,
        "sync" => cmd_sync(rest).await,
        "doctor" => cmd_doctor(rest).await,
        other => bail!("Unknown subcommand '{other}'. Run `spt --help` for usage."),
    }
}

fn print_help() {
    println!(
        r#"spt {} — Super Productivity TUI & CLI

USAGE:
    spt                     Launch interactive TUI
    spt <COMMAND> [ARGS]    Run non-interactive CLI command

COMMANDS:
    add <title...>          Add task using short syntax (+proj #tag t1h @due !deadline)
                            Flags: --top, --note <text>, --parent <id>, --json
    ls [view]               List tasks (views: all, today, inbox, planned, overdue)
                            Flags: --project <p>, --tag <t>, --search <q>, --done, --limit <n>, --json
    done <id|query>         Mark task done (--undo to reopen, --json)
    undo-done <id|query>    Reopen a completed task (--json)
    edit <id|query> <...>   Edit task inline with short syntax (--json)
    schedule <id|q> <when>  Schedule task (today, tomorrow, next-week, YYYY-MM-DD, clear)
                            Flags: --remind <HH:MM>, --json
    rm <id|query>           Delete a task (--json)
    start <id|query>        Start timer on task (--json)
    stop                    Stop active task timer (--json)
    log <id|query> <dur>    Add time spent (e.g. 25m, 1h30m) (--json)
    status                  Single-line status (--waybar, --tmux, --json)
    current                 Show currently tracked task (exits 1 if idle) (--json)
    projects                List projects (--json)
    tags                    List tags (--json)
    counter [act] [name]    Manage counters: list, inc, dec, set <name> [val] (--json)
    summary [today]         Show today's completed/unfinished summary (--json)
    worklog                 Show time-tracking breakdown (--day|--week|--month, --by project|tag, --json)
    sync                    Flush offline queue and refresh local cache (--json)
    doctor                  Check API, companion bridge, config, and cache health (--json)"#,
        env!("CARGO_PKG_VERSION")
    );
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
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
        if a.starts_with("--") {
            continue;
        }
        out.push(a.clone());
    }
    out
}

struct Snapshot {
    client: SuperProductivityClient,
    cache: OfflineCache,
    online: bool,
}

async fn load_snapshot(refresh: bool) -> Snapshot {
    let config = AppConfig::load();
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
        client,
        cache,
        online,
    }
}

fn all_projects(cache: &OfflineCache) -> Vec<crate::models::Project> {
    let mut merged = cache.projects.clone();
    for lp in &cache.local_projects {
        if !merged.iter().any(|p| p.id == lp.id) {
            merged.push(lp.clone());
        }
    }
    merged
}

fn all_tags(cache: &OfflineCache) -> Vec<crate::models::Tag> {
    let mut merged = cache.tags.clone();
    for lt in &cache.local_tags {
        if !merged.iter().any(|t| t.id == lt.id) {
            merged.push(lt.clone());
        }
    }
    merged
}

pub fn resolve_task<'a>(tasks: &'a [Task], query: &str) -> Result<&'a Task> {
    if let Some(exact) = tasks.iter().find(|t| t.id == query) {
        return Ok(exact);
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
                "Ambiguous query '{query}' matched {n} tasks. Use a task ID:\n{}",
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

async fn cmd_add(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let to_top = has_flag(args, "--top");
    let note = flag_value(args, "--note");
    let parent_id = flag_value(args, "--parent");
    let pos = positional_args(args, &["--note", "--parent"]);
    let raw = pos.join(" ").trim().to_string();
    if raw.is_empty() {
        bail!("Usage: spt add \"<title + short syntax>\" [--top] [--note <text>] [--parent <id>]");
    }

    let mut snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    let tags = all_tags(&snap.cache);
    let parsed = parse_short_syntax(&raw, &projects, &tags, None, Vec::new());
    let payload = CreateTaskPayload {
        title: if parsed.title.is_empty() {
            raw.clone()
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
                println!("Created [{}] {}", created.id, created.title);
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
        println!("Queued offline [{}] {}", task.id, task.title);
    }
    Ok(())
}

async fn cmd_ls(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let include_done = has_flag(args, "--done");
    let proj_filter = flag_value(args, "--project").map(|s| s.to_lowercase());
    let tag_filter = flag_value(args, "--tag").map(|s| s.to_lowercase());
    let search_filter = flag_value(args, "--search").map(|s| s.to_lowercase());
    let limit = flag_value(args, "--limit").and_then(|s| s.parse::<usize>().ok());
    let pos = positional_args(args, &["--project", "--tag", "--search", "--limit"]);
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
            "all" => true,
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
        println!("No tasks found.");
        return Ok(());
    }

    for t in filtered {
        let check = if t.is_done { "[x]" } else { "[ ]" };
        let mut meta = Vec::new();
        if let Some(pid) = &t.project_id
            && let Some(p) = projects.iter().find(|p| &p.id == pid)
        {
            meta.push(format!("+{}", p.title));
        }
        for tid in &t.tag_ids {
            if let Some(tag) = tags.iter().find(|tg| &tg.id == tid) {
                meta.push(format!("#{}", tag.title));
            }
        }
        if let Some(due) = &t.due_day {
            meta.push(format!("@{due}"));
        }
        if let Some(dl) = &t.deadline_day {
            meta.push(format!("!{dl}"));
        }
        let meta_str = if meta.is_empty() {
            String::new()
        } else {
            format!("  {}", meta.join(" "))
        };
        println!("{check} {}  ({}){meta_str}", t.title, t.id);
    }
    Ok(())
}

async fn apply_patch(
    snap: &mut Snapshot,
    task_id: &str,
    patch: serde_json::Value,
    json_out: bool,
    human_msg: &str,
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
            println!("{human_msg}");
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
        println!("{human_msg} (queued offline)");
    }
    Ok(())
}

async fn cmd_done(args: &[String], force_undo: bool) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let undo = force_undo || has_flag(args, "--undo");
    let pos = positional_args(args, &[]);
    let query = pos.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("Usage: spt done <id|query> [--undo] [--json]");
    }
    let mut snap = load_snapshot(true).await;
    let task = resolve_task(&snap.cache.tasks, &query)?.clone();
    let target_done = !undo;
    if let Some(t) = snap.cache.tasks.iter_mut().find(|t| t.id == task.id) {
        t.is_done = target_done;
    }
    let verb = if target_done { "Completed" } else { "Reopened" };
    apply_patch(
        &mut snap,
        &task.id,
        json!({ "isDone": target_done }),
        json_out,
        &format!("{verb}: {} ({})", task.title, task.id),
    )
    .await
}

async fn cmd_edit(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let pos = positional_args(args, &[]);
    if pos.len() < 2 {
        bail!("Usage: spt edit <id|query> \"<title + short syntax>\" [--json]");
    }
    let query = &pos[0];
    let raw = pos[1..].join(" ").trim().to_string();
    let mut snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    let tags = all_tags(&snap.cache);
    let task = resolve_task(&snap.cache.tasks, query)?.clone();
    let parsed = parse_short_syntax(&raw, &projects, &tags, None, Vec::new());

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
        &format!("Updated: {final_title} ({})", task.id),
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

async fn cmd_schedule(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let remind = flag_value(args, "--remind");
    let pos = positional_args(args, &["--remind"]);
    if pos.len() < 2 {
        bail!(
            "Usage: spt schedule <id|query> <today|tomorrow|next-week|YYYY-MM-DD|clear> [--remind HH:MM]"
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
        &format!("Scheduled '{}' -> {label}", task.title),
    )
    .await
}

async fn cmd_rm(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let pos = positional_args(args, &[]);
    let query = pos.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("Usage: spt rm <id|query> [--json]");
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
            println!("Deleted: {} ({})", task.title, task.id);
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
        println!("Deleted: {} ({}) (queued offline)", task.title, task.id);
    }
    Ok(())
}

async fn cmd_start(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let pos = positional_args(args, &[]);
    let query = pos.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("Usage: spt start <id|query> [--json]");
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
            println!("Started timer: {} ({})", task.title, task.id);
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
            "Started timer: {} ({}) (queued offline)",
            task.title, task.id
        );
    }
    Ok(())
}

async fn cmd_stop(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let mut snap = load_snapshot(true).await;
    snap.cache.current_task_id = None;

    if snap.online && snap.client.set_current_task(None).await.is_ok() {
        let _ = snap.cache.save();
        if json_out {
            println!("{}", json!({ "tracking": null }));
        } else {
            println!("Stopped active task timer.");
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
        println!("Stopped active task timer (queued offline).");
    }
    Ok(())
}

async fn cmd_log(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let pos = positional_args(args, &[]);
    if pos.len() < 2 {
        bail!("Usage: spt log <id|query> <duration> (e.g. 25m, 1h30m)");
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
            "Logged +{} on '{}' (total {})",
            format_ms(add_ms),
            task.title,
            format_ms(new_spent)
        ),
    )
    .await
}

async fn cmd_status(args: &[String]) -> Result<()> {
    let waybar = has_flag(args, "--waybar");
    let json_out = has_flag(args, "--json");
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

    let conn_dot = if snap.online { "●" } else { "○ offline" };
    match current {
        Some(t) => println!("▶ {} | {today_remaining} today | {conn_dot}", t.title),
        None => println!("{today_remaining} today | {conn_dot}"),
    }
    Ok(())
}

async fn cmd_current(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
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
                    "{} ({}) [{}]",
                    t.title,
                    t.id,
                    format_ms(t.time_spent.unwrap_or(0))
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

async fn cmd_projects(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let snap = load_snapshot(true).await;
    let projects = all_projects(&snap.cache);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&projects)?);
    } else {
        for p in projects {
            println!("{}  ({})", p.title, p.id);
        }
    }
    Ok(())
}

async fn cmd_tags(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let snap = load_snapshot(true).await;
    let tags = all_tags(&snap.cache);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&tags)?);
    } else {
        for t in tags {
            println!("#{}  ({})", t.title, t.id);
        }
    }
    Ok(())
}

fn cmd_counter(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let pos = positional_args(args, &[]);
    let mut config = AppConfig::load();
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
                println!("{}: {}  ({})", c.title, c.count, c.id);
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
        println!("{}: {}", updated.title, updated.count);
    }
    Ok(())
}

async fn cmd_summary(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
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
            "Daily Summary ({today}): {} completed, {} unfinished, {} focus time",
            completed.len(),
            unfinished.len(),
            format_ms(focus_ms)
        );
    }
    Ok(())
}

async fn cmd_worklog(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let by = flag_value(args, "--by").unwrap_or_else(|| "project".to_string());
    let days = if has_flag(args, "--month") {
        30
    } else if has_flag(args, "--week") {
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
        println!("No time logged in this period.");
    } else {
        for (name, ms) in rows {
            println!("{name:<24} {}", format_ms(ms));
        }
    }
    Ok(())
}

async fn cmd_sync(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
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
            "Synced: {} tasks ({replayed} replayed, {} pending)",
            cache.tasks.len(),
            cache.pending_ops.len()
        );
    }
    Ok(())
}

async fn cmd_doctor(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
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
            "API ({}): {}",
            config.api_url,
            if api_ok { "OK" } else { "OFFLINE" }
        );
        println!(
            "Bridge (127.0.0.1:{}): {}",
            config.bridge_port,
            if bridge_ok {
                "LISTENING"
            } else {
                "NOT RUNNING"
            }
        );
        println!("Config: {}", AppConfig::config_path().display());
        println!(
            "Cache:  {} ({} tasks, {} pending ops)",
            OfflineCache::cache_path().display(),
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
    fn resolves_task_by_id_prefix_or_title() {
        let tasks = vec![
            sample_task("abc123", "Write docs"),
            sample_task("def456", "Fix login bug"),
        ];
        assert_eq!(resolve_task(&tasks, "abc123").unwrap().id, "abc123");
        assert_eq!(resolve_task(&tasks, "def").unwrap().id, "def456");
        assert_eq!(resolve_task(&tasks, "login").unwrap().id, "def456");
        assert!(resolve_task(&tasks, "missing").is_err());
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
    fn positional_args_skips_value_flags() {
        let args: Vec<String> = vec!["add", "--top", "--note", "hello", "Buy", "milk", "--json"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            positional_args(&args[1..], &["--note", "--parent"]),
            vec!["Buy", "milk"]
        );
    }
}
