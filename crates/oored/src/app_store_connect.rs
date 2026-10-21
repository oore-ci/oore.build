//! App Store Connect calls used after an IPA upload: find the processed build,
//! hand it to TestFlight or an App Store version, submit it for review and
//! drive Apple's phased release. Provisioning calls live in `apple_api`.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::apple_api::{self, AppleApiCredentials};

/// A failed App Store Connect call, worded for release history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AscError(pub String);

impl std::fmt::Display for AscError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

type AscResult<T> = Result<T, AscError>;

#[derive(Debug, Deserialize)]
struct Resource {
    id: String,
    #[serde(default)]
    attributes: Value,
}

#[derive(Debug, Deserialize)]
struct ListEnvelope {
    data: Vec<Resource>,
}

#[derive(Debug, Deserialize)]
struct OneEnvelope {
    data: Resource,
}

#[derive(Debug, Deserialize)]
struct OptionalEnvelope {
    data: Option<Resource>,
}

/// Apple's processing state for an uploaded build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProcessing {
    Processing,
    Valid,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AscBuild {
    pub id: String,
    pub processing: BuildProcessing,
    pub uses_non_exempt_encryption: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AscBetaGroup {
    pub id: String,
    pub name: String,
    pub is_internal: bool,
    pub has_access_to_all_builds: bool,
}

/// A build's TestFlight states, from its `buildBetaDetail`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AscBetaDetail {
    /// Such as `PROCESSING`, `MISSING_EXPORT_COMPLIANCE`, `READY_FOR_BETA_TESTING`
    /// or `IN_BETA_TESTING`.
    pub internal: Option<String>,
    /// Such as `READY_FOR_BETA_SUBMISSION`, `WAITING_FOR_BETA_REVIEW`,
    /// `IN_BETA_TESTING` or `BETA_REJECTED`.
    pub external: Option<String>,
}

/// App Store versions Apple still lets the developer edit, so one of them can
/// take a new version string instead of a second version being created.
const EDITABLE_VERSION_STATES: &str =
    "PREPARE_FOR_SUBMISSION,DEVELOPER_REJECTED,REJECTED,METADATA_REJECTED,INVALID_BINARY";

/// Version states that mean the version is already with App Review.
fn version_is_submitted(state: &str) -> bool {
    matches!(state, "WAITING_FOR_REVIEW" | "IN_REVIEW")
}

/// Item IDs of a review submission's App Store versions, from a
/// `reviewSubmissions/{id}/items` response. Items for other things (in-app
/// events, custom product pages) come back as `None`.
fn submission_item_versions(items: &Value) -> Vec<Option<String>> {
    items["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.pointer("/relationships/appStoreVersion/data/id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct AscPhasedRelease {
    pub id: String,
    pub state: String,
    pub current_day_number: Option<i64>,
}

pub struct AppStoreConnect<'a> {
    client: &'a reqwest::Client,
    creds: &'a AppleApiCredentials,
}

fn attr_str(resource: &Resource, key: &str) -> Option<String> {
    resource
        .attributes
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn attr_bool(resource: &Resource, key: &str) -> Option<bool> {
    resource.attributes.get(key).and_then(Value::as_bool)
}

fn parse_processing(raw: Option<&str>) -> BuildProcessing {
    match raw {
        Some("VALID") => BuildProcessing::Valid,
        Some("FAILED" | "INVALID") => BuildProcessing::Failed,
        _ => BuildProcessing::Processing,
    }
}

impl<'a> AppStoreConnect<'a> {
    pub fn new(client: &'a reqwest::Client, creds: &'a AppleApiCredentials) -> Self {
        Self { client, creds }
    }

    fn token(&self) -> AscResult<String> {
        apple_api::generate_app_store_connect_jwt(self.creds)
            .map_err(|(_, body)| AscError(body.0.error))
    }

    async fn send(
        &self,
        scope: &str,
        request: reqwest::RequestBuilder,
    ) -> AscResult<Option<String>> {
        let response = request
            .bearer_auth(self.token()?)
            .send()
            .await
            .map_err(|error| AscError(format!("{scope} failed: {error}")))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AscError(format!(
                "{scope} failed ({status}): {}",
                apple_api::parse_apple_error(&body)
            )));
        }
        Ok((!body.trim().is_empty()).then_some(body))
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        scope: &str,
        path: &str,
        query: &[(&str, &str)],
    ) -> AscResult<T> {
        let body = self
            .send(
                scope,
                self.client.get(apple_api::endpoint(path)).query(query),
            )
            .await?
            .unwrap_or_default();
        serde_json::from_str(&body)
            .map_err(|error| AscError(format!("{scope} returned an invalid response: {error}")))
    }

    async fn post(&self, scope: &str, path: &str, body: Value) -> AscResult<Option<Resource>> {
        let body = self
            .send(
                scope,
                self.client.post(apple_api::endpoint(path)).json(&body),
            )
            .await?;
        Ok(body
            .and_then(|raw| serde_json::from_str::<OneEnvelope>(&raw).ok())
            .map(|envelope| envelope.data))
    }

    async fn delete(&self, scope: &str, path: &str) -> AscResult<()> {
        self.send(scope, self.client.delete(apple_api::endpoint(path)))
            .await
            .map(|_| ())
    }

    async fn patch(&self, scope: &str, path: &str, body: Value) -> AscResult<()> {
        self.send(
            scope,
            self.client.patch(apple_api::endpoint(path)).json(&body),
        )
        .await
        .map(|_| ())
    }

    pub async fn find_app_id(&self, bundle_id: &str) -> AscResult<Option<String>> {
        let apps: ListEnvelope = self
            .get(
                "App lookup",
                "/v1/apps",
                &[
                    ("filter[bundleId]", bundle_id),
                    ("fields[apps]", "bundleId"),
                    ("limit", "5"),
                ],
            )
            .await?;
        Ok(apps
            .data
            .into_iter()
            .find(|app| attr_str(app, "bundleId").as_deref() == Some(bundle_id))
            .map(|app| app.id))
    }

    /// Finds the build uploaded for `version` (CFBundleShortVersionString) and
    /// `build_number` (CFBundleVersion). Apple lists a build only after it has
    /// started processing, so `None` is normal shortly after upload.
    pub async fn find_build(
        &self,
        app_id: &str,
        version: &str,
        build_number: &str,
    ) -> AscResult<Option<AscBuild>> {
        let builds: ListEnvelope = self
            .get(
                "Build lookup",
                "/v1/builds",
                &[
                    ("filter[app]", app_id),
                    ("filter[version]", build_number),
                    ("filter[preReleaseVersion.version]", version),
                    ("filter[preReleaseVersion.platform]", "IOS"),
                    (
                        "fields[builds]",
                        "version,processingState,usesNonExemptEncryption",
                    ),
                    ("limit", "1"),
                ],
            )
            .await?;
        Ok(builds.data.into_iter().next().map(|build| AscBuild {
            processing: parse_processing(attr_str(&build, "processingState").as_deref()),
            uses_non_exempt_encryption: attr_bool(&build, "usesNonExemptEncryption"),
            id: build.id,
        }))
    }

    pub async fn set_export_compliance(
        &self,
        build_id: &str,
        uses_non_exempt_encryption: bool,
    ) -> AscResult<()> {
        self.patch(
            "Export compliance update",
            &format!("/v1/builds/{build_id}"),
            json!({
                "data": {
                    "type": "builds",
                    "id": build_id,
                    "attributes": { "usesNonExemptEncryption": uses_non_exempt_encryption }
                }
            }),
        )
        .await
    }

    pub async fn list_beta_groups(&self, app_id: &str) -> AscResult<Vec<AscBetaGroup>> {
        let groups: ListEnvelope = self
            .get(
                "TestFlight group lookup",
                "/v1/betaGroups",
                &[
                    ("filter[app]", app_id),
                    (
                        "fields[betaGroups]",
                        "name,isInternalGroup,hasAccessToAllBuilds",
                    ),
                    ("limit", "200"),
                ],
            )
            .await?;
        Ok(groups
            .data
            .into_iter()
            .map(|group| AscBetaGroup {
                name: attr_str(&group, "name").unwrap_or_default(),
                is_internal: attr_bool(&group, "isInternalGroup").unwrap_or(false),
                has_access_to_all_builds: attr_bool(&group, "hasAccessToAllBuilds")
                    .unwrap_or(false),
                id: group.id,
            })
            .collect())
    }

    pub async fn add_build_to_groups(&self, build_id: &str, group_ids: &[String]) -> AscResult<()> {
        if group_ids.is_empty() {
            return Ok(());
        }
        let data: Vec<Value> = group_ids
            .iter()
            .map(|id| json!({ "type": "betaGroups", "id": id }))
            .collect();
        self.post(
            "Adding build to TestFlight groups",
            &format!("/v1/builds/{build_id}/relationships/betaGroups"),
            json!({ "data": data }),
        )
        .await
        .map(|_| ())
    }

    pub async fn submit_for_beta_review(&self, build_id: &str) -> AscResult<()> {
        self.post(
            "TestFlight beta review submission",
            "/v1/betaAppReviewSubmissions",
            json!({
                "data": {
                    "type": "betaAppReviewSubmissions",
                    "relationships": {
                        "build": { "data": { "type": "builds", "id": build_id } }
                    }
                }
            }),
        )
        .await
        .map(|_| ())
    }

    pub async fn beta_detail(&self, build_id: &str) -> AscResult<AscBetaDetail> {
        let detail: OptionalEnvelope = self
            .get(
                "TestFlight state lookup",
                &format!("/v1/builds/{build_id}/buildBetaDetail"),
                &[(
                    "fields[buildBetaDetails]",
                    "internalBuildState,externalBuildState",
                )],
            )
            .await?;
        Ok(detail
            .data
            .map(|detail| AscBetaDetail {
                internal: attr_str(&detail, "internalBuildState"),
                external: attr_str(&detail, "externalBuildState"),
            })
            .unwrap_or_default())
    }

    pub async fn set_beta_whats_new(
        &self,
        build_id: &str,
        locale: &str,
        whats_new: &str,
    ) -> AscResult<()> {
        let existing: ListEnvelope = self
            .get(
                "TestFlight notes lookup",
                &format!("/v1/builds/{build_id}/betaBuildLocalizations"),
                &[
                    ("fields[betaBuildLocalizations]", "locale"),
                    ("limit", "50"),
                ],
            )
            .await?;
        if let Some(localization) = existing
            .data
            .into_iter()
            .find(|item| attr_str(item, "locale").as_deref() == Some(locale))
        {
            return self
                .patch(
                    "TestFlight notes update",
                    &format!("/v1/betaBuildLocalizations/{}", localization.id),
                    json!({
                        "data": {
                            "type": "betaBuildLocalizations",
                            "id": localization.id,
                            "attributes": { "whatsNew": whats_new }
                        }
                    }),
                )
                .await;
        }
        self.post(
            "TestFlight notes creation",
            "/v1/betaBuildLocalizations",
            json!({
                "data": {
                    "type": "betaBuildLocalizations",
                    "attributes": { "locale": locale, "whatsNew": whats_new },
                    "relationships": {
                        "build": { "data": { "type": "builds", "id": build_id } }
                    }
                }
            }),
        )
        .await
        .map(|_| ())
    }

    /// Returns the App Store version for `version`. Like fastlane's
    /// `ensure_version!`, an editable version with another version string is
    /// renamed rather than a second one created, which Apple would refuse.
    pub async fn find_or_create_version(&self, app_id: &str, version: &str) -> AscResult<String> {
        let versions: ListEnvelope = self
            .get(
                "App Store version lookup",
                &format!("/v1/apps/{app_id}/appStoreVersions"),
                &[
                    ("filter[versionString]", version),
                    ("filter[platform]", "IOS"),
                    ("fields[appStoreVersions]", "versionString"),
                    ("limit", "1"),
                ],
            )
            .await?;
        if let Some(existing) = versions.data.into_iter().next() {
            return Ok(existing.id);
        }
        let editable: ListEnvelope = self
            .get(
                "Editable App Store version lookup",
                &format!("/v1/apps/{app_id}/appStoreVersions"),
                &[
                    ("filter[appVersionState]", EDITABLE_VERSION_STATES),
                    ("filter[platform]", "IOS"),
                    ("fields[appStoreVersions]", "versionString"),
                    ("limit", "1"),
                ],
            )
            .await?;
        if let Some(existing) = editable.data.into_iter().next() {
            self.patch(
                "App Store version rename",
                &format!("/v1/appStoreVersions/{}", existing.id),
                json!({
                    "data": {
                        "type": "appStoreVersions",
                        "id": existing.id,
                        "attributes": { "versionString": version }
                    }
                }),
            )
            .await?;
            return Ok(existing.id);
        }
        let created = self
            .post(
                "App Store version creation",
                "/v1/appStoreVersions",
                json!({
                    "data": {
                        "type": "appStoreVersions",
                        "attributes": { "platform": "IOS", "versionString": version },
                        "relationships": {
                            "app": { "data": { "type": "apps", "id": app_id } }
                        }
                    }
                }),
            )
            .await?;
        created
            .map(|version| version.id)
            .ok_or_else(|| AscError("App Store version creation returned no version".to_string()))
    }

    pub async fn attach_build_to_version(&self, version_id: &str, build_id: &str) -> AscResult<()> {
        self.patch(
            "Attaching build to App Store version",
            &format!("/v1/appStoreVersions/{version_id}/relationships/build"),
            json!({ "data": { "type": "builds", "id": build_id } }),
        )
        .await
    }

    pub async fn set_version_whats_new(
        &self,
        version_id: &str,
        locale: &str,
        whats_new: &str,
    ) -> AscResult<()> {
        let existing: ListEnvelope = self
            .get(
                "App Store notes lookup",
                &format!("/v1/appStoreVersions/{version_id}/appStoreVersionLocalizations"),
                &[
                    ("fields[appStoreVersionLocalizations]", "locale"),
                    ("limit", "50"),
                ],
            )
            .await?;
        if let Some(localization) = existing
            .data
            .into_iter()
            .find(|item| attr_str(item, "locale").as_deref() == Some(locale))
        {
            return self
                .patch(
                    "App Store notes update",
                    &format!("/v1/appStoreVersionLocalizations/{}", localization.id),
                    json!({
                        "data": {
                            "type": "appStoreVersionLocalizations",
                            "id": localization.id,
                            "attributes": { "whatsNew": whats_new }
                        }
                    }),
                )
                .await;
        }
        self.post(
            "App Store notes creation",
            "/v1/appStoreVersionLocalizations",
            json!({
                "data": {
                    "type": "appStoreVersionLocalizations",
                    "attributes": { "locale": locale, "whatsNew": whats_new },
                    "relationships": {
                        "appStoreVersion": {
                            "data": { "type": "appStoreVersions", "id": version_id }
                        }
                    }
                }
            }),
        )
        .await
        .map(|_| ())
    }

    pub async fn phased_release(&self, version_id: &str) -> AscResult<Option<AscPhasedRelease>> {
        let phased: OptionalEnvelope = self
            .get(
                "Phased release lookup",
                &format!("/v1/appStoreVersions/{version_id}/appStoreVersionPhasedRelease"),
                &[],
            )
            .await?;
        Ok(phased.data.map(|phased| AscPhasedRelease {
            state: attr_str(&phased, "phasedReleaseState").unwrap_or_default(),
            current_day_number: phased
                .attributes
                .get("currentDayNumber")
                .and_then(Value::as_i64),
            id: phased.id,
        }))
    }

    pub async fn enable_phased_release(&self, version_id: &str) -> AscResult<()> {
        if self.phased_release(version_id).await?.is_some() {
            return Ok(());
        }
        self.post(
            "Phased release setup",
            "/v1/appStoreVersionPhasedReleases",
            json!({
                "data": {
                    "type": "appStoreVersionPhasedReleases",
                    "attributes": { "phasedReleaseState": "INACTIVE" },
                    "relationships": {
                        "appStoreVersion": {
                            "data": { "type": "appStoreVersions", "id": version_id }
                        }
                    }
                }
            }),
        )
        .await
        .map(|_| ())
    }

    /// Removes a phased release that has not started, so the version releases
    /// to everyone at once.
    pub async fn disable_phased_release(&self, version_id: &str) -> AscResult<()> {
        match self.phased_release(version_id).await? {
            Some(phased) if phased.state == "INACTIVE" => {
                self.delete(
                    "Phased release removal",
                    &format!("/v1/appStoreVersionPhasedReleases/{}", phased.id),
                )
                .await
            }
            _ => Ok(()),
        }
    }

    /// Sets the phased release to `ACTIVE`, `PAUSED` or `COMPLETE`.
    pub async fn set_phased_release_state(&self, phased_id: &str, state: &str) -> AscResult<()> {
        self.patch(
            "Phased release update",
            &format!("/v1/appStoreVersionPhasedReleases/{phased_id}"),
            json!({
                "data": {
                    "type": "appStoreVersionPhasedReleases",
                    "id": phased_id,
                    "attributes": { "phasedReleaseState": state }
                }
            }),
        )
        .await
    }

    /// Submits the version for App Review through a review submission. Safe
    /// to call again after a partial failure: a version already submitted or
    /// already in the open submission is not added twice. Like fastlane's
    /// deliver, it stops when the open submission holds items it did not add.
    pub async fn submit_for_review(&self, app_id: &str, version_id: &str) -> AscResult<()> {
        if version_is_submitted(&self.version_state(version_id).await?) {
            return Ok(());
        }
        let open: ListEnvelope = self
            .get(
                "Review submission lookup",
                "/v1/reviewSubmissions",
                &[
                    ("filter[app]", app_id),
                    ("filter[platform]", "IOS"),
                    ("filter[state]", "READY_FOR_REVIEW"),
                    ("limit", "1"),
                ],
            )
            .await?;
        let submission_id = match open.data.into_iter().next() {
            Some(existing) => existing.id,
            None => self
                .post(
                    "Review submission creation",
                    "/v1/reviewSubmissions",
                    json!({
                        "data": {
                            "type": "reviewSubmissions",
                            "attributes": { "platform": "IOS" },
                            "relationships": {
                                "app": { "data": { "type": "apps", "id": app_id } }
                            }
                        }
                    }),
                )
                .await?
                .map(|submission| submission.id)
                .ok_or_else(|| {
                    AscError("Review submission creation returned no submission".to_string())
                })?,
        };
        let items: Value = self
            .get(
                "Review submission items lookup",
                &format!("/v1/reviewSubmissions/{submission_id}/items"),
                &[
                    ("fields[reviewSubmissionItems]", "appStoreVersion"),
                    ("include", "appStoreVersion"),
                    ("fields[appStoreVersions]", "versionString"),
                    ("limit", "50"),
                ],
            )
            .await?;
        let item_versions = submission_item_versions(&items);
        if item_versions
            .iter()
            .any(|item| item.as_deref() != Some(version_id))
        {
            return Err(AscError(
                "App Store Connect already has a draft review submission with other items; submit or remove it in App Store Connect, then Oore retries".to_string(),
            ));
        }
        if item_versions.is_empty() {
            self.post(
                "Adding version to review submission",
                "/v1/reviewSubmissionItems",
                json!({
                    "data": {
                        "type": "reviewSubmissionItems",
                        "relationships": {
                            "reviewSubmission": {
                                "data": { "type": "reviewSubmissions", "id": submission_id }
                            },
                            "appStoreVersion": {
                                "data": { "type": "appStoreVersions", "id": version_id }
                            }
                        }
                    }
                }),
            )
            .await?;
        }
        self.patch(
            "Review submission",
            &format!("/v1/reviewSubmissions/{submission_id}"),
            json!({
                "data": {
                    "type": "reviewSubmissions",
                    "id": submission_id,
                    "attributes": { "submitted": true }
                }
            }),
        )
        .await
    }

    /// The version's App Store state, preferring `appVersionState` and falling
    /// back to the older `appStoreState`.
    pub async fn version_state(&self, version_id: &str) -> AscResult<String> {
        let version: OneEnvelope = self
            .get(
                "App Store version state lookup",
                &format!("/v1/appStoreVersions/{version_id}"),
                &[("fields[appStoreVersions]", "appStoreState,appVersionState")],
            )
            .await?;
        attr_str(&version.data, "appVersionState")
            .or_else(|| attr_str(&version.data, "appStoreState"))
            .ok_or_else(|| AscError("App Store version has no state".to_string()))
    }
}

/// Share of users Apple's phased release has reached on a given day.
pub fn phased_release_fraction(day: Option<i64>) -> f64 {
    match day.unwrap_or(1) {
        i64::MIN..=1 => 0.01,
        2 => 0.02,
        3 => 0.05,
        4 => 0.10,
        5 => 0.20,
        6 => 0.50,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processing_state_maps_terminal_values() {
        assert_eq!(parse_processing(Some("VALID")), BuildProcessing::Valid);
        assert_eq!(parse_processing(Some("INVALID")), BuildProcessing::Failed);
        assert_eq!(parse_processing(Some("FAILED")), BuildProcessing::Failed);
        assert_eq!(
            parse_processing(Some("PROCESSING")),
            BuildProcessing::Processing
        );
        assert_eq!(parse_processing(None), BuildProcessing::Processing);
    }

    #[test]
    fn submission_items_name_their_versions() {
        let items = json!({
            "data": [
                {"type": "reviewSubmissionItems", "id": "i1",
                 "relationships": {"appStoreVersion": {"data": {"type": "appStoreVersions", "id": "v1"}}}},
                {"type": "reviewSubmissionItems", "id": "i2",
                 "relationships": {"appEvent": {"data": {"type": "appEvents", "id": "e1"}}}}
            ]
        });
        assert_eq!(
            submission_item_versions(&items),
            vec![Some("v1".to_string()), None]
        );
        assert!(submission_item_versions(&json!({"data": []})).is_empty());
        assert!(version_is_submitted("WAITING_FOR_REVIEW"));
        assert!(!version_is_submitted("PREPARE_FOR_SUBMISSION"));
    }

    #[test]
    fn phased_release_follows_apple_schedule() {
        let schedule: Vec<f64> = (1..=8)
            .map(|day| phased_release_fraction(Some(day)))
            .collect();
        assert_eq!(schedule, vec![0.01, 0.02, 0.05, 0.10, 0.20, 0.50, 1.0, 1.0]);
        assert_eq!(phased_release_fraction(None), 0.01);
    }
}
