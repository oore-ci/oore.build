//! MCP resources and prompts: the `.oore.yaml` reference for agents that
//! write pipeline files, and a "diagnose this build" prompt.

use rmcp::model::{
    GetPromptResult, Prompt, PromptArgument, PromptMessage, ReadResourceResult, Resource,
    ResourceContents, Role,
};

pub const OORE_YAML_URI: &str = "oore://docs/oore-yaml-v2.md";

/// The docs page, read when the daemon is built, so it always matches this
/// version. Its front matter is dropped.
const OORE_YAML_PAGE: &str =
    include_str!("../../../../apps/docs/content/docs/reference/config/oore-yaml-v2.mdx");

fn oore_yaml_markdown() -> String {
    let body = OORE_YAML_PAGE
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---"))
        .map_or(OORE_YAML_PAGE, |(_, body)| body)
        .trim_start();
    format!(
        "# .oore.yaml version 2 reference\n\nFrom https://docs.oore.build/reference/config/oore-yaml-v2 \
         for this Oore version. Check a file with the validate_pipeline_file tool.\n\n{body}"
    )
}

pub fn resources() -> Vec<Resource> {
    vec![
        Resource::new(OORE_YAML_URI, "oore-yaml-v2")
            .with_title(".oore.yaml reference")
            .with_description(
                "How to write an Oore pipeline file (.oore.yaml version 2): app, targets, steps, \
                 workflows, vars and secrets.",
            )
            .with_mime_type("text/markdown")
            .with_size(oore_yaml_markdown().len() as u64),
    ]
}

pub fn read(uri: &str) -> Option<ReadResourceResult> {
    (uri == OORE_YAML_URI).then(|| {
        let mut contents = ResourceContents::text(oore_yaml_markdown(), uri);
        if let ResourceContents::TextResourceContents { mime_type, .. } = &mut contents {
            *mime_type = Some("text/markdown".into());
        }
        ReadResourceResult::new(vec![contents])
    })
}

pub const DIAGNOSE: &str = "diagnose_build";

pub fn prompts() -> Vec<Prompt> {
    let mut prompt = Prompt::new(
        DIAGNOSE,
        Some("Find out why an Oore build failed and propose a fix."),
        Some(vec![
            PromptArgument::new("build")
                .with_title("Build")
                .with_description("Build ID, or build number when app is given.")
                .with_required(true),
            PromptArgument::new("app")
                .with_title("App")
                .with_description("App ID or name, needed with a build number."),
        ]),
    );
    prompt.title = Some("Diagnose this build".into());
    vec![prompt]
}

pub fn get(name: &str, arguments: Option<&rmcp::model::JsonObject>) -> Option<GetPromptResult> {
    if name != DIAGNOSE {
        return None;
    }
    let arg = |key: &str| {
        arguments
            .and_then(|args| args.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let build = arg("build").unwrap_or_else(|| "the most recent failed build".into());
    let find = match arg("app") {
        Some(app) => format!(
            "First call list_builds with app {app:?} and find build {build}; use its id below."
        ),
        None if build.chars().all(|c| c.is_ascii_digit()) => format!(
            "Build {build} is a number. Ask which app it belongs to, or call list_builds to find it, then use its id."
        ),
        None => format!("The build ID is {build:?}."),
    };
    let text = format!(
        "Find out why this Oore build failed and how to fix it.\n\n{find}\n\n\
         1. Call diagnose_build with the build ID. It gives the failed step, its exit code, the likely \
         cause, the last lines of that step and any failed tests.\n\
         2. If that is not enough, call get_build_logs with the failed step's name, and search for \
         words from the error.\n\
         3. If the cause is in .oore.yaml, read the oore-yaml-v2 resource and check the fix with \
         validate_pipeline_file.\n\n\
         Answer with: what failed, the evidence (quote the log lines), the most likely cause, and the \
         fix. Say plainly when you are guessing. Do not start a new build unless asked."
    );
    Some(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
            .with_description("Diagnose a failed Oore build"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_has_no_front_matter() {
        let markdown = oore_yaml_markdown();
        assert!(markdown.starts_with("# .oore.yaml version 2 reference"));
        assert!(!markdown.contains("\ntitle: "));
        assert!(markdown.contains("version: 2"));
        assert!(read(OORE_YAML_URI).is_some());
        assert!(read("oore://other").is_none());
    }

    #[test]
    fn the_prompt_names_the_build() {
        let mut args = rmcp::model::JsonObject::new();
        args.insert("build".into(), "b123".into());
        let result = get(DIAGNOSE, Some(&args)).unwrap();
        let text = serde_json::to_string(&result).unwrap();
        assert!(text.contains("b123"));
        assert!(text.contains("diagnose_build"));
        assert!(get("other", None).is_none());
    }
}
