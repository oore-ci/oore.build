//! The credentials a `.oore.yaml` version 2 file names. `oore pipeline plan`
//! prints them per workflow, and `oore pipeline validate --project` checks
//! them against the project's signing library and secrets.
//!
//! The file only names credentials. Which identity a build really signs with
//! can still change in Oore: a pipeline's workflow settings can pick another
//! identity or turn publishing off, and the iOS export method follows the
//! identity's profiles. Offline, the CLI shows what the file names.

use std::collections::BTreeSet;

use oore_contract::{
    BuildPlatform, IosExportMethod, ListSecretsResponse, ListSigningIdentitiesResponse,
    PipelineDetailResponse, PipelinePlan, PlanRequest, RepositoryConfig, SigningIdentity,
    SigningIdentityKind,
};

use crate::build_commands::{Api, ApiConnectionArgs};

/// A signing identity a plan names, with the field that names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedIdentity {
    pub platform: &'static str,
    pub name: String,
    pub field: String,
}

/// A secret a plan names outside `${{ secrets.NAME }}` references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedSecret {
    pub name: String,
    pub field: String,
}

/// `workflows.<name>.signing.<platform>` when the workflow names its own
/// identity, else the top-level `signing.<platform>` it inherits.
fn signing_field(config: &RepositoryConfig, workflow: Option<&str>, platform: &str) -> String {
    if let (RepositoryConfig::V2(config), Some(workflow)) = (config, workflow) {
        let own = config
            .workflows
            .get(&workflow.to_string())
            .and_then(|spec| spec.signing.as_ref())
            .is_some_and(|signing| match platform {
                "android" => signing.android.is_some(),
                _ => signing.ios.is_some(),
            });
        if own {
            return format!("workflows.{workflow}.signing.{platform}");
        }
    }
    format!("signing.{platform}")
}

/// The identities a plan signs with, for the platforms it builds.
pub(crate) fn named_identities(
    config: &RepositoryConfig,
    plan: &PipelinePlan,
) -> Vec<NamedIdentity> {
    [
        ("android", BuildPlatform::Android, &plan.signing.android),
        ("ios", BuildPlatform::Ios, &plan.signing.ios),
    ]
    .into_iter()
    .filter(|(_, platform, _)| plan.platforms.contains(platform))
    .filter_map(|(platform, _, name)| {
        let name = name.as_deref()?.trim();
        Some(NamedIdentity {
            platform,
            name: name.to_string(),
            field: signing_field(config, plan.workflow.as_deref(), platform),
        })
    })
    .collect()
}

/// The secrets a plan's store uploads name: Google Play's `service_account`.
pub(crate) fn named_secrets(plan: &PipelinePlan) -> Vec<NamedSecret> {
    plan.publish
        .play
        .as_ref()
        .and_then(|play| {
            let name = play.spec.service_account.as_deref()?.trim();
            Some(NamedSecret {
                name: name.to_string(),
                field: format!("{}.service_account", play.source),
            })
        })
        .into_iter()
        .collect()
}

/// The signing and store credential lines of a version 2 plan.
pub(crate) fn render_credentials(config: &RepositoryConfig, plan: &PipelinePlan) -> String {
    let mut out = String::new();
    let identities = named_identities(config, plan);
    let signed: Vec<(&str, &str, BuildPlatform)> = vec![
        ("android", "Android", BuildPlatform::Android),
        ("ios", "iOS", BuildPlatform::Ios),
    ]
    .into_iter()
    .filter(|(_, _, platform)| plan.platforms.contains(platform))
    .collect();
    if !signed.is_empty() {
        out.push_str("  signing:\n");
        for (key, label, _) in &signed {
            match identities.iter().find(|identity| identity.platform == *key) {
                Some(identity) => out.push_str(&format!(
                    "    {key}: identity {} ({})\n",
                    identity.name, identity.field
                )),
                None => out.push_str(&format!(
                    "    {key}: the pipeline's {label} signing settings\n"
                )),
            }
        }
    }
    if plan.platforms.contains(&BuildPlatform::Ios) {
        let identity = identities
            .iter()
            .find(|identity| identity.platform == "ios")
            .map(|identity| format!("identity {}", identity.name));
        let line = if plan.publish.needs_app_store_export() {
            format!(
                "App Store, because the workflow uploads to App Store Connect; {} must hold App Store profiles",
                identity.as_deref().unwrap_or("the pipeline's iOS signing")
            )
        } else {
            match identity {
                Some(identity) => {
                    format!("follows the profiles of {identity}: App Store or ad hoc")
                }
                None => "follows the pipeline's iOS signing settings".to_string(),
            }
        };
        out.push_str(&format!("  ios export: {line}\n"));
    }
    let mut stores = Vec::new();
    if let Some(play) = &plan.publish.play {
        stores.push(match named_secrets(plan).first() {
            Some(secret) => format!(
                "google play: service account secret {} ({})",
                secret.name, secret.field
            ),
            None => format!(
                "google play: the service account secret in the pipeline's Google Play settings ({} sets no service_account)",
                play.source
            ),
        });
    }
    if plan.publish.needs_app_store_export() {
        stores.push(
            "app store connect: the API key in the pipeline's iOS signing settings".to_string(),
        );
    }
    if !stores.is_empty() {
        out.push_str("  store credentials:\n");
        for line in stores {
            out.push_str(&format!("    {line}\n"));
        }
    }
    out
}

/// What a project holds, as the session can see it.
#[derive(Debug, Default)]
pub(crate) struct ProjectCredentials {
    pub identities: Vec<SigningIdentity>,
    /// Project secrets, plus the pipeline's when one was named, plus the
    /// instance's when the session may list them.
    pub secrets: BTreeSet<String>,
    pub pipeline_listed: bool,
    pub instance_listed: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CheckOutcome {
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    pub identities_checked: usize,
    pub secrets_checked: usize,
}

fn kind_label(kind: SigningIdentityKind) -> &'static str {
    match kind {
        SigningIdentityKind::AndroidKeystore => "Android keystore",
        SigningIdentityKind::IosDistribution => "iOS distribution",
    }
}

/// `a`, `a or b`, `a, b or c`.
fn or_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_string(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

fn push_once(list: &mut Vec<String>, line: String) {
    if !list.contains(&line) {
        list.push(line);
    }
}

/// Check every name the plans give against what the project holds. A name
/// the project does not have is an error, as it would refuse the build or
/// fail its signing or publish step.
pub(crate) fn check_credentials(
    config: &RepositoryConfig,
    plans: &[PipelinePlan],
    project: &ProjectCredentials,
    project_id: &str,
) -> CheckOutcome {
    let mut outcome = CheckOutcome::default();
    let mut identities = BTreeSet::new();
    let mut secrets = BTreeSet::new();
    for plan in plans {
        for named in named_identities(config, plan) {
            identities.insert(named.name.clone());
            let expected = match named.platform {
                "android" => SigningIdentityKind::AndroidKeystore,
                _ => SigningIdentityKind::IosDistribution,
            };
            let Some(identity) = project
                .identities
                .iter()
                .find(|identity| identity.name == named.name)
            else {
                push_once(
                    &mut outcome.errors,
                    format!(
                        "{} names signing identity '{}', which project {project_id} does not have; add it under Project settings > Signing",
                        named.field, named.name
                    ),
                );
                continue;
            };
            if identity.kind != expected {
                push_once(
                    &mut outcome.errors,
                    format!(
                        "{} names '{}', which is an {} identity; {} signing needs an {} identity",
                        named.field,
                        named.name,
                        kind_label(identity.kind),
                        named.platform,
                        kind_label(expected)
                    ),
                );
                continue;
            }
            if expected == SigningIdentityKind::IosDistribution
                && plan.publish.needs_app_store_export()
                && identity.export_method == Some(IosExportMethod::AdHoc)
            {
                let scope = plan
                    .workflow
                    .as_deref()
                    .map(|workflow| format!("workflow {workflow}"))
                    .unwrap_or_else(|| "the file".to_string());
                push_once(
                    &mut outcome.errors,
                    format!(
                        "{scope} uploads to App Store Connect, but {} names '{}', which holds ad hoc profiles; use an identity with App Store profiles",
                        named.field, named.name
                    ),
                );
            }
        }
        for named in named_secrets(plan) {
            secrets.insert(named.name.clone());
            if project.secrets.contains(&named.name) {
                continue;
            }
            let mut scopes = vec!["project"];
            if project.pipeline_listed {
                scopes.push("pipeline");
            }
            if project.instance_listed {
                scopes.push("instance");
                push_once(
                    &mut outcome.errors,
                    format!(
                        "{} names secret '{}', which is not a {} secret",
                        named.field,
                        named.name,
                        or_list(&scopes)
                    ),
                );
            } else {
                push_once(
                    &mut outcome.notes,
                    format!(
                        "{} names secret '{}', which is not a {} secret; this session cannot list instance secrets, so it was not checked there",
                        named.field,
                        named.name,
                        or_list(&scopes)
                    ),
                );
            }
        }
    }
    outcome.identities_checked = identities.len();
    outcome.secrets_checked = secrets.len();
    outcome
}

/// Every workflow's plan, for every platform it builds.
pub(crate) fn plan_every_workflow(config: &RepositoryConfig) -> anyhow::Result<Vec<PipelinePlan>> {
    let names = config.workflow_names();
    let workflows: Vec<Option<&str>> = if names.is_empty() {
        vec![None]
    } else {
        names.into_iter().map(Some).collect()
    };
    workflows
        .into_iter()
        .map(|workflow| {
            config
                .plan(PlanRequest {
                    workflow,
                    selected_platforms: None,
                    build_play_bundle: false,
                })
                .map_err(anyhow::Error::msg)
        })
        .collect()
}

async fn load_project_credentials(
    api: &Api,
    project_id: &str,
    pipeline_id: Option<&str>,
) -> anyhow::Result<ProjectCredentials> {
    let base = &api.daemon_url;
    let identities: ListSigningIdentitiesResponse = api
        .send(
            api.client.get(format!(
                "{base}/v1/projects/{project_id}/signing-identities"
            )),
            "signing identity listing",
        )
        .await?;
    let project_secrets: ListSecretsResponse = api
        .send(
            api.client
                .get(format!("{base}/v1/projects/{project_id}/secrets")),
            "project secret listing",
        )
        .await?;
    let mut credentials = ProjectCredentials {
        identities: identities.identities,
        secrets: project_secrets
            .secrets
            .into_iter()
            .map(|secret| secret.name)
            .collect(),
        ..ProjectCredentials::default()
    };
    if let Some(pipeline_id) = pipeline_id {
        let detail: PipelineDetailResponse = api
            .send(
                api.client.get(format!("{base}/v1/pipelines/{pipeline_id}")),
                "pipeline lookup",
            )
            .await?;
        if detail.pipeline.project_id != project_id {
            anyhow::bail!(
                "pipeline {pipeline_id} belongs to project {}, not {project_id}",
                detail.pipeline.project_id
            );
        }
        let pipeline_secrets: ListSecretsResponse = api
            .send(
                api.client
                    .get(format!("{base}/v1/pipelines/{pipeline_id}/secrets")),
                "pipeline secret listing",
            )
            .await?;
        credentials.secrets.extend(
            pipeline_secrets
                .secrets
                .into_iter()
                .map(|secret| secret.name),
        );
        credentials.pipeline_listed = true;
    }
    let instance: Option<ListSecretsResponse> = api
        .send_unless_forbidden(
            api.client.get(format!("{base}/v1/secrets")),
            "instance secret listing",
        )
        .await?;
    if let Some(instance) = instance {
        credentials
            .secrets
            .extend(instance.secrets.into_iter().map(|secret| secret.name));
        credentials.instance_listed = true;
    }
    Ok(credentials)
}

/// `oore pipeline validate --project`: check the file's names through the API.
pub(crate) async fn validate_against_project(
    path: &str,
    config: &RepositoryConfig,
    project_id: &str,
    pipeline_id: Option<&str>,
    connection: &ApiConnectionArgs,
) -> anyhow::Result<()> {
    let project_id = project_id.trim();
    let pipeline_id = pipeline_id.map(str::trim).filter(|id| !id.is_empty());
    if project_id.is_empty() {
        anyhow::bail!("--project must name a project id");
    }
    let plans = plan_every_workflow(config)?;
    let api = Api::connect(connection)?;
    let project = load_project_credentials(&api, project_id, pipeline_id).await?;
    let outcome = check_credentials(config, &plans, &project, project_id);
    for note in &outcome.notes {
        eprintln!("note: {note}");
    }
    for error in &outcome.errors {
        eprintln!("error: {error}");
    }
    if !outcome.errors.is_empty() {
        anyhow::bail!(
            "{path} has {} against project {project_id}",
            match outcome.errors.len() {
                1 => "1 credential error".to_string(),
                count => format!("{count} credential errors"),
            }
        );
    }
    println!(
        "{path} is valid; checked {} against project {project_id}",
        checked_summary(&outcome)
    );
    Ok(())
}

fn checked_summary(outcome: &CheckOutcome) -> String {
    let plural = |count: usize, one: &str, many: &str| match count {
        1 => format!("1 {one}"),
        _ => format!("{count} {many}"),
    };
    format!(
        "{} and {}",
        plural(
            outcome.identities_checked,
            "signing identity",
            "signing identities"
        ),
        plural(outcome.secrets_checked, "secret", "secrets")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use oore_contract::parse_repository_config;

    const FILE: &str = "version: 2
signing:
  android: upload-key
targets:
  android:
    build: flutter build apk --release
  ios:
    build: flutter build ipa --release
workflows:
  adhoc:
    signing: { ios: coin-adhoc }
  release:
    signing: { ios: coin-store }
    targets:
      android:
        build: flutter build appbundle --release
        publish:
          play: { track: internal, service_account: PLAY_RELEASE }
      ios:
        build: flutter build ipa --release
        publish:
          app_store: { destination: testflight, groups: [QA] }
";

    fn config() -> RepositoryConfig {
        parse_repository_config(FILE).unwrap()
    }

    fn plan(config: &RepositoryConfig, workflow: &str) -> PipelinePlan {
        config
            .plan(PlanRequest {
                workflow: Some(workflow),
                selected_platforms: None,
                build_play_bundle: false,
            })
            .unwrap()
    }

    fn identity(
        name: &str,
        kind: SigningIdentityKind,
        export_method: Option<IosExportMethod>,
    ) -> SigningIdentity {
        SigningIdentity {
            id: format!("id-{name}"),
            project_id: "proj-1".to_string(),
            name: name.to_string(),
            kind,
            filename: None,
            key_alias: None,
            team_id: None,
            export_method,
            expires_at: None,
            profiles: Vec::new(),
            used_by: Vec::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn plan_prints_the_identities_store_credentials_and_export_per_workflow() {
        let config = config();
        assert_eq!(
            render_credentials(&config, &plan(&config, "release")),
            "  signing:\n\
             \x20   android: identity upload-key (signing.android)\n\
             \x20   ios: identity coin-store (workflows.release.signing.ios)\n\
             \x20 ios export: App Store, because the workflow uploads to App Store Connect; identity coin-store must hold App Store profiles\n\
             \x20 store credentials:\n\
             \x20   google play: service account secret PLAY_RELEASE (workflows.release.targets.android.publish.play.service_account)\n\
             \x20   app store connect: the API key in the pipeline's iOS signing settings\n"
        );
        assert_eq!(
            render_credentials(&config, &plan(&config, "adhoc")),
            "  signing:\n\
             \x20   android: identity upload-key (signing.android)\n\
             \x20   ios: identity coin-adhoc (workflows.adhoc.signing.ios)\n\
             \x20 ios export: follows the profiles of identity coin-adhoc: App Store or ad hoc\n"
        );

        // Nothing named: the pipeline's settings sign, and only built
        // platforms are listed.
        let bare = parse_repository_config(
            "version: 2\ntargets:\n  android:\n    build: flutter build appbundle\n    publish:\n      play: { track: internal }\n",
        )
        .unwrap();
        let bare_plan = bare
            .plan(PlanRequest {
                workflow: None,
                selected_platforms: None,
                build_play_bundle: false,
            })
            .unwrap();
        assert_eq!(
            render_credentials(&bare, &bare_plan),
            "  signing:\n\
             \x20   android: the pipeline's Android signing settings\n\
             \x20 store credentials:\n\
             \x20   google play: the service account secret in the pipeline's Google Play settings (targets.android.publish.play sets no service_account)\n"
        );
    }

    #[test]
    fn validate_accepts_names_the_project_holds() {
        let config = config();
        let plans = plan_every_workflow(&config).unwrap();
        let project = ProjectCredentials {
            identities: vec![
                identity("upload-key", SigningIdentityKind::AndroidKeystore, None),
                identity(
                    "coin-adhoc",
                    SigningIdentityKind::IosDistribution,
                    Some(IosExportMethod::AdHoc),
                ),
                identity(
                    "coin-store",
                    SigningIdentityKind::IosDistribution,
                    Some(IosExportMethod::AppStore),
                ),
            ],
            secrets: BTreeSet::from(["PLAY_RELEASE".to_string()]),
            pipeline_listed: false,
            instance_listed: true,
        };
        let outcome = check_credentials(&config, &plans, &project, "proj-1");
        assert_eq!(outcome.errors, Vec::<String>::new());
        assert_eq!(outcome.notes, Vec::<String>::new());
        assert_eq!(outcome.identities_checked, 3);
        assert_eq!(outcome.secrets_checked, 1);
        assert_eq!(
            checked_summary(&outcome),
            "3 signing identities and 1 secret"
        );
    }

    #[test]
    fn validate_turns_missing_and_mismatched_names_into_errors() {
        let config = config();
        let plans = plan_every_workflow(&config).unwrap();
        let project = ProjectCredentials {
            identities: vec![
                // An iOS identity where Android needs a keystore.
                identity(
                    "upload-key",
                    SigningIdentityKind::IosDistribution,
                    Some(IosExportMethod::AppStore),
                ),
                // Ad hoc profiles for a workflow that uploads to TestFlight.
                identity(
                    "coin-store",
                    SigningIdentityKind::IosDistribution,
                    Some(IosExportMethod::AdHoc),
                ),
            ],
            secrets: BTreeSet::new(),
            pipeline_listed: true,
            instance_listed: true,
        };
        let outcome = check_credentials(&config, &plans, &project, "proj-1");
        assert_eq!(
            outcome.errors,
            vec![
                // Inherited by both workflows, reported once.
                "signing.android names 'upload-key', which is an iOS distribution identity; android signing needs an Android keystore identity".to_string(),
                "workflows.adhoc.signing.ios names signing identity 'coin-adhoc', which project proj-1 does not have; add it under Project settings > Signing".to_string(),
                "workflow release uploads to App Store Connect, but workflows.release.signing.ios names 'coin-store', which holds ad hoc profiles; use an identity with App Store profiles".to_string(),
                "workflows.release.targets.android.publish.play.service_account names secret 'PLAY_RELEASE', which is not a project, pipeline or instance secret".to_string(),
            ]
        );
    }

    #[test]
    fn validate_notes_secrets_it_cannot_check_in_the_instance_scope() {
        let config = config();
        let plans = plan_every_workflow(&config).unwrap();
        let project = ProjectCredentials {
            identities: vec![
                identity("upload-key", SigningIdentityKind::AndroidKeystore, None),
                identity("coin-adhoc", SigningIdentityKind::IosDistribution, None),
                identity("coin-store", SigningIdentityKind::IosDistribution, None),
            ],
            ..ProjectCredentials::default()
        };
        let outcome = check_credentials(&config, &plans, &project, "proj-1");
        assert!(outcome.errors.is_empty());
        assert_eq!(
            outcome.notes,
            vec!["workflows.release.targets.android.publish.play.service_account names secret 'PLAY_RELEASE', which is not a project secret; this session cannot list instance secrets, so it was not checked there".to_string()]
        );
    }

    #[test]
    fn version_1_files_name_no_credentials() {
        let config = parse_repository_config(
            "version: 1\nplatforms: [android]\ncommands:\n  build: ['flutter build apk']\n",
        )
        .unwrap();
        let plans = plan_every_workflow(&config).unwrap();
        let outcome = check_credentials(&config, &plans, &ProjectCredentials::default(), "p");
        assert_eq!(outcome, CheckOutcome::default());
    }
}
