use crate::models::{Project, Tag};
use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, Weekday};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedShortSyntax {
    pub title: String,
    pub project_id: Option<String>,
    pub tag_ids: Vec<String>,
    pub due_day: Option<String>,
    pub deadline_day: Option<String>,
    pub time_spent: Option<u64>,
    pub time_estimate: Option<u64>,
}

/// Parses compound durations like `1h5m3s`, `2d`, `1.5h`, `30m`, `45s`.
pub fn parse_duration_ms(input: &str) -> Option<u64> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    let mut total_ms: f64 = 0.0;
    let mut num_buf = String::new();
    let mut matched_any = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            num_buf.push(ch);
        } else if matches!(ch, 'd' | 'h' | 'm' | 's') {
            if num_buf.is_empty() {
                return None;
            }
            let val: f64 = num_buf.parse().ok()?;
            num_buf.clear();
            let mult = match ch {
                'd' => 86_400_000.0,
                'h' => 3_600_000.0,
                'm' => 60_000.0,
                's' => 1_000.0,
                _ => return None,
            };
            total_ms += val * mult;
            matched_any = true;
        } else if ch.is_whitespace() {
            continue;
        } else {
            return None;
        }
    }

    if !num_buf.is_empty() || !matched_any {
        return None;
    }
    Some(total_ms.round() as u64)
}

/// Parses `[t]<spent>/<estimate>` or `[t]<estimate>` tokens.
pub fn parse_time_token(token: &str) -> Option<(Option<u64>, Option<u64>)> {
    let body = token.strip_prefix('t').unwrap_or(token);
    if body.is_empty() {
        return None;
    }
    if let Some((left, right)) = body.split_once('/') {
        let spent = if left.is_empty() {
            None
        } else {
            Some(parse_duration_ms(left)?)
        };
        let est = if right.is_empty() {
            None
        } else {
            Some(parse_duration_ms(right)?)
        };
        if spent.is_none() && est.is_none() {
            return None;
        }
        Some((spent, est))
    } else {
        let est = parse_duration_ms(body)?;
        Some((None, Some(est)))
    }
}

/// Resolves date expressions like `today`, `tomorrow`, `friday`, `next_week`, `2d`, `1h5m3s`, `YYYY-MM-DD`.
pub fn parse_date_expr(expr: &str, today: NaiveDate) -> Option<String> {
    let raw = expr.trim().to_lowercase();
    if raw.is_empty() {
        return None;
    }
    let normalized = raw.replace('_', " ");
    match normalized.as_str() {
        "today" | "tod" => return Some(today.format("%Y-%m-%d").to_string()),
        "tomorrow" | "tom" => {
            return Some(
                (today + ChronoDuration::days(1))
                    .format("%Y-%m-%d")
                    .to_string(),
            );
        }
        "next week" => {
            return Some(
                (today + ChronoDuration::days(7))
                    .format("%Y-%m-%d")
                    .to_string(),
            );
        }
        "next month" => {
            return Some(
                (today + ChronoDuration::days(30))
                    .format("%Y-%m-%d")
                    .to_string(),
            );
        }
        _ => {}
    }

    if let Ok(d) = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d") {
        return Some(d.format("%Y-%m-%d").to_string());
    }

    if let Some(wd) = parse_weekday(&normalized) {
        let cur = today.weekday().num_days_from_monday() as i64;
        let tgt = wd.num_days_from_monday() as i64;
        let mut diff = tgt - cur;
        if diff <= 0 {
            diff += 7;
        }
        return Some(
            (today + ChronoDuration::days(diff))
                .format("%Y-%m-%d")
                .to_string(),
        );
    }

    if let Some(ms) = parse_duration_ms(&normalized) {
        let days = (ms / 86_400_000) as i64;
        return Some(
            (today + ChronoDuration::days(days))
                .format("%Y-%m-%d")
                .to_string(),
        );
    }

    // Time of day like `4pm` or `14:30` maps to today
    if normalized.ends_with("am")
        || normalized.ends_with("pm")
        || (normalized.contains(':') && normalized.chars().all(|c| c.is_ascii_digit() || c == ':'))
    {
        return Some(today.format("%Y-%m-%d").to_string());
    }

    None
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    match s {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

pub fn parse_short_syntax(
    raw_title: &str,
    projects: &[Project],
    tags: &[Tag],
    initial_project_id: Option<String>,
    initial_tag_ids: Vec<String>,
) -> ParsedShortSyntax {
    let today = Local::now().date_naive();
    let mut clean_words = Vec::new();
    let mut project_id = initial_project_id;
    let mut tag_ids = initial_tag_ids;
    let mut due_day = None;
    let mut deadline_day = None;
    let mut time_spent = None;
    let mut time_estimate = None;

    for word in raw_title.split_whitespace() {
        if let Some(tag_name) = word.strip_prefix('#') {
            if let Some(tag) = tags.iter().find(|t| t.title.eq_ignore_ascii_case(tag_name)) {
                if !tag_ids.contains(&tag.id) {
                    tag_ids.push(tag.id.clone());
                }
            } else {
                clean_words.push(word);
            }
        } else if let Some(proj_name) = word.strip_prefix('+') {
            let needle = proj_name.replace(' ', "").to_lowercase();
            let matched = projects
                .iter()
                .filter(|p| {
                    let hay = p.title.replace(' ', "").to_lowercase();
                    hay == needle || (needle.len() >= 3 && hay.starts_with(&needle))
                })
                .min_by_key(|p| p.title.len());
            if let Some(p) = matched {
                project_id = Some(p.id.clone());
            } else {
                clean_words.push(word);
            }
        } else if let Some(due_expr) = word.strip_prefix('@') {
            if let Some(d) = parse_date_expr(due_expr, today) {
                due_day = Some(d);
            } else {
                clean_words.push(word);
            }
        } else if let Some(dl_expr) = word.strip_prefix('!') {
            if let Some(d) = parse_date_expr(dl_expr, today) {
                deadline_day = Some(d);
            } else {
                clean_words.push(word);
            }
        } else if let Some((spent, est)) = parse_time_token(word) {
            if spent.is_some() {
                time_spent = spent;
            }
            if est.is_some() {
                time_estimate = est;
            }
        } else {
            clean_words.push(word);
        }
    }

    ParsedShortSyntax {
        title: clean_words.join(" "),
        project_id,
        tag_ids,
        due_day,
        deadline_day,
        time_spent,
        time_estimate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compound_durations() {
        assert_eq!(parse_duration_ms("1h5m3s"), Some(3_903_000));
        assert_eq!(parse_duration_ms("2d"), Some(172_800_000));
        assert_eq!(parse_duration_ms("1.5h"), Some(5_400_000));
        assert_eq!(parse_time_token("t1h5m3s"), Some((None, Some(3_903_000))));
        assert_eq!(
            parse_time_token("30m/1h"),
            Some((Some(1_800_000), Some(3_600_000)))
        );
    }

    #[test]
    fn parses_full_short_syntax() {
        let projects = vec![Project {
            id: "p1".into(),
            title: "Work".into(),
            theme: None,
            color: None,
        }];
        let tags = vec![Tag {
            id: "t1".into(),
            title: "urgent".into(),
            color: None,
            theme: None,
        }];
        let res = parse_short_syntax(
            "Fix bug +work #urgent t1h5m3s @tomorrow !next_week",
            &projects,
            &tags,
            None,
            vec![],
        );
        assert_eq!(res.title, "Fix bug");
        assert_eq!(res.project_id.as_deref(), Some("p1"));
        assert_eq!(res.tag_ids, vec!["t1".to_string()]);
        assert_eq!(res.time_estimate, Some(3_903_000));
        assert!(res.due_day.is_some());
        assert!(res.deadline_day.is_some());
    }
}
