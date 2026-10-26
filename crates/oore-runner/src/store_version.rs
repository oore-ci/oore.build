//! Store version checks that run before an upload, so a build whose version
//! code the store would reject fails early with both numbers in the message.

use std::collections::BTreeSet;

use anyhow::Context;
use oore_google_play::play_json;
use serde::Deserialize;

const PLAY_API_BASE: &str = "https://androidpublisher.googleapis.com/androidpublisher/v3";

/// Version codes Google Play already holds for one app.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlayVersionCodes {
    /// Highest version code in any release on the target track.
    pub track_highest: Option<i64>,
    /// Every version code uploaded to the app, on any track or none.
    pub used: BTreeSet<i64>,
}

#[derive(Debug, Deserialize)]
struct PlayEdit {
    id: String,
}

#[derive(Debug, Default, Deserialize)]
struct PlayTracks {
    #[serde(default)]
    tracks: Vec<PlayTrack>,
}

#[derive(Debug, Deserialize)]
struct PlayTrack {
    track: String,
    #[serde(default)]
    releases: Vec<PlayRelease>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayRelease {
    #[serde(default)]
    version_codes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PlayBundles {
    #[serde(default)]
    bundles: Vec<PlayBundle>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayBundle {
    version_code: i64,
}

fn parse_play_version_codes(
    tracks: PlayTracks,
    bundles: PlayBundles,
    track: &str,
) -> anyhow::Result<PlayVersionCodes> {
    let mut codes = PlayVersionCodes::default();
    for entry in tracks.tracks {
        for release in entry.releases {
            for raw in release.version_codes {
                let code: i64 = raw
                    .parse()
                    .with_context(|| format!("Google Play returned version code {raw:?}"))?;
                codes.used.insert(code);
                if entry.track == track {
                    codes.track_highest = codes.track_highest.max(Some(code));
                }
            }
        }
    }
    codes
        .used
        .extend(bundles.bundles.iter().map(|bundle| bundle.version_code));
    Ok(codes)
}

/// Reads the version codes Google Play holds for `package_name` through a
/// throwaway edit, which is deleted afterwards and never committed.
pub async fn fetch_play_version_codes(
    client: &reqwest::Client,
    access_token: &str,
    package_name: &str,
    track: &str,
) -> anyhow::Result<PlayVersionCodes> {
    fetch_play_version_codes_from(PLAY_API_BASE, client, access_token, package_name, track).await
}

async fn fetch_play_version_codes_from(
    api_base: &str,
    client: &reqwest::Client,
    access_token: &str,
    package_name: &str,
    track: &str,
) -> anyhow::Result<PlayVersionCodes> {
    let app = format!("{api_base}/applications/{package_name}");
    let edit: PlayEdit = play_json(
        client
            .post(format!("{app}/edits"))
            .bearer_auth(access_token)
            .json(&serde_json::json!({})),
        "open an edit",
    )
    .await?;
    let edit_url = format!("{app}/edits/{}", edit.id);

    let read = async {
        let tracks: PlayTracks = play_json(
            client
                .get(format!("{edit_url}/tracks"))
                .bearer_auth(access_token),
            "list tracks",
        )
        .await?;
        let bundles: PlayBundles = play_json(
            client
                .get(format!("{edit_url}/bundles"))
                .bearer_auth(access_token),
            "list bundles",
        )
        .await?;
        parse_play_version_codes(tracks, bundles, track)
    }
    .await;

    // The edit only existed to read state; leaving it open would block the
    // next edit's commit, so delete it whether or not the reads succeeded.
    let _ = client
        .delete(&edit_url)
        .bearer_auth(access_token)
        .send()
        .await;
    read
}

/// Fails when Google Play would reject `version_code` on `track`: when the
/// code was already uploaded, or is not above the track's highest release.
pub fn ensure_play_version_code(
    version_code: i64,
    store: &PlayVersionCodes,
    track: &str,
) -> anyhow::Result<()> {
    if let Some(highest) = store.track_highest
        && version_code <= highest
    {
        anyhow::bail!(
            "Version code {version_code} is not higher than {highest}, the highest version code on the Google Play {track} track. Raise build_number_offset by at least {} so this build gets a higher OORE_VERSION_CODE.",
            highest - version_code + 1
        );
    }
    if store.used.contains(&version_code) {
        let highest_used = store.used.last().copied().unwrap_or(version_code);
        anyhow::bail!(
            "Version code {version_code} was already uploaded to Google Play (highest uploaded is {highest_used}). Raise build_number_offset by at least {} so this build gets an unused OORE_VERSION_CODE.",
            highest_used - version_code + 1
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(track: &str) -> PlayVersionCodes {
        let tracks: PlayTracks = serde_json::from_value(serde_json::json!({
            "tracks": [
                {"track": "production", "releases": [
                    {"status": "completed", "versionCodes": ["120"]},
                    {"status": "inProgress", "versionCodes": ["130", "131"]}
                ]},
                {"track": "internal", "releases": [{"versionCodes": ["150"]}]},
                {"track": "beta"}
            ]
        }))
        .unwrap();
        let bundles: PlayBundles = serde_json::from_value(serde_json::json!({
            "bundles": [{"versionCode": 140, "sha256": "x"}, {"versionCode": 150}]
        }))
        .unwrap();
        parse_play_version_codes(tracks, bundles, track).unwrap()
    }

    #[test]
    fn reads_target_track_highest_and_every_used_code() {
        let production = codes("production");
        assert_eq!(production.track_highest, Some(131));
        assert_eq!(
            production.used.iter().copied().collect::<Vec<_>>(),
            [120, 130, 131, 140, 150]
        );
        assert_eq!(codes("beta").track_highest, None);
    }

    #[test]
    fn rejects_codes_not_above_the_target_track_naming_both_numbers() {
        let error = ensure_play_version_code(42, &codes("production"), "production")
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("Version code 42 is not higher than 131"),
            "{error}"
        );
        assert!(error.contains("by at least 90"), "{error}");
        assert!(ensure_play_version_code(131, &codes("production"), "production").is_err());
    }

    #[test]
    fn rejects_codes_already_uploaded_elsewhere() {
        let error = ensure_play_version_code(140, &codes("production"), "production")
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("Version code 140 was already uploaded"),
            "{error}"
        );
        assert!(error.contains("highest uploaded is 150"), "{error}");
    }

    #[test]
    fn accepts_unused_codes_above_the_track() {
        ensure_play_version_code(135, &codes("production"), "production").unwrap();
        ensure_play_version_code(1, &PlayVersionCodes::default(), "internal").unwrap();
    }

    #[tokio::test]
    async fn fetch_reads_through_a_deleted_edit() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for _ in 0..4 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = head.lines().next().unwrap().to_string();
                assert!(head.contains("authorization: Bearer token"), "{head}");
                let body = if line.starts_with("POST") {
                    r#"{"id":"e1"}"#
                } else if line.contains("/tracks") {
                    r#"{"tracks":[{"track":"production","releases":[{"versionCodes":["7"]}]}]}"#
                } else if line.contains("/bundles") {
                    r#"{"bundles":[{"versionCode":9}]}"#
                } else {
                    ""
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                requests.push(line);
            }
            requests
        });

        let client = reqwest::Client::new();
        let codes =
            fetch_play_version_codes_from(&base, &client, "token", "com.example", "production")
                .await
                .unwrap();
        assert_eq!(codes.track_highest, Some(7));
        assert_eq!(codes.used.iter().copied().collect::<Vec<_>>(), [7, 9]);

        let requests = server.await.unwrap();
        assert!(requests[0].starts_with("POST /applications/com.example/edits "));
        assert!(requests[1].starts_with("GET /applications/com.example/edits/e1/tracks "));
        assert!(requests[2].starts_with("GET /applications/com.example/edits/e1/bundles "));
        assert!(requests[3].starts_with("DELETE /applications/com.example/edits/e1 "));
    }

    #[tokio::test]
    async fn fetch_failure_carries_googles_error_message() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = r#"{"error":{"code":403,"message":"The caller does not have permission","status":"PERMISSION_DENIED"}}"#;
            let response = format!(
                "HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let client = reqwest::Client::new();
        let error =
            fetch_play_version_codes_from(&base, &client, "token", "com.example", "internal")
                .await
                .unwrap_err()
                .to_string();
        server.await.unwrap();
        assert!(
            error.starts_with(
                "Google Play could not open an edit (403 Forbidden): The caller does not have permission"
            ),
            "{error}"
        );
        assert!(error.contains("Users and permissions"), "{error}");
    }
}
