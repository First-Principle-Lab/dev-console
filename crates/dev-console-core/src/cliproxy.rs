//! Codex usage for the CLI Proxy tab (read-only).
//!
//! Two data sources, both local-first:
//!
//!   1. the ChatGPT/Codex backend (`https://chatgpt.com/backend-api/wham/usage`), called with the
//!      OAuth access token CLIProxyAPI already stores under `~/.cli-proxy-api/codex-*.json` - the
//!      same upstream T3 Code reads rate limits from, and the same credentials the user's proxy
//!      routes through;
//!   2. the Codex CLI's own session transcripts (`~/.codex/sessions/**/*.jsonl`), whose
//!      `token_usage_record` events carry per-response token counts. These feed the cost and token
//!      graphs; prices are applied on the frontend so the estimate stays inspectable. Only
//!      sessions whose `session_meta` names the proxy as `model_provider` count: a session on
//!      Codex's own login is not proxy usage. Transcripts do not say which proxy account served
//!      them, so this is the proxy's usage on this machine, not one account's.
//!
//! Nothing here logs, prints or stores the access token: it is sent to curl over stdin, never in process arguments.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const FIVE_HOURS_MINS: i64 = 5 * 60;
const WEEK_MINS: i64 = 7 * 24 * 60;
const MONTH_MINS: i64 = 30 * 24 * 60;
/// The `model_provider` Codex records for a session routed through CLIProxyAPI.
const PROXY_PROVIDER: &str = "cliproxy";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliproxyUsageResponse {
    pub account: Account,
    pub limits: Option<LimitLimits>,
    pub limits_error: Option<String>,
    pub records: Vec<UsageRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub email: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitLimits {
    pub checked_at: String,
    pub plan: Option<String>,
    pub windows: Vec<LimitWindow>,
    pub reset_credits: Option<ResetCredits>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub used_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_duration_mins: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredits {
    pub available_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub ts: String,
    pub session: String,
    pub model: String,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
}

fn home() -> AppResult<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| AppError::Other("HOME is not set".to_string()))
}

fn iso_from_epoch_seconds(value: f64) -> Option<String> {
    let secs = value as i64;
    if !value.is_finite() || secs <= 0 {
        return None;
    }
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

fn iso_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

/// `kind`/`label` follow T3's codex window mapping: 5-hour and shorter windows
/// are a session, anything past a week is weekly or monthly.
fn kind_for_duration(mins: i64) -> (&'static str, &'static str) {
    if mins >= MONTH_MINS {
        ("monthly", "Monthly")
    } else if mins >= WEEK_MINS {
        ("weekly", "Weekly")
    } else {
        ("session", "Session")
    }
}

struct CodexAuth {
    access_token: String,
    account_id: Option<String>,
    email: Option<String>,
    plan: Option<String>,
}

/// The first non-disabled `codex-*.json` auth file under `~/.cli-proxy-api`.
fn read_codex_auth() -> AppResult<CodexAuth> {
    let dir = home()?.join(".cli-proxy-api");
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|error| AppError::Other(format!("cannot list {}: {error}", dir.display())))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            name.starts_with("codex-") && name.ends_with(".json")
        })
        .collect();
    candidates.sort();
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value.get("disabled").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(token) = value.get("access_token").and_then(Value::as_str) else {
            continue;
        };
        if token.is_empty() {
            continue;
        }
        return Ok(CodexAuth {
            access_token: token.to_string(),
            account_id: value
                .get("account_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            email: value
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_string),
            plan: value
                .get("plan_type")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    Err(AppError::Other(
        "no Codex auth file in ~/.cli-proxy-api; sign in through the proxy first".to_string(),
    ))
}

/// GET through curl, returning (status, body). curl is a hard dependency of the
/// Linux desktop already; adding an HTTP client crate for one call is not.
fn https_get(url: &str, auth: &CodexAuth) -> AppResult<(u16, String)> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    // curl's config is private stdin; never expose bearer credentials through /proc.
    let quote = |value: &str| value.replace('\\', "\\\\").replace('"', "\\\"");
    if auth.access_token.contains(['\r', '\n'])
        || auth
            .account_id
            .as_ref()
            .is_some_and(|v| v.contains(['\r', '\n']))
    {
        return Err(AppError::Other("invalid credential header".into()));
    }
    let mut config = format!(
        "header = \"Authorization: Bearer {}\"\n",
        quote(&auth.access_token)
    );
    if let Some(id) = &auth.account_id {
        config.push_str(&format!("header = \"Chatgpt-Account-Id: {}\"\n", quote(id)));
    }
    let mut child = Command::new("curl")
        .args([
            "-q",
            "-sS",
            "--max-time",
            "25",
            "--config",
            "-",
            "-w",
            "\n%{http_code}",
            "-H",
            "Accept: application/json",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| AppError::Other("could not start usage request".into()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| AppError::Other("usage request input unavailable".into()))?
        .write_all(config.as_bytes())
        .map_err(|_| AppError::Other("could not send usage request".into()))?;
    let output = child
        .wait_with_output()
        .map_err(|_| AppError::Other("usage request failed".into()))?;
    if !output.status.success() {
        return Err(AppError::Other(
            "usage request timed out or could not connect".into(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, code) = match text.rsplit_once('\n') {
        Some((body, code)) => (body.to_string(), code.trim().parse::<u16>().unwrap_or(0)),
        None => (text.to_string(), 0),
    };
    Ok((code, body))
}

/// T3's codex window mapping: primary is the 5-hour session window, secondary
/// the weekly one; free/go plans report monthly windows instead.
fn codex_windows(rate_limit: &Value, plan: &str) -> Vec<LimitWindow> {
    let is_monthly_plan = plan == "free" || plan == "go";
    let fallback_mins = if is_monthly_plan {
        MONTH_MINS
    } else {
        FIVE_HOURS_MINS
    };
    let mut windows = Vec::new();
    for (id, key, fallback) in [
        ("primary", "primary_window", fallback_mins),
        ("secondary", "secondary_window", WEEK_MINS),
    ] {
        let Some(window) = rate_limit.get(key) else {
            continue;
        };
        if window.is_null() {
            continue;
        }
        let Some(used_percent) = window.get("used_percent").and_then(Value::as_f64) else {
            continue;
        };
        let duration_secs = window
            .get("limit_window_seconds")
            .and_then(Value::as_f64)
            .filter(|value| *value > 0.0);
        let window_duration_mins = duration_secs
            .map(|secs| (secs / 60.0) as i64)
            .filter(|mins| *mins > 0)
            .unwrap_or(fallback);
        let (kind, label) = kind_for_duration(window_duration_mins);
        windows.push(LimitWindow {
            id: id.to_string(),
            kind: kind.to_string(),
            label: label.to_string(),
            used_percent: used_percent.clamp(0.0, 100.0),
            window_duration_mins: Some(window_duration_mins),
            resets_at: window
                .get("reset_at")
                .and_then(Value::as_f64)
                .and_then(iso_from_epoch_seconds),
        });
    }
    windows
}

fn fetch_limits(auth: &CodexAuth) -> Result<LimitLimits, String> {
    let (status, body) = https_get(USAGE_URL, auth).map_err(|error| error.to_string())?;
    if status != 200 {
        return Err(if status == 401 {
            "the Codex token was rejected (401); refresh the sign-in through the proxy".to_string()
        } else {
            format!("the Codex usage endpoint answered {status}")
        });
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("unreadable usage response: {error}"))?;
    let plan = value
        .get("plan_type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let windows = value
        .get("rate_limit")
        .filter(|limit| !limit.is_null())
        .map(|limit| codex_windows(limit, plan.as_deref().unwrap_or_default()))
        .unwrap_or_default();
    Ok(LimitLimits {
        checked_at: iso_now(),
        plan: plan.or_else(|| auth.plan.clone()),
        windows,
        reset_credits: None,
    })
}

struct WindowRange {
    since: Option<i64>,
    until: i64,
}

fn parse_iso(value: Option<&str>) -> Option<i64> {
    let text = value?;
    OffsetDateTime::parse(text, &Rfc3339)
        .map(|dt| dt.unix_timestamp())
        .ok()
}

/// Walks `~/.codex/sessions` and collects the `token_usage_record` events whose
/// timestamp falls inside the range. Files older than the window are skipped by
/// modified time, which is safe because a file only ever grows forward.
fn scan_sessions(root: &Path, range: &WindowRange) -> Vec<UsageRecord> {
    let mut records = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(since) = range.since {
                let modified = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|elapsed| elapsed.as_secs() as i64);
                if modified.is_some_and(|modified| modified < since) {
                    continue;
                }
            }
            collect_records(&path, range, &mut records);
        }
    }
    records.sort_by(|left, right| left.ts.cmp(&right.ts));
    records
}

fn collect_records(path: &Path, range: &WindowRange, records: &mut Vec<UsageRecord>) {
    let Ok(file) = File::open(path) else {
        return;
    };
    let mut model = String::new();
    let mut session = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut seen = std::collections::HashSet::new();
    let mut provider = None;
    let mut own = Vec::new();
    let mut legacy = Vec::new();
    let mut last_total: Option<Value> = None;
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { break };
        // Cheap byte filter first: the full JSON parse only runs on the two
        // record types this scan needs.
        if line.contains("\"token_usage_record\"") {
            if let Some(mut record) = parse_usage_record(&line, &model, range) {
                let identity = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|v| v["payload"]["response_id"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| line.clone());
                if !seen.insert(identity) {
                    continue;
                }
                record.session = session.clone();
                own.push(record);
            }
        } else if line.contains("\"token_count\"") {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                let info = &value["payload"]["info"];
                let total = &info["total_token_usage"];
                if total.is_object() && last_total.as_ref() == Some(total) {
                    continue;
                }
                let usage = if total.is_object() {
                    let mut delta = serde_json::Map::new();
                    for key in [
                        "input_tokens",
                        "cached_input_tokens",
                        "cache_write_input_tokens",
                        "output_tokens",
                        "reasoning_output_tokens",
                    ] {
                        let current = total[key].as_u64().unwrap_or(0);
                        let prior = last_total
                            .as_ref()
                            .and_then(|v| v[key].as_u64())
                            .unwrap_or(0);
                        delta.insert(key.into(), Value::from(current.saturating_sub(prior)));
                    }
                    last_total = Some(total.clone());
                    Value::Object(delta)
                } else {
                    info["last_token_usage"].clone()
                };
                if usage.is_object() {
                    let normalized = serde_json::json!({"timestamp":value["timestamp"],"payload":{"usage":usage}}).to_string();
                    if let Some(mut record) = parse_usage_record(&normalized, &model, range) {
                        record.session = session.clone();
                        legacy.push(record);
                    }
                }
            }
        } else if line.contains("\"turn_context\"") || line.contains("\"session_meta\"") {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if value["type"] == "session_meta" {
                    if let Some(id) = value["payload"]["id"].as_str() {
                        session = id.to_owned();
                    }
                    if let Some(name) = value["payload"]["model_provider"].as_str() {
                        provider = Some(name.to_owned());
                    }
                }
                if let Some(next) = value
                    .get("payload")
                    .and_then(|payload| payload.get("model"))
                    .and_then(Value::as_str)
                {
                    model = next.to_string();
                }
            }
        }
    }
    // A session on Codex's own login, or one that never says, is not proxy usage.
    if provider.as_deref() != Some(PROXY_PROVIDER) {
        return;
    }
    // Current Codex emits both representations. Prefer per-response records when present,
    // never add cumulative snapshots a second time. Older files use snapshots only.
    records.extend(if own.is_empty() { legacy } else { own });
}

fn parse_usage_record(line: &str, model: &str, range: &WindowRange) -> Option<UsageRecord> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    let ts = value.get("timestamp")?.as_str()?.to_string();
    let at = parse_iso(Some(&ts))?;
    if let Some(since) = range.since {
        if at < since {
            return None;
        }
    }
    if at > range.until {
        return None;
    }
    let payload = value.get("payload")?;
    let usage = payload.get("usage")?;
    let session = payload
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let number = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    Some(UsageRecord {
        ts,
        session,
        model: model.to_string(),
        input_tokens: number("input_tokens")
            .saturating_sub(number("cached_input_tokens"))
            .saturating_sub(number("cache_write_input_tokens")),
        cached_input_tokens: number("cached_input_tokens"),
        cache_creation_tokens: number("cache_write_input_tokens"),
        output_tokens: number("output_tokens"),
        reasoning_tokens: number("reasoning_output_tokens"),
    })
}

/// The `cliproxy_usage` command body. Runs the network call first (fast), then
/// the transcript scan (potentially slow), all off the UI thread.
pub fn cliproxy_usage(
    since: Option<String>,
    until: Option<String>,
) -> AppResult<CliproxyUsageResponse> {
    let auth = read_codex_auth();
    let limits = match &auth {
        Ok(auth) => fetch_limits(auth),
        Err(error) => Err(error.to_string()),
    };
    let limits_error = limits
        .as_ref()
        .err()
        .map(|message| format!("Codex limits: {message}"));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default();
    let range = WindowRange {
        since: parse_iso(since.as_deref()).or(Some(0)),
        until: parse_iso(until.as_deref()).unwrap_or(now),
    };
    let home = home()?;
    let mut roots = vec![
        home.join(".codex/sessions"),
        home.join(".codex-cliproxy/sessions"),
    ];
    if let Some(root) = std::env::var_os("CODEX_HOME") {
        roots.push(PathBuf::from(root).join("sessions"));
    }
    let mut visited = std::collections::HashSet::new();
    let mut records = Vec::new();
    for root in roots {
        if let Ok(root) = root.canonicalize() {
            if visited.insert(root.clone()) {
                records.extend(scan_sessions(&root, &range));
            }
        }
    }
    records.sort_by(|a, b| a.ts.cmp(&b.ts));
    Ok(CliproxyUsageResponse {
        account: Account {
            email: auth.as_ref().ok().and_then(|a| a.email.clone()),
            plan: auth.as_ref().ok().and_then(|a| a.plan.clone()),
        },
        limits: limits.ok(),
        limits_error,
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn maps_windows_without_inventing_reset_times() {
        let windows = codex_windows(
            &json!({"primary_window":{"used_percent":130},"secondary_window":{"used_percent":12,"limit_window_seconds":604800,"reset_at":1794500000}}),
            "plus",
        );
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].used_percent, 100.0);
        assert_eq!(windows[0].resets_at, None);
        assert_eq!(windows[1].kind, "weekly");
        let free = codex_windows(&json!({"primary_window":{"used_percent":0}}), "free");
        assert_eq!(free[0].kind, "monthly");
    }
    #[test]
    fn cached_input_is_not_counted_twice_and_reasoning_is_not_added_to_output() {
        let line = json!({"type":"token_usage_record","timestamp":"2026-11-12T10:00:00Z","payload":{"usage":{"input_tokens":1000,"cached_input_tokens":800,"output_tokens":100,"reasoning_output_tokens":60}}}).to_string();
        let r = parse_usage_record(
            &line,
            "model",
            &WindowRange {
                since: None,
                until: i64::MAX,
            },
        )
        .unwrap();
        assert_eq!(r.input_tokens, 200);
        assert_eq!(r.cached_input_tokens, 800);
        assert_eq!(r.output_tokens, 100);
        assert!(parse_usage_record(
            &line,
            "model",
            &WindowRange {
                since: None,
                until: 0
            }
        )
        .is_none());
    }
    #[test]
    fn scan_keeps_session_identity_and_deduplicates_response_ids() {
        let dir = tempfile::tempdir().unwrap();
        let header = json!({"type":"session_meta","payload":{"id":"synthetic-session","model_provider":"cliproxy"}});
        let context = json!({"type":"turn_context","payload":{"model":"gpt-6-sol"}});
        let record = json!({"type":"token_usage_record","timestamp":"2026-11-12T10:00:00Z","payload":{"response_id":"synthetic-response","usage":{"input_tokens":100}}});
        std::fs::write(
            dir.path().join("session.jsonl"),
            format!("{header}\n{context}\n{record}\n{record}\nmalformed\n"),
        )
        .unwrap();
        let r = scan_sessions(
            dir.path(),
            &WindowRange {
                since: None,
                until: i64::MAX,
            },
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].session, "synthetic-session");
        assert_eq!(r[0].model, "gpt-6-sol");
    }
    #[test]
    fn only_sessions_routed_through_the_proxy_count() {
        let dir = tempfile::tempdir().unwrap();
        let record = json!({"type":"token_usage_record","timestamp":"2026-11-12T10:00:00Z","payload":{"usage":{"input_tokens":100}}});
        for (name, provider) in [
            ("proxy", Some("cliproxy")),
            ("own-login", Some("openai")),
            ("unknown", None),
        ] {
            let mut payload = json!({"id": name});
            if let Some(provider) = provider {
                payload["model_provider"] = json!(provider);
            }
            let header = json!({"type":"session_meta","payload":payload});
            std::fs::write(
                dir.path().join(format!("{name}.jsonl")),
                format!("{header}\n{record}\n"),
            )
            .unwrap();
        }
        let r = scan_sessions(
            dir.path(),
            &WindowRange {
                since: None,
                until: i64::MAX,
            },
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].session, "proxy");
    }
}
