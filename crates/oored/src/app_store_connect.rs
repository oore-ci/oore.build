//! App Store Connect calls used after an IPA upload: find the processed build,
//! hand it to TestFlight or an App Store version, submit it for review and
//! drive Apple's phased release. Provisioning calls live in `apple_api`.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::apple_api::{self, AppleApiCredentials};

/// A failed App Store Connect call, worded for release history. The flag
/// says whether Apple refused the request (a 4xx answer such as a 409 for a
/// missing age rating), which a person has to fix, rather than the call
/// failing on the way (a timeout, a 5xx), which a retry may fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AscError(pub String, pub bool);

impl AscError {
    /// A failure a retry may fix.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into(), false)
    }

    /// Apple refused the request; someone has to change something.
    pub fn refused(message: impl Into<String>) -> Self {
        Self(message.into(), true)
    }

    pub fn is_refusal(&self) -> bool {
        self.1
    }
}

impl std::fmt::Display for AscError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) type AscResult<T> = Result<T, AscError>;

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

/// An App Store version and the state Apple shows for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AscVersion {
    pub id: String,
    pub version_string: Option<String>,
    /// `appVersionState`, or the older `appStoreState`.
    pub state: Option<String>,
}

impl AscVersion {
    fn from_resource(resource: Resource) -> Self {
        Self {
            version_string: attr_str(&resource, "versionString"),
            state: attr_str(&resource, "appVersionState")
                .or_else(|| attr_str(&resource, "appStoreState")),
            id: resource.id,
        }
    }
}

/// Whether Apple lets the developer change a version in `state`, such as
/// attaching another build.
pub fn version_is_editable(state: &str) -> bool {
    EDITABLE_VERSION_STATES
        .split(',')
        .any(|editable| editable == state)
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

/// An App Store version as the version list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AscListedVersion {
    pub id: String,
    pub version_string: String,
    pub state: String,
    /// The attached build's CFBundleVersion.
    pub build_number: Option<String>,
    pub build_id: Option<String>,
}

/// Versions and their builds from an `appStoreVersions` list that included
/// `build`.
fn listed_versions(document: &Value) -> Vec<AscListedVersion> {
    let builds: std::collections::HashMap<&str, &str> = document["included"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "builds")
        .filter_map(|item| {
            Some((
                item["id"].as_str()?,
                item.pointer("/attributes/version")?.as_str()?,
            ))
        })
        .collect();
    document["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|version| {
            let attributes = &version["attributes"];
            let build_id = version
                .pointer("/relationships/build/data/id")
                .and_then(Value::as_str);
            Some(AscListedVersion {
                id: version["id"].as_str()?.to_string(),
                version_string: attributes["versionString"].as_str()?.to_string(),
                state: attributes["appVersionState"]
                    .as_str()
                    .or_else(|| attributes["appStoreState"].as_str())?
                    .to_string(),
                build_number: build_id
                    .and_then(|id| builds.get(id))
                    .map(|number| (*number).to_string()),
                build_id: build_id.map(str::to_string),
            })
        })
        .collect()
}

/// Age rating questions with no answer, in words. Overrides, the Korea
/// rating and the info URL are optional, so they never count.
fn unanswered_age_rating_questions(attributes: &Value) -> Vec<String> {
    const OPTIONAL: &[&str] = &[
        "ageRatingOverride",
        "ageRatingOverrideV2",
        "koreaAgeRatingOverride",
        "gracRatingClassificationNumber",
        "developerAgeRatingInfoUrl",
        "kidsAgeBand",
    ];
    let Some(attributes) = attributes.as_object() else {
        return Vec::new();
    };
    let mut missing: Vec<String> = attributes
        .iter()
        .filter(|(key, value)| value.is_null() && !OPTIONAL.contains(&key.as_str()))
        .map(|(key, _)| words_of(key))
        .collect();
    missing.sort();
    missing
}

/// `gamblingSimulated` as "gambling simulated".
fn words_of(key: &str) -> String {
    let mut words = String::new();
    for character in key.chars() {
        if character.is_ascii_uppercase() {
            words.push(' ');
            words.push(character.to_ascii_lowercase());
        } else {
            words.push(character);
        }
    }
    words
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
            .map_err(|(_, body)| AscError::refused(body.0.error))
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
            .map_err(|error| AscError::new(format!("{scope} failed: {error}")))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let message = format!(
                "{scope} failed ({status}): {}",
                apple_api::parse_apple_error(&body)
            );
            // Rate limits and timeouts clear up on their own.
            let transient = status.is_server_error()
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status == reqwest::StatusCode::REQUEST_TIMEOUT;
            return Err(if transient {
                AscError::new(message)
            } else {
                AscError::refused(message)
            });
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
        serde_json::from_str(&body).map_err(|error| {
            AscError::new(format!("{scope} returned an invalid response: {error}"))
        })
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

    /// Hang and launch metrics for the app's recent versions, in Apple's
    /// Xcode metrics format.
    pub async fn perf_power_metrics(&self, app_id: &str) -> AscResult<Value> {
        let body = self
            .send(
                "Power and performance metrics",
                self.client
                    .get(apple_api::endpoint(&format!(
                        "/v1/apps/{app_id}/perfPowerMetrics"
                    )))
                    .query(&[
                        ("filter[metricType]", "HANG,LAUNCH"),
                        ("filter[platform]", "IOS"),
                    ])
                    .header(
                        reqwest::header::ACCEPT,
                        "application/vnd.apple.xcode-metrics+json",
                    ),
            )
            .await?
            .unwrap_or_default();
        serde_json::from_str(if body.is_empty() { "{}" } else { &body }).map_err(|error| {
            AscError::new(format!(
                "Power and performance metrics returned an invalid response: {error}"
            ))
        })
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
        created.map(|version| version.id).ok_or_else(|| {
            AscError::new("App Store version creation returned no version".to_string())
        })
    }

    /// The app's App Store version with this version string, in any state,
    /// with its state. Promotion only; upload distribution keeps
    /// `find_or_create_version`.
    pub async fn find_version(&self, app_id: &str, version: &str) -> AscResult<Option<AscVersion>> {
        let versions: ListEnvelope = self
            .get(
                "App Store version lookup",
                &format!("/v1/apps/{app_id}/appStoreVersions"),
                &[
                    ("filter[versionString]", version),
                    ("filter[platform]", "IOS"),
                    (
                        "fields[appStoreVersions]",
                        "versionString,appVersionState,appStoreState",
                    ),
                    ("limit", "1"),
                ],
            )
            .await?;
        Ok(versions
            .data
            .into_iter()
            .next()
            .map(AscVersion::from_resource))
    }

    /// The app's version that Apple still lets the developer edit, if any,
    /// with its version string and state. Promotion only.
    pub async fn find_editable_version(&self, app_id: &str) -> AscResult<Option<AscVersion>> {
        let editable: ListEnvelope = self
            .get(
                "Editable App Store version lookup",
                &format!("/v1/apps/{app_id}/appStoreVersions"),
                &[
                    ("filter[appVersionState]", EDITABLE_VERSION_STATES),
                    ("filter[platform]", "IOS"),
                    (
                        "fields[appStoreVersions]",
                        "versionString,appVersionState,appStoreState",
                    ),
                    ("limit", "1"),
                ],
            )
            .await?;
        Ok(editable
            .data
            .into_iter()
            .next()
            .map(AscVersion::from_resource))
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
    pub async fn submit_for_review(
        &self,
        app_id: &str,
        version_id: &str,
        cancel_previous: bool,
    ) -> AscResult<()> {
        if version_is_submitted(&self.version_state(version_id).await?) {
            return Ok(());
        }
        // A rejected version waits in a submission with unresolved issues;
        // submitting that one again is a resubmission.
        let open: ListEnvelope = self
            .get(
                "Review submission lookup",
                "/v1/reviewSubmissions",
                &[
                    ("filter[app]", app_id),
                    ("filter[platform]", "IOS"),
                    ("filter[state]", "READY_FOR_REVIEW,UNRESOLVED_ISSUES"),
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
                    AscError::new("Review submission creation returned no submission".to_string())
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
            if !cancel_previous {
                return Err(AscError::refused(
                    "App Store Connect already has a draft review submission with other items; submit or remove it in App Store Connect, then Oore retries".to_string(),
                ));
            }
            // Like Codemagic's --cancel-previous-submissions: cancel the
            // other submission, and the next poll starts a new one.
            self.patch(
                "Canceling the previous review submission",
                &format!("/v1/reviewSubmissions/{submission_id}"),
                json!({
                    "data": {
                        "type": "reviewSubmissions",
                        "id": submission_id,
                        "attributes": { "canceled": true }
                    }
                }),
            )
            .await?;
            return Err(AscError::new(
                "Canceled the previous review submission in App Store Connect; submitting this version next",
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

    /// Sets when Apple releases the version once App Review approves it.
    pub async fn set_release_type(
        &self,
        version_id: &str,
        release_type: &str,
        earliest_release_date: Option<&str>,
    ) -> AscResult<()> {
        let mut attributes = json!({ "releaseType": release_type });
        if release_type == "SCHEDULED"
            && let Some(date) = earliest_release_date
        {
            attributes["earliestReleaseDate"] = json!(date);
        }
        self.patch(
            "App Store release type update",
            &format!("/v1/appStoreVersions/{version_id}"),
            json!({
                "data": {
                    "type": "appStoreVersions",
                    "id": version_id,
                    "attributes": attributes
                }
            }),
        )
        .await
    }

    /// The id of the build attached to the version, if any.
    pub async fn version_build_id(&self, version_id: &str) -> AscResult<Option<String>> {
        let build: OptionalEnvelope = self
            .get(
                "App Store version build lookup",
                &format!("/v1/appStoreVersions/{version_id}/build"),
                &[("fields[builds]", "version")],
            )
            .await?;
        Ok(build.data.map(|build| build.id))
    }

    /// Releases an approved version that waits for the developer
    /// (`PENDING_DEVELOPER_RELEASE`) or for its scheduled date.
    pub async fn release_version(&self, version_id: &str) -> AscResult<()> {
        self.post(
            "App Store version release",
            "/v1/appStoreVersionReleaseRequests",
            json!({
                "data": {
                    "type": "appStoreVersionReleaseRequests",
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

    /// Takes the version out of App Review by canceling the review
    /// submission that holds it (a developer reject).
    pub async fn cancel_review(&self, app_id: &str, version_id: &str) -> AscResult<()> {
        let submissions: ListEnvelope = self
            .get(
                "Review submission lookup",
                "/v1/reviewSubmissions",
                &[
                    ("filter[app]", app_id),
                    ("filter[platform]", "IOS"),
                    (
                        "filter[state]",
                        "WAITING_FOR_REVIEW,IN_REVIEW,UNRESOLVED_ISSUES",
                    ),
                    ("limit", "10"),
                ],
            )
            .await?;
        for submission in submissions.data {
            let items: Value = self
                .get(
                    "Review submission items lookup",
                    &format!("/v1/reviewSubmissions/{}/items", submission.id),
                    &[
                        ("fields[reviewSubmissionItems]", "appStoreVersion"),
                        ("include", "appStoreVersion"),
                        ("fields[appStoreVersions]", "versionString"),
                        ("limit", "50"),
                    ],
                )
                .await?;
            if submission_item_versions(&items)
                .iter()
                .any(|item| item.as_deref() == Some(version_id))
            {
                return self
                    .patch(
                        "Review submission cancellation",
                        &format!("/v1/reviewSubmissions/{}", submission.id),
                        json!({
                            "data": {
                                "type": "reviewSubmissions",
                                "id": submission.id,
                                "attributes": { "canceled": true }
                            }
                        }),
                    )
                    .await;
            }
        }
        Err(AscError::refused(
            "App Store Connect has no review submission for this version to cancel",
        ))
    }

    /// The app's newest App Store versions, newest first, with their build.
    pub async fn list_versions(
        &self,
        app_id: &str,
        limit: u32,
    ) -> AscResult<Vec<AscListedVersion>> {
        let limit = limit.to_string();
        let versions: Value = self
            .get(
                "App Store versions lookup",
                &format!("/v1/apps/{app_id}/appStoreVersions"),
                &[
                    ("filter[platform]", "IOS"),
                    (
                        "fields[appStoreVersions]",
                        "versionString,appVersionState,appStoreState,releaseType,build,createdDate",
                    ),
                    ("include", "build"),
                    ("fields[builds]", "version"),
                    ("limit", &limit),
                ],
            )
            .await?;
        Ok(listed_versions(&versions))
    }

    /// The age rating questions the app's editable App Info leaves
    /// unanswered.
    pub async fn age_rating_gaps(&self, app_id: &str) -> AscResult<Vec<String>> {
        let infos: ListEnvelope = self
            .get(
                "App info lookup",
                &format!("/v1/apps/{app_id}/appInfos"),
                &[("fields[appInfos]", "state,appStoreState"), ("limit", "5")],
            )
            .await?;
        let info = infos
            .data
            .iter()
            .find(|info| {
                matches!(
                    attr_str(info, "state").as_deref(),
                    Some("PREPARE_FOR_SUBMISSION" | "DEVELOPER_REJECTED" | "REJECTED")
                )
            })
            .or_else(|| infos.data.first());
        let Some(info) = info else {
            return Ok(Vec::new());
        };
        let declaration: OptionalEnvelope = self
            .get(
                "Age rating lookup",
                &format!("/v1/appInfos/{}/ageRatingDeclaration", info.id),
                &[],
            )
            .await?;
        Ok(declaration
            .data
            .map(|declaration| unanswered_age_rating_questions(&declaration.attributes))
            .unwrap_or_default())
    }

    /// The review contact fields the version leaves empty.
    pub async fn review_detail_gaps(&self, version_id: &str) -> AscResult<Vec<String>> {
        let detail: OptionalEnvelope = self
            .get(
                "App Review details lookup",
                &format!("/v1/appStoreVersions/{version_id}/appStoreReviewDetail"),
                &[],
            )
            .await?;
        let Some(detail) = detail.data else {
            return Ok(vec!["App Review contact information".to_string()]);
        };
        Ok([
            ("contactFirstName", "contact first name"),
            ("contactLastName", "contact last name"),
            ("contactPhone", "contact phone"),
            ("contactEmail", "contact email"),
        ]
        .into_iter()
        .filter(|(key, _)| {
            attr_str(&detail, key)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|(_, label)| label.to_string())
        .collect())
    }

    /// Whether an open review submission holds items other than App Store
    /// versions, which would stop Oore's submission.
    pub async fn open_submission_has_other_items(&self, app_id: &str) -> AscResult<bool> {
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
        let Some(submission) = open.data.into_iter().next() else {
            return Ok(false);
        };
        let items: Value = self
            .get(
                "Review submission items lookup",
                &format!("/v1/reviewSubmissions/{}/items", submission.id),
                &[
                    ("fields[reviewSubmissionItems]", "appStoreVersion"),
                    ("limit", "50"),
                ],
            )
            .await?;
        Ok(submission_item_versions(&items).iter().any(Option::is_none))
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
            .ok_or_else(|| AscError::new("App Store version has no state".to_string()))
    }
}

/// The App Store Connect calls that hand a processed build out, to TestFlight
/// groups or an App Store version. Distribution after an upload and
/// promotion between environments share them; tests stand in for Apple.
pub(crate) trait AscApi {
    async fn find_app_id(&self, bundle_id: &str) -> AscResult<Option<String>>;
    async fn beta_detail(&self, build_id: &str) -> AscResult<AscBetaDetail>;
    async fn set_beta_whats_new(
        &self,
        build_id: &str,
        locale: &str,
        whats_new: &str,
    ) -> AscResult<()>;
    async fn list_beta_groups(&self, app_id: &str) -> AscResult<Vec<AscBetaGroup>>;
    async fn add_build_to_groups(&self, build_id: &str, group_ids: &[String]) -> AscResult<()>;
    async fn submit_for_beta_review(&self, build_id: &str) -> AscResult<()>;
    async fn find_or_create_version(&self, app_id: &str, version: &str) -> AscResult<String>;
    async fn find_version(&self, app_id: &str, version: &str) -> AscResult<Option<AscVersion>>;
    async fn find_editable_version(&self, app_id: &str) -> AscResult<Option<AscVersion>>;
    async fn attach_build_to_version(&self, version_id: &str, build_id: &str) -> AscResult<()>;
    async fn set_version_whats_new(
        &self,
        version_id: &str,
        locale: &str,
        whats_new: &str,
    ) -> AscResult<()>;
    async fn enable_phased_release(&self, version_id: &str) -> AscResult<()>;
    async fn disable_phased_release(&self, version_id: &str) -> AscResult<()>;
    async fn submit_for_review(
        &self,
        app_id: &str,
        version_id: &str,
        cancel_previous: bool,
    ) -> AscResult<()>;
    async fn set_release_type(
        &self,
        version_id: &str,
        release_type: &str,
        earliest_release_date: Option<&str>,
    ) -> AscResult<()>;
}

impl AscApi for AppStoreConnect<'_> {
    async fn find_app_id(&self, bundle_id: &str) -> AscResult<Option<String>> {
        AppStoreConnect::find_app_id(self, bundle_id).await
    }
    async fn beta_detail(&self, build_id: &str) -> AscResult<AscBetaDetail> {
        AppStoreConnect::beta_detail(self, build_id).await
    }
    async fn set_beta_whats_new(
        &self,
        build_id: &str,
        locale: &str,
        whats_new: &str,
    ) -> AscResult<()> {
        AppStoreConnect::set_beta_whats_new(self, build_id, locale, whats_new).await
    }
    async fn list_beta_groups(&self, app_id: &str) -> AscResult<Vec<AscBetaGroup>> {
        AppStoreConnect::list_beta_groups(self, app_id).await
    }
    async fn add_build_to_groups(&self, build_id: &str, group_ids: &[String]) -> AscResult<()> {
        AppStoreConnect::add_build_to_groups(self, build_id, group_ids).await
    }
    async fn submit_for_beta_review(&self, build_id: &str) -> AscResult<()> {
        AppStoreConnect::submit_for_beta_review(self, build_id).await
    }
    async fn find_or_create_version(&self, app_id: &str, version: &str) -> AscResult<String> {
        AppStoreConnect::find_or_create_version(self, app_id, version).await
    }
    async fn find_version(&self, app_id: &str, version: &str) -> AscResult<Option<AscVersion>> {
        AppStoreConnect::find_version(self, app_id, version).await
    }
    async fn find_editable_version(&self, app_id: &str) -> AscResult<Option<AscVersion>> {
        AppStoreConnect::find_editable_version(self, app_id).await
    }
    async fn attach_build_to_version(&self, version_id: &str, build_id: &str) -> AscResult<()> {
        AppStoreConnect::attach_build_to_version(self, version_id, build_id).await
    }
    async fn set_version_whats_new(
        &self,
        version_id: &str,
        locale: &str,
        whats_new: &str,
    ) -> AscResult<()> {
        AppStoreConnect::set_version_whats_new(self, version_id, locale, whats_new).await
    }
    async fn enable_phased_release(&self, version_id: &str) -> AscResult<()> {
        AppStoreConnect::enable_phased_release(self, version_id).await
    }
    async fn disable_phased_release(&self, version_id: &str) -> AscResult<()> {
        AppStoreConnect::disable_phased_release(self, version_id).await
    }
    async fn submit_for_review(
        &self,
        app_id: &str,
        version_id: &str,
        cancel_previous: bool,
    ) -> AscResult<()> {
        AppStoreConnect::submit_for_review(self, app_id, version_id, cancel_previous).await
    }
    async fn set_release_type(
        &self,
        version_id: &str,
        release_type: &str,
        earliest_release_date: Option<&str>,
    ) -> AscResult<()> {
        AppStoreConnect::set_release_type(self, version_id, release_type, earliest_release_date)
            .await
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
        assert!(version_is_editable("PREPARE_FOR_SUBMISSION"));
        assert!(version_is_editable("DEVELOPER_REJECTED"));
        assert!(!version_is_editable("READY_FOR_SALE"));
        assert!(!version_is_editable("WAITING_FOR_REVIEW"));
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
