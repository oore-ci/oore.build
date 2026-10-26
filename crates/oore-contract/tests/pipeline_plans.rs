//! Golden plans for repository pipeline files.
//!
//! Each `tests/fixtures/pipeline/<name>.yaml` is planned once per workflow
//! (or once, without workflows) and compared with `<name>[.<workflow>].plan.json`.
//! Run with `UPDATE_GOLDEN=1` to rewrite the expected files after an intended
//! change, then review the diff: for v1 files any change is a regression.

use std::path::{Path, PathBuf};

use oore_contract::{PlanRequest, parse_repository_config};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pipeline")
}

#[test]
fn fixture_files_plan_to_their_golden_plans() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut yaml_files: Vec<PathBuf> = std::fs::read_dir(fixtures())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    yaml_files.sort();
    assert!(yaml_files.len() >= 4, "fixtures are missing");

    let mut checked = 0;
    for path in yaml_files {
        let raw = std::fs::read_to_string(&path).unwrap();
        let config = parse_repository_config(&raw)
            .unwrap_or_else(|error| panic!("{} is invalid:\n{error}", path.display()));
        let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        let workflows: Vec<Option<String>> = match config.workflow_names().as_slice() {
            [] => vec![None],
            names => names.iter().map(|name| Some(name.to_string())).collect(),
        };
        for workflow in workflows {
            let plan = config
                .plan(PlanRequest {
                    workflow: workflow.as_deref(),
                    ..PlanRequest::default()
                })
                .unwrap_or_else(|error| panic!("{stem} {workflow:?}: {error}"));
            let actual = serde_json::to_string_pretty(&plan).unwrap() + "\n";
            let golden = fixtures().join(match &workflow {
                Some(workflow) => format!("{stem}.{workflow}.plan.json"),
                None => format!("{stem}.plan.json"),
            });
            if update {
                std::fs::write(&golden, &actual).unwrap();
            } else {
                let expected = std::fs::read_to_string(&golden)
                    .unwrap_or_else(|_| panic!("{} is missing", golden.display()));
                assert_eq!(actual, expected, "{} changed", golden.display());
            }
            checked += 1;
        }
    }
    assert!(checked >= 7);
}
