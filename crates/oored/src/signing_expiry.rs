//! Certificates and provisioning profiles close to expiring, from both the
//! pipeline signing settings and the app's signing library, and the
//! reminders they are owed.

use serde::Deserialize;
use sqlx::{Row, SqlitePool};

const DAY: i64 = 86_400;
/// Reminders start this far ahead.
pub(crate) const REMINDER_WINDOW: i64 = 30 * DAY;
/// An expiry older than this sends no reminder: it was dealt with or is
/// unused, and a first scan after an upgrade should not report old files.
const EXPIRED_GRACE: i64 = 7 * DAY;

/// How close a file is to expiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Band {
    Month,
    Week,
    Expired,
}

impl Band {
    pub(crate) fn of(expires_at: i64, now: i64) -> Option<Self> {
        let left = expires_at - now;
        if left <= 0 {
            Some(Self::Expired)
        } else if left <= 7 * DAY {
            Some(Self::Week)
        } else if left <= REMINDER_WINDOW {
            Some(Self::Month)
        } else {
            None
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Week => "week",
            Self::Expired => "expired",
        }
    }
}

/// One certificate or profile with an expiry date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Expiry {
    /// Stable across scans, e.g. `pipeline-profile:<id>`.
    pub key: String,
    pub project_id: String,
    pub project_name: String,
    /// "Distribution certificate" or "Profile for com.example.app".
    pub what: String,
    /// The pipeline or signing identity that holds it.
    pub holder: String,
    pub expires_at: i64,
    /// Where to replace it in the web app.
    pub path: String,
}

#[derive(Deserialize, Default)]
struct LibrarySummary {
    expires_at: Option<i64>,
    #[serde(default)]
    profiles: Vec<LibraryProfile>,
}

#[derive(Deserialize)]
struct LibraryProfile {
    bundle_id: String,
    expires_at: Option<i64>,
}

/// Every certificate and profile that expires between `from` and `until`,
/// soonest first.
pub(crate) async fn expiring(
    pool: &SqlitePool,
    from: i64,
    until: i64,
) -> sqlx::Result<Vec<Expiry>> {
    let mut found = Vec::new();
    let rows = sqlx::query(
        "SELECT 'pipeline-certificate:' || pl.id AS key, pl.id AS pipeline_id, pl.name AS holder, \
                p.id AS project_id, p.name AS project_name, \
                'Distribution certificate' AS what, s.p12_expires_at AS expires_at \
         FROM pipeline_ios_signing_settings s \
         JOIN pipelines pl ON pl.id = s.pipeline_id JOIN projects p ON p.id = pl.project_id \
         WHERE s.p12_expires_at BETWEEN ?1 AND ?2 \
         UNION ALL \
         SELECT 'pipeline-profile:' || pp.id, pl.id, pl.name, p.id, p.name, \
                'Profile for ' || pp.bundle_id, pp.expires_at \
         FROM pipeline_ios_provisioning_profiles pp \
         JOIN pipelines pl ON pl.id = pp.pipeline_id JOIN projects p ON p.id = pl.project_id \
         WHERE pp.expires_at BETWEEN ?1 AND ?2",
    )
    .bind(from)
    .bind(until)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let project_id: String = row.get("project_id");
        let pipeline_id: String = row.get("pipeline_id");
        found.push(Expiry {
            key: row.get("key"),
            path: format!("/projects/{project_id}/pipelines/{pipeline_id}/edit?signing=ios"),
            project_id,
            project_name: row.get("project_name"),
            what: row.get("what"),
            holder: row.get("holder"),
            expires_at: row.get("expires_at"),
        });
    }

    let identities = sqlx::query(
        "SELECT i.id, i.name, i.summary, p.id AS project_id, p.name AS project_name \
         FROM project_signing_identities i JOIN projects p ON p.id = i.project_id \
         WHERE i.kind = 'ios_distribution'",
    )
    .fetch_all(pool)
    .await?;
    for row in identities {
        let id: String = row.get("id");
        let project_id: String = row.get("project_id");
        let project_name: String = row.get("project_name");
        let holder: String = row.get("name");
        let summary: LibrarySummary =
            serde_json::from_str(&row.get::<String, _>("summary")).unwrap_or_default();
        let dates = summary
            .expires_at
            .map(|at| {
                (
                    format!("library-certificate:{id}"),
                    "Distribution certificate".to_string(),
                    at,
                )
            })
            .into_iter()
            .chain(summary.profiles.into_iter().filter_map(|profile| {
                profile.expires_at.map(|at| {
                    (
                        format!("library-profile:{id}:{}", profile.bundle_id),
                        format!("Profile for {}", profile.bundle_id),
                        at,
                    )
                })
            }));
        for (key, what, expires_at) in dates {
            if (from..=until).contains(&expires_at) {
                found.push(Expiry {
                    key,
                    project_id: project_id.clone(),
                    project_name: project_name.clone(),
                    what,
                    holder: holder.clone(),
                    expires_at,
                    path: format!("/projects/{project_id}?tab=settings"),
                });
            }
        }
    }
    found.sort_by(|a, b| {
        a.expires_at
            .cmp(&b.expires_at)
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(found)
}

/// Reminders due now: each file whose band has no reminder yet. Records
/// them as sent, so a reminder is never repeated. A file already in the 7 day
/// band gets that reminder only, not the 30 day one too.
pub(crate) async fn due_reminders(
    pool: &SqlitePool,
    now: i64,
) -> sqlx::Result<Vec<(Expiry, Band)>> {
    let mut due = Vec::new();
    for expiry in expiring(pool, now - EXPIRED_GRACE, now + REMINDER_WINDOW).await? {
        let Some(band) = Band::of(expiry.expires_at, now) else {
            continue;
        };
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO signing_expiry_notices \
             (credential_key, expires_at, band, project_id, notified_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&expiry.key)
        .bind(expiry.expires_at)
        .bind(band.as_str())
        .bind(&expiry.project_id)
        .bind(now)
        .execute(pool)
        .await?
        .rows_affected();
        if inserted == 1 {
            due.push((expiry, band));
        }
    }
    Ok(due)
}

/// "expires in 6 days", "expires tomorrow", "expired".
pub(crate) fn when(expires_at: i64, now: i64) -> String {
    let left = expires_at - now;
    if left <= 0 {
        return "has expired".to_string();
    }
    let days = ((left + DAY / 2) / DAY).max(1);
    if days == 1 {
        "expires tomorrow".to_string()
    } else {
        format!("expires in {days} days")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 0, 0)",
            "INSERT INTO projects (id, name, default_branch, created_by, created_at, updated_at) \
             VALUES ('p1', 'Shop', 'main', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Release', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn set_certificate(pool: &SqlitePool, expires_at: i64) {
        sqlx::query(
            "INSERT INTO pipeline_ios_signing_settings (id, pipeline_id, p12_expires_at, created_at, updated_at) \
             VALUES ('s1', 'pl1', ?1, 0, 0) \
             ON CONFLICT(pipeline_id) DO UPDATE SET p12_expires_at = excluded.p12_expires_at",
        )
        .bind(expires_at)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn bands_and_wording() {
        assert_eq!(Band::of(NOW + 31 * DAY, NOW), None);
        assert_eq!(Band::of(NOW + 30 * DAY, NOW), Some(Band::Month));
        assert_eq!(Band::of(NOW + 7 * DAY, NOW), Some(Band::Week));
        assert_eq!(Band::of(NOW, NOW), Some(Band::Expired));
        assert_eq!(when(NOW + 6 * DAY, NOW), "expires in 6 days");
        assert_eq!(when(NOW + DAY, NOW), "expires tomorrow");
        assert_eq!(when(NOW + 60, NOW), "expires tomorrow");
        assert_eq!(when(NOW - 1, NOW), "has expired");
    }

    #[tokio::test]
    async fn each_band_reminds_once_and_a_replaced_file_starts_again() {
        let pool = pool().await;
        let expires = NOW + 20 * DAY;
        set_certificate(&pool, expires).await;
        sqlx::query(
            "INSERT INTO project_signing_identities (id, project_id, name, kind, summary, material_encrypted, created_at, updated_at) \
             VALUES ('i1', 'p1', 'App Store', 'ios_distribution', ?1, 'x', 0, 0)",
        )
        .bind(
            serde_json::json!({
                "expires_at": NOW + 400 * DAY,
                "profiles": [{ "bundle_id": "com.shop", "expires_at": NOW + 3 * DAY }],
            })
            .to_string(),
        )
        .execute(&pool)
        .await
        .unwrap();

        let due = due_reminders(&pool, NOW).await.unwrap();
        let found: Vec<(&str, Band)> = due.iter().map(|(e, b)| (e.key.as_str(), *b)).collect();
        assert_eq!(
            found,
            [
                ("library-profile:i1:com.shop", Band::Week),
                ("pipeline-certificate:pl1", Band::Month),
            ]
        );
        assert_eq!(due[1].0.path, "/projects/p1/pipelines/pl1/edit?signing=ios");
        assert_eq!(due[0].0.what, "Profile for com.shop");

        // Nothing new an hour later.
        assert!(due_reminders(&pool, NOW + 3600).await.unwrap().is_empty());

        // The certificate reaches 7 days, then expires.
        let at_week = expires - 6 * DAY;
        let due = due_reminders(&pool, at_week).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].1, Band::Week);
        let due = due_reminders(&pool, expires + 60).await.unwrap();
        assert!(
            due.iter()
                .any(|(e, b)| e.key == "pipeline-certificate:pl1" && *b == Band::Expired)
        );

        // A new certificate is a new expiry date: reminders start again.
        set_certificate(&pool, NOW + 25 * DAY).await;
        let due = due_reminders(&pool, NOW).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].1, Band::Month);
    }

    #[tokio::test]
    async fn long_expired_files_send_nothing() {
        let pool = pool().await;
        set_certificate(&pool, NOW - 30 * DAY).await;
        assert!(due_reminders(&pool, NOW).await.unwrap().is_empty());
    }
}
