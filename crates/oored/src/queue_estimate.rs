//! A queued build's place in line, in words people can use: how many builds
//! a runner takes first and about how long that takes.

use std::collections::HashMap;

use oore_contract::BuildQueueEstimate;
use sqlx::{Row, SqlitePool};

/// Recent successful builds per pipeline that the median uses.
const HISTORY: i64 = 20;

fn median(mut values: Vec<i64>) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

async fn pipeline_median(
    pool: &SqlitePool,
    cache: &mut HashMap<String, Option<i64>>,
    pipeline_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    if let Some(known) = cache.get(pipeline_id) {
        return Ok(*known);
    }
    let durations: Vec<i64> = sqlx::query_scalar(
        "SELECT finished_at - started_at FROM builds \
         WHERE pipeline_id = ?1 AND status = 'succeeded' \
           AND started_at IS NOT NULL AND finished_at IS NOT NULL \
         ORDER BY finished_at DESC LIMIT ?2",
    )
    .bind(pipeline_id)
    .bind(HISTORY)
    .fetch_all(pool)
    .await?;
    let value = median(durations);
    cache.insert(pipeline_id.to_string(), value);
    Ok(value)
}

/// Remaining work, in seconds, of builds ahead and running, spread over the
/// online runners' build slots. `None` while no pipeline involved has a
/// finished build to learn from.
pub(crate) fn wait_seconds(
    queued_ahead: &[Option<i64>],
    running_remaining: &[Option<i64>],
    slots: i64,
) -> Option<i64> {
    let known: Vec<i64> = queued_ahead
        .iter()
        .chain(running_remaining)
        .flatten()
        .copied()
        .collect();
    if known.is_empty() && !(queued_ahead.is_empty() && running_remaining.is_empty()) {
        return None;
    }
    let total: i64 = known.iter().sum();
    Some(total / slots.max(1))
}

/// The estimate for a build that waits in the queue, or `None` for any
/// other build, including one held for approval.
pub async fn estimate(
    pool: &SqlitePool,
    build_id: &str,
    now: i64,
) -> Result<Option<BuildQueueEstimate>, sqlx::Error> {
    let Some(row) =
        sqlx::query("SELECT status, approval_state, queued_at FROM builds WHERE id = ?1")
            .bind(build_id)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let status: String = row.get("status");
    let approval: Option<String> = row.get("approval_state");
    if status != "queued" || approval.as_deref() == Some("pending") {
        return Ok(None);
    }
    let queued_at: i64 = row.get("queued_at");

    let ahead = sqlx::query(
        "SELECT pipeline_id FROM builds \
         WHERE status = 'queued' AND (approval_state IS NULL OR approval_state != 'pending') \
           AND (queued_at < ?1 OR (queued_at = ?1 AND id < ?2))",
    )
    .bind(queued_at)
    .bind(build_id)
    .fetch_all(pool)
    .await?;
    let running = sqlx::query(
        "SELECT pipeline_id, started_at FROM builds WHERE status IN ('assigned', 'running')",
    )
    .fetch_all(pool)
    .await?;
    let slots: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(max_concurrent_builds), 0) FROM runners WHERE status IN ('online', 'busy')",
    )
    .fetch_one(pool)
    .await?;

    let mut cache = HashMap::new();
    let mut queued_work = Vec::with_capacity(ahead.len());
    for row in &ahead {
        let pipeline_id: Option<String> = row.get("pipeline_id");
        queued_work.push(match pipeline_id {
            Some(id) => pipeline_median(pool, &mut cache, &id).await?,
            None => None,
        });
    }
    let mut running_work = Vec::with_capacity(running.len());
    for row in &running {
        let pipeline_id: Option<String> = row.get("pipeline_id");
        let started: Option<i64> = row.get("started_at");
        let typical = match pipeline_id {
            Some(id) => pipeline_median(pool, &mut cache, &id).await?,
            None => None,
        };
        running_work.push(typical.map(|typical| {
            let elapsed = started.map_or(0, |started| now - started);
            (typical - elapsed).max(30)
        }));
    }
    // With a free slot the build is next, whatever runs elsewhere.
    let busy = ahead.len() as i64 + running.len() as i64;
    let estimated_wait_seconds = if slots > busy {
        Some(0)
    } else {
        wait_seconds(&queued_work, &running_work, slots)
    };
    Ok(Some(BuildQueueEstimate {
        ahead: ahead.len() as i64,
        running: running.len() as i64,
        estimated_wait_seconds,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_is_shared_by_the_slots() {
        assert_eq!(
            wait_seconds(&[Some(600), Some(300)], &[Some(120)], 2),
            Some(510)
        );
        assert_eq!(wait_seconds(&[None], &[None], 1), None);
        assert_eq!(wait_seconds(&[], &[], 1), Some(0));
        assert_eq!(wait_seconds(&[Some(60)], &[None], 0), Some(60));
    }
}
