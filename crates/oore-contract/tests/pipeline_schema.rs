//! The published JSON Schema for `.oore.yaml` version 2
//! (`apps/docs/public/schema/oore-yaml-v2.json`) must agree with the parser.
//!
//! The schema is written by hand, because most v2 types have their own
//! `Deserialize`. These tests keep it honest:
//! - every key set and enum in the schema equals what the parser accepts,
//!   read from serde's "expected one of" errors, so a field added to
//!   `pipeline_config.rs` fails here until the schema has it;
//! - every valid v2 fixture and every v2 example on the reference page passes
//!   the schema, and files the parser rejects for a key or a value the schema
//!   describes fail it.

use std::path::{Path, PathBuf};

use oore_contract::parse_repository_config;
use serde_json::{Map, Value, json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schema() -> Value {
    let path = repo_root().join("apps/docs/public/schema/oore-yaml-v2.json");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

// ── A small JSON Schema validator for the keywords the schema uses ──────

fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    match schema.get("$ref").and_then(Value::as_str) {
        Some(reference) => {
            let pointer = reference.strip_prefix('#').expect("local $ref only");
            resolve(root, root.pointer(pointer).expect("$ref resolves"))
        }
        None => schema,
    }
}

fn type_matches(name: &str, value: &Value) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        other => panic!("schema uses unsupported type {other}"),
    }
}

fn validate(root: &Value, schema: &Value, value: &Value, at: &str, errors: &mut Vec<String>) {
    let schema = resolve(root, schema);
    if let Some(kind) = schema.get("type").and_then(Value::as_str)
        && !type_matches(kind, value)
    {
        errors.push(format!("{at}: expected {kind}"));
        return;
    }
    if let Some(expected) = schema.get("const")
        && expected != value
    {
        errors.push(format!("{at}: expected {expected}"));
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        errors.push(format!("{at}: {value} is not one of {options:?}"));
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        let matching = variants
            .iter()
            .filter(|variant| {
                let mut nested = Vec::new();
                validate(root, variant, value, at, &mut nested);
                nested.is_empty()
            })
            .count();
        if matching != 1 {
            errors.push(format!("{at}: matches {matching} of oneOf"));
        }
    }
    if let Some(limit) = schema.get("maxLength").and_then(Value::as_u64)
        && let Some(text) = value.as_str()
        && text.chars().count() as u64 > limit
    {
        errors.push(format!("{at}: longer than {limit}"));
    }
    if let Some(number) = value.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64)
            && number < min
        {
            errors.push(format!("{at}: below {min}"));
        }
        if let Some(min) = schema.get("exclusiveMinimum").and_then(Value::as_f64)
            && number <= min
        {
            errors.push(format!("{at}: not above {min}"));
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64)
            && number > max
        {
            errors.push(format!("{at}: above {max}"));
        }
    }
    if let Some(items) = value.as_array() {
        if let Some(limit) = schema.get("maxItems").and_then(Value::as_u64)
            && items.len() as u64 > limit
        {
            errors.push(format!("{at}: more than {limit} items"));
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate(root, item_schema, item, &format!("{at}[{index}]"), errors);
            }
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        for required in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = required.as_str().unwrap();
            if !object.contains_key(name) {
                errors.push(format!("{at}: missing {name}"));
            }
        }
        for (key, item) in object {
            let path = format!("{at}.{key}");
            if let Some(names) = schema.get("propertyNames") {
                validate(root, names, &Value::String(key.clone()), &path, errors);
            }
            match properties.and_then(|properties| properties.get(key)) {
                Some(property) => validate(root, property, item, &path, errors),
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => errors.push(format!("{path}: unknown key")),
                    Some(Value::Bool(true)) | None => {}
                    Some(additional) => validate(root, additional, item, &path, errors),
                },
            }
        }
    }
}

fn schema_errors(document: &Value) -> Vec<String> {
    let root = schema();
    let mut errors = Vec::new();
    validate(&root, &root, document, "$", &mut errors);
    errors
}

fn yaml_to_json(raw: &str) -> Value {
    serde_yaml::from_str(raw).expect("YAML parses")
}

// ── Reading what the parser accepts ─────────────────────────────────────

/// A v2 file that reaches every object the schema describes.
fn full_document() -> Value {
    json!({
        "version": 2,
        "name": "Everything",
        "app": { "flutter": "3.24.5", "flavors": ["free"] },
        "targets": {
            "android": {
                "build": "flutter build appbundle --release --flavor ${{ flavor }}",
                "artifacts": ["build/**/*.aab"],
                "publish": { "play": { "track": "internal", "rollout": 0.5, "preflight": "warn" } },
                "budgets": { "mode": "warn", "rules": [{ "kind": "aab", "max_bytes": 1000 }] }
            },
            "ios": {
                "build": "flutter build ipa --release --flavor ${{ flavor }}",
                "publish": { "app_store": { "destination": "testflight", "groups": ["QA"] } }
            }
        },
        "steps": [
            { "name": "Tests", "run": "flutter test", "reports": { "tests": "tests.json" } },
            "build"
        ],
        "env": { "A": "1" },
        "vars": { "B": "2" },
        "on": { "push": { "branches": ["main"] }, "tag": "v*" },
        "signing": { "android": "upload" },
        "runs_on": "macos",
        "isolation": "host",
        "build_number_offset": 10,
        "release_notes": { "changelog": "CHANGELOG.md" },
        "workflows": {
            "release": {
                "on": { "pull_request": true },
                "targets": { "android": { "build": "flutter build apk" } },
                "flavors": ["free"],
                "steps": ["Tests", "build"],
                "env": { "C": "3" },
                "signing": { "ios": "dist" }
            }
        }
    })
}

fn parse_json(document: &Value) -> Result<(), String> {
    let raw = serde_yaml::to_string(document).unwrap();
    parse_repository_config(&raw).map(|_| ())
}

/// The backticked names after "expected" in a serde error.
fn expected_names(error: &str) -> Vec<String> {
    let tail = error
        .split_once("expected")
        .unwrap_or_else(|| panic!("no `expected` in: {error}"))
        .1;
    let mut names: Vec<String> = tail
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    names.sort();
    names
}

fn with_value(pointer: &str, value: Value) -> Value {
    let mut document = full_document();
    *document
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("{pointer} is not in the full document")) = value;
    document
}

fn with_key(pointer: &str, key: &str) -> Value {
    let mut document = full_document();
    let object: &mut Map<String, Value> = document
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("{pointer} is not an object in the full document"));
    object.insert(key.to_string(), json!("x"));
    document
}

fn schema_keys(schema_pointer: &str) -> Vec<String> {
    let root = schema();
    let node = resolve(&root, root.pointer(schema_pointer).unwrap());
    let mut keys: Vec<String> = node["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{schema_pointer} has no properties"))
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn schema_enum(schema_pointer: &str) -> Vec<String> {
    let root = schema();
    let node = resolve(&root, root.pointer(schema_pointer).unwrap());
    let mut values: Vec<String> = node["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("{schema_pointer} has no enum"))
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    values.sort();
    values
}

#[test]
fn full_document_parses_and_matches_the_schema() {
    let document = full_document();
    parse_json(&document).unwrap_or_else(|error| panic!("full document is invalid: {error}"));
    assert_eq!(schema_errors(&document), Vec::<String>::new());
}

#[test]
fn schema_keys_match_the_parser() {
    // (where to add an unknown key in the document, the schema node for it)
    let objects = [
        ("", ""),
        ("/app", "/properties/app"),
        ("/targets/android", "/$defs/target"),
        ("/targets/android/publish", "/$defs/publish"),
        (
            "/targets/android/publish/play",
            "/$defs/publish/properties/play",
        ),
        (
            "/targets/ios/publish/app_store",
            "/$defs/publish/properties/app_store",
        ),
        ("/targets/android/budgets", "/$defs/sizeBudget"),
        (
            "/targets/android/budgets/rules/0",
            "/$defs/sizeBudget/properties/rules/items",
        ),
        ("/steps/0", "/$defs/steps/items/oneOf/1"),
        (
            "/steps/0/reports",
            "/$defs/steps/items/oneOf/1/properties/reports",
        ),
        ("/on", "/$defs/trigger"),
        ("/on/push", "/$defs/branchTrigger/oneOf/1"),
        ("/signing", "/$defs/signing"),
        ("/release_notes", "/properties/release_notes"),
        ("/workflows/release", "/$defs/workflow"),
    ];
    let mut failures = Vec::new();
    for (pointer, schema_pointer) in objects {
        let error = parse_json(&with_key(pointer, "zz_unknown"))
            .expect_err(&format!("unknown key at {pointer:?} was accepted"));
        assert!(
            error.contains("unknown field `zz_unknown`"),
            "{pointer:?}: unexpected error {error}"
        );
        let parser = expected_names(&error);
        let schema = schema_keys(schema_pointer);
        if parser != schema {
            failures.push(format!(
                "{pointer:?}: parser accepts {parser:?}, schema {schema_pointer} lists {schema:?}"
            ));
        }
    }

    // testflight is the other publish shape.
    let mut document = full_document();
    document["targets"]["ios"]["publish"] =
        json!({ "testflight": { "group": "QA", "zz_unknown": 1 } });
    let error = parse_json(&document).expect_err("unknown testflight key was accepted");
    let parser = expected_names(&error);
    let schema = schema_keys("/$defs/publish/properties/testflight");
    if parser != schema {
        failures.push(format!("testflight: parser {parser:?}, schema {schema:?}"));
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn schema_enums_match_the_parser() {
    let enums = [
        ("/isolation", "/properties/isolation"),
        (
            "/targets/android/publish/play/preflight",
            "/$defs/preflight",
        ),
        (
            "/targets/ios/publish/app_store/destination",
            "/$defs/publish/properties/app_store/properties/destination",
        ),
        (
            "/targets/android/budgets/mode",
            "/$defs/sizeBudget/properties/mode",
        ),
    ];
    let mut failures = Vec::new();
    for (pointer, schema_pointer) in enums {
        let error = parse_json(&with_value(pointer, json!("zz_bad")))
            .expect_err(&format!("bad value at {pointer} was accepted"));
        let parser = expected_names(&error);
        let schema = schema_enum(schema_pointer);
        if parser != schema {
            failures.push(format!("{pointer}: parser {parser:?}, schema {schema:?}"));
        }
    }

    let mut document = full_document();
    document["targets"]["ios"]["publish"]["app_store"] =
        json!({ "destination": "app_store", "release_type": "zz_bad" });
    let error = parse_json(&document).expect_err("bad release_type was accepted");
    let parser = expected_names(&error);
    let schema = schema_enum("/$defs/publish/properties/app_store/properties/release_type");
    if parser != schema {
        failures.push(format!(
            "release_type: parser {parser:?}, schema {schema:?}"
        ));
    }

    let mut document = full_document();
    let android = document["targets"]["android"].take();
    document["targets"] = json!({ "zz_bad": android });
    let error = parse_json(&document).expect_err("bad platform was accepted");
    let parser = expected_names(&error);
    let schema = schema_enum("/$defs/platform");
    if parser != schema {
        failures.push(format!("platform: parser {parser:?}, schema {schema:?}"));
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn collect_mdx(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_mdx(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "mdx") {
            out.push(path);
        }
    }
}

/// Every `version: 2` YAML block in the docs and every v2 fixture.
fn v2_examples() -> Vec<(String, String)> {
    let mut examples = Vec::new();
    let fixtures = repo_root().join("crates/oore-contract/tests/fixtures/pipeline");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&fixtures)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    paths.sort();
    for path in paths {
        let raw = std::fs::read_to_string(&path).unwrap();
        if raw.lines().any(|line| line.trim() == "version: 2") {
            examples.push((path.display().to_string(), raw));
        }
    }

    let mut pages = Vec::new();
    collect_mdx(&repo_root().join("apps/docs/content/docs"), &mut pages);
    pages.sort();
    for page in pages {
        let text = std::fs::read_to_string(&page).unwrap();
        let mut lines = text.lines();
        let mut index = 0;
        while let Some(line) = lines.next() {
            if line.trim_start().starts_with("```yaml") {
                let block: Vec<&str> = lines
                    .by_ref()
                    .take_while(|line| !line.trim_start().starts_with("```"))
                    .collect();
                index += 1;
                let raw = block.join("\n");
                if raw.lines().any(|line| line.trim() == "version: 2") {
                    examples.push((format!("{} block {index}", page.display()), raw));
                }
            }
        }
    }
    examples
}

#[test]
fn valid_examples_pass_the_schema() {
    let examples = v2_examples();
    assert!(examples.len() >= 4, "too few v2 examples found");
    let mut failures = Vec::new();
    let mut checked = 0;
    for (name, raw) in examples {
        // Snippets the parser rejects on purpose (error examples) are skipped.
        if parse_repository_config(&raw).is_err() {
            continue;
        }
        checked += 1;
        let errors = schema_errors(&yaml_to_json(&raw));
        if !errors.is_empty() {
            failures.push(format!("{name}:\n  {}", errors.join("\n  ")));
        }
    }
    assert!(checked >= 4, "too few valid v2 examples checked");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn invalid_files_fail_the_schema_and_the_parser() {
    let cases = [
        ("unknown top-level key", with_key("", "zz_unknown")),
        ("wrong version", with_value("/version", json!(3))),
        ("bad isolation", with_value("/isolation", json!("docker"))),
        (
            "rollout above 1",
            with_value("/targets/android/publish/play/rollout", json!(1.5)),
        ),
        (
            "play without track",
            with_value("/targets/android/publish/play", json!({ "draft": true })),
        ),
        (
            "step without run",
            with_value("/steps/0", json!({ "name": "x" })),
        ),
        (
            "publish true",
            with_value("/targets/android/publish", json!(true)),
        ),
        (
            "release notes with both",
            with_value(
                "/release_notes",
                json!({ "changelog": "CHANGELOG.md", "dir": "notes" }),
            ),
        ),
    ];
    for (name, document) in cases {
        assert!(
            !schema_errors(&document).is_empty(),
            "{name}: the schema accepted it"
        );
        assert!(
            parse_json(&document).is_err(),
            "{name}: the parser accepted it"
        );
    }
}
