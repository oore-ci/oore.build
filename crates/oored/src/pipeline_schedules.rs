//! Cron schedules on pipelines.
//!
//! `trigger_config.schedules` holds five-field cron expressions evaluated in
//! UTC. A daemon task checks them every few seconds and starts one build per
//! due schedule. Each schedule's last checked minute lives in
//! `pipeline_schedule_runs`, so a restart neither replays old runs nor fires
//! a minute twice.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Datelike, Timelike, Utc};
use oore_contract::{ScheduleTrigger, TriggerConfig};
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};

use crate::util::now_unix;

/// Most schedules one pipeline may define.
pub const MAX_SCHEDULES: usize = 10;
/// A schedule missed while the daemon was down still fires if it was due
/// within this window. Older runs are skipped.
const CATCH_UP_SECS: i64 = 5 * 60;
const TICK: Duration = Duration::from_secs(20);

/// A parsed five-field cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: u64,
    hours: u64,
    days_of_month: u64,
    months: u64,
    days_of_week: u64,
    /// Standard cron: when both day fields are restricted, either may match.
    day_of_month_any: bool,
    day_of_week_any: bool,
}

impl Cron {
    pub fn parse(expression: &str) -> Result<Self, String> {
        let expression = expression.trim();
        let expanded = match expression {
            "@hourly" => "0 * * * *",
            "@daily" | "@midnight" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" | "@annually" => "0 0 1 1 *",
            other => other,
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        let [minute, hour, day_of_month, month, day_of_week] = fields[..] else {
            return Err(
                "must have five fields: minute hour day-of-month month day-of-week".to_string(),
            );
        };
        let mut days_of_week = parse_field(day_of_week, 0, 7, "day-of-week")?;
        // 7 is Sunday, like 0.
        if days_of_week & (1 << 7) != 0 {
            days_of_week = (days_of_week & !(1 << 7)) | 1;
        }
        Ok(Self {
            minutes: parse_field(minute, 0, 59, "minute")?,
            hours: parse_field(hour, 0, 23, "hour")?,
            days_of_month: parse_field(day_of_month, 1, 31, "day-of-month")?,
            months: parse_field(month, 1, 12, "month")?,
            days_of_week,
            day_of_month_any: day_of_month == "*",
            day_of_week_any: day_of_week == "*",
        })
    }

    /// Whether the schedule fires in the minute containing `at`.
    pub fn matches(&self, at: DateTime<Utc>) -> bool {
        let bit = |set: u64, value: u32| set & (1 << value) != 0;
        let day_of_month = bit(self.days_of_month, at.day());
        let day_of_week = bit(self.days_of_week, at.weekday().num_days_from_sunday());
        let day = match (self.day_of_month_any, self.day_of_week_any) {
            (true, true) => true,
            (true, false) => day_of_week,
            (false, true) => day_of_month,
            (false, false) => day_of_month || day_of_week,
        };
        day && bit(self.minutes, at.minute())
            && bit(self.hours, at.hour())
            && bit(self.months, at.month())
    }

    /// Whether the schedule fires in any minute of `(after, until]`.
    fn due_between(&self, after: i64, until: i64) -> bool {
        let mut minute = (after / 60 + 1) * 60;
        while minute <= until {
            if DateTime::from_timestamp(minute, 0).is_some_and(|at| self.matches(at)) {
                return true;
            }
            minute += 60;
        }
        false
    }
}

fn parse_field(field: &str, min: u32, max: u32, name: &str) -> Result<u64, String> {
    let mut set = 0u64;
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step: u32 = step
                    .parse()
                    .ok()
                    .filter(|step| *step > 0)
                    .ok_or_else(|| format!("{name} step '{step}' must be a positive number"))?;
                (range, step)
            }
            None => (part, 1),
        };
        let (start, end) = if range == "*" {
            (min, max)
        } else if let Some((start, end)) = range.split_once('-') {
            (
                parse_value(start, min, max, name)?,
                parse_value(end, min, max, name)?,
            )
        } else {
            let value = parse_value(range, min, max, name)?;
            // `5/15` means from 5 to the end in steps of 15.
            (value, if part.contains('/') { max } else { value })
        };
        if start > end {
            return Err(format!("{name} range '{range}' runs backwards"));
        }
        let mut value = start;
        while value <= end {
            set |= 1 << value;
            value += step;
        }
    }
    Ok(set)
}

fn parse_value(raw: &str, min: u32, max: u32, name: &str) -> Result<u32, String> {
    raw.parse::<u32>()
        .ok()
        .filter(|value| (min..=max).contains(value))
        .ok_or_else(|| format!("{name} value '{raw}' must be between {min} and {max}"))
}

/// Validation errors for a pipeline's schedules.
pub fn validate_schedules(schedules: &[ScheduleTrigger]) -> Vec<String> {
    let mut errors = Vec::new();
    if schedules.len() > MAX_SCHEDULES {
        errors.push(format!(
            "trigger_config.schedules has too many entries (max {MAX_SCHEDULES})"
        ));
    }
    for (index, schedule) in schedules.iter().enumerate() {
        if let Err(error) = Cron::parse(&schedule.cron) {
            errors.push(format!("trigger_config.schedules[{index}].cron {error}"));
        }
        if schedule
            .branch
            .as_deref()
            .is_some_and(|branch| branch.trim().is_empty())
        {
            errors.push(format!(
                "trigger_config.schedules[{index}].branch must not be empty when provided"
            ));
        }
    }
    errors
}

fn schedule_key(schedule: &ScheduleTrigger) -> String {
    format!(
        "{}|{}",
        schedule.cron.trim(),
        schedule.branch.as_deref().unwrap_or("").trim()
    )
}

/// Start the schedule monitor.
pub fn start_schedule_monitor(state: Arc<crate::AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            if let Err(error) =
                run_schedule_tick(&state.db, &state.encryption_key, now_unix()).await
            {
                error!(%error, "schedule monitor tick failed");
            }
        }
    });
}

/// Start a build for every schedule that came due since its last check.
/// Returns the IDs of the builds started.
pub async fn run_schedule_tick(
    pool: &SqlitePool,
    encryption_key: &[u8],
    now: i64,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT id, trigger_config FROM pipelines \
         WHERE enabled = 1 AND json_array_length(trigger_config, '$.schedules') > 0",
    )
    .fetch_all(pool)
    .await?;

    let mut started = Vec::new();
    for row in rows {
        let pipeline_id: String = row.get("id");
        let Ok(trigger_config) =
            serde_json::from_str::<TriggerConfig>(&row.get::<String, _>("trigger_config"))
        else {
            warn!(%pipeline_id, "skipping schedules: invalid trigger_config");
            continue;
        };
        for schedule in &trigger_config.schedules {
            let Ok(cron) = Cron::parse(&schedule.cron) else {
                warn!(%pipeline_id, cron = %schedule.cron, "skipping invalid schedule");
                continue;
            };
            let Some(previous) = claim_due_run(pool, &pipeline_id, schedule, &cron, now).await?
            else {
                continue;
            };
            match crate::builds::create_scheduled_build(
                pool,
                encryption_key,
                &pipeline_id,
                schedule,
            )
            .await
            {
                Ok(build_id) => {
                    info!(%pipeline_id, %build_id, cron = %schedule.cron, "scheduled build started");
                    started.push(build_id);
                }
                // A source outage or a busy database is usually brief. Hand
                // the run back so a later tick retries it within the
                // catch-up window.
                Err((status, body))
                    if status.is_server_error()
                        && release_claim(pool, &pipeline_id, schedule, previous, now).await? =>
                {
                    warn!(
                        %pipeline_id,
                        cron = %schedule.cron,
                        %status,
                        error = %body.0.error,
                        "scheduled build could not start; retrying"
                    );
                }
                Err((status, body)) => {
                    error!(
                        %pipeline_id,
                        cron = %schedule.cron,
                        %status,
                        error = %body.0.error,
                        "scheduled build could not start"
                    );
                }
            }
        }
    }
    Ok(started)
}

/// Advance the schedule's checkpoint to `now`. When a run came due in
/// between, returns the previous checkpoint. The conditional update makes a
/// run fire at most once.
async fn claim_due_run(
    pool: &SqlitePool,
    pipeline_id: &str,
    schedule: &ScheduleTrigger,
    cron: &Cron,
    now: i64,
) -> anyhow::Result<Option<i64>> {
    let key = schedule_key(schedule);
    let last_checked: Option<i64> = sqlx::query_scalar(
        "SELECT last_checked_at FROM pipeline_schedule_runs \
         WHERE pipeline_id = ?1 AND schedule_key = ?2",
    )
    .bind(pipeline_id)
    .bind(&key)
    .fetch_optional(pool)
    .await?;

    let Some(last_checked) = last_checked else {
        // A new schedule starts counting from now rather than replaying.
        sqlx::query(
            "INSERT INTO pipeline_schedule_runs (pipeline_id, schedule_key, last_checked_at) \
             VALUES (?1, ?2, ?3) ON CONFLICT(pipeline_id, schedule_key) DO NOTHING",
        )
        .bind(pipeline_id)
        .bind(&key)
        .bind(now)
        .execute(pool)
        .await?;
        return Ok(None);
    };
    if now / 60 <= last_checked / 60 {
        return Ok(None);
    }

    let claimed = sqlx::query(
        "UPDATE pipeline_schedule_runs SET last_checked_at = ?1 \
         WHERE pipeline_id = ?2 AND schedule_key = ?3 AND last_checked_at = ?4",
    )
    .bind(now)
    .bind(pipeline_id)
    .bind(&key)
    .bind(last_checked)
    .execute(pool)
    .await?
    .rows_affected()
        == 1;
    Ok(
        (claimed && cron.due_between(last_checked.max(now - CATCH_UP_SECS), now))
            .then_some(last_checked),
    )
}

/// Put the checkpoint back to `previous` after a run claimed at `now` failed
/// to start, unless another tick moved it or the run's build exists after all.
async fn release_claim(
    pool: &SqlitePool,
    pipeline_id: &str,
    schedule: &ScheduleTrigger,
    previous: i64,
    now: i64,
) -> anyhow::Result<bool> {
    let released = sqlx::query(
        "UPDATE pipeline_schedule_runs SET last_checked_at = ?1 \
         WHERE pipeline_id = ?2 AND schedule_key = ?3 AND last_checked_at = ?4 \
           AND NOT EXISTS ( \
               SELECT 1 FROM builds \
               WHERE pipeline_id = ?2 AND trigger_type = 'schedule' AND created_at >= ?4 \
           )",
    )
    .bind(previous)
    .bind(pipeline_id)
    .bind(schedule_key(schedule))
    .bind(now)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(released == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn parses_steps_ranges_lists_and_aliases() {
        let cron = Cron::parse("*/15 9-17 * * 1-5").unwrap();
        assert!(cron.matches(at("2026-10-21T09:45:00Z"))); // Wednesday
        assert!(!cron.matches(at("2026-10-21T09:50:00Z")));
        assert!(!cron.matches(at("2026-10-25T10:00:00Z"))); // Sunday

        let daily = Cron::parse("@daily").unwrap();
        assert!(daily.matches(at("2026-10-21T00:00:00Z")));
        assert!(!daily.matches(at("2026-10-21T00:01:00Z")));

        let sunday = Cron::parse("30 2 * * 7").unwrap();
        assert!(sunday.matches(at("2026-10-25T02:30:00Z")));

        let list = Cron::parse("0,30 12 1,15 * *").unwrap();
        assert!(list.matches(at("2026-10-15T12:30:00Z")));
        assert!(!list.matches(at("2026-10-16T12:30:00Z")));
    }

    #[test]
    fn restricted_day_fields_match_either() {
        // The 1st of the month or any Monday.
        let cron = Cron::parse("0 6 1 * 1").unwrap();
        assert!(cron.matches(at("2026-10-01T06:00:00Z"))); // Thursday the 1st
        assert!(cron.matches(at("2026-10-19T06:00:00Z"))); // Monday
        assert!(!cron.matches(at("2026-10-21T06:00:00Z")));
    }

    #[test]
    fn rejects_malformed_expressions() {
        for bad in [
            "* * * *",
            "60 * * * *",
            "* 24 * * *",
            "*/0 * * * *",
            "5-1 * * * *",
            "a * * * *",
        ] {
            assert!(Cron::parse(bad).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn due_between_covers_the_half_open_window() {
        let cron = Cron::parse("0 * * * *").unwrap();
        let hour = at("2026-10-21T10:00:00Z").timestamp();
        assert!(cron.due_between(hour - 60, hour));
        assert!(!cron.due_between(hour, hour + 59 * 60));
    }

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 'developer', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) VALUES ('p1', 'App', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) VALUES ('pl1', 'p1', 'main', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn a_due_minute_is_claimed_once_and_new_schedules_do_not_replay() {
        let pool = pool().await;
        let schedule = ScheduleTrigger {
            cron: "0 * * * *".into(),
            branch: None,
        };
        let cron = Cron::parse(&schedule.cron).unwrap();
        let hour = at("2026-10-21T10:00:00Z").timestamp();

        // First sight records a checkpoint even though the minute is due.
        assert_eq!(
            claim_due_run(&pool, "pl1", &schedule, &cron, hour + 5)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            claim_due_run(&pool, "pl1", &schedule, &cron, hour + 30)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            claim_due_run(&pool, "pl1", &schedule, &cron, hour + 3600)
                .await
                .unwrap(),
            Some(hour + 5)
        );
        assert_eq!(
            claim_due_run(&pool, "pl1", &schedule, &cron, hour + 3610)
                .await
                .unwrap(),
            None
        );

        // Runs missed for longer than the catch-up window are skipped.
        assert_eq!(
            claim_due_run(&pool, "pl1", &schedule, &cron, hour + 3 * 3600 + 900)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn a_run_that_fails_to_start_is_retried_on_the_next_tick() {
        let repo = tempfile::tempdir().unwrap();
        let path = repo.path().to_str().unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "init",
            ],
        ] {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(path)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let pool = pool().await;
        for statement in [
            "INSERT INTO integrations (id, provider, host_url, auth_mode, created_by, created_at, updated_at) \
             VALUES ('i1', 'local_git', 'local://filesystem', 'local_path', 'u1', 0, 0)"
                .to_string(),
            "INSERT INTO integration_installations (id, integration_id, external_id, account_name, created_at, updated_at) \
             VALUES ('inst1', 'i1', 'local', 'local', 0, 0)"
                .to_string(),
            format!(
                "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, html_url, created_at, updated_at) \
                 VALUES ('r1', 'inst1', 'repo', 'app', '{path}', 0, 0)"
            ),
            "UPDATE projects SET repository_id = 'r1', default_branch = 'main'".to_string(),
            r#"UPDATE pipelines SET trigger_config = '{"schedules":[{"cron":"0 * * * *"}]}'"#
                .to_string(),
            "CREATE TRIGGER fail_build BEFORE INSERT ON builds \
             BEGIN SELECT RAISE(ABORT, 'database is locked'); END"
                .to_string(),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&pool)
                .await
                .unwrap();
        }
        let key = [0u8; 32];
        let hour = at("2026-10-21T10:00:00Z").timestamp();

        assert!(
            run_schedule_tick(&pool, &key, hour - 30)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            run_schedule_tick(&pool, &key, hour + 10)
                .await
                .unwrap()
                .is_empty()
        );
        sqlx::query("DROP TRIGGER fail_build")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            run_schedule_tick(&pool, &key, hour + 30)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            run_schedule_tick(&pool, &key, hour + 50)
                .await
                .unwrap()
                .is_empty()
        );

        let builds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(builds, 1);
    }
}
