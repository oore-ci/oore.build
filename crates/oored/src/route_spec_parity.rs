//! Keeps the daemon's routes and the OpenAPI file in agreement, so
//! `oore api`, `@oore/client` and the MCP tools can rely on the file.

use std::collections::BTreeSet;

/// Routes that are not REST operations: browser pages and the MCP endpoint.
const NOT_IN_SPEC: &[&str] = &[
    "/install/latest/{token}",
    "/install/register-device/{token}/profile.mobileconfig",
    "/install/register-device/{token}/callback",
    "/install/register-device/{token}/done",
    "/mcp",
];

const METHODS: &[&str] = &["get", "post", "put", "patch", "delete"];

/// `(method, path)` of every `.route(...)` in the daemon's source.
fn routes_in_source() -> BTreeSet<(String, String)> {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut routes = BTreeSet::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs")
                || path
                    .file_name()
                    .is_some_and(|name| name == "route_spec_parity.rs")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            // Test modules build their own small routers.
            let text = text
                .split("#[cfg(test)]\nmod tests")
                .next()
                .unwrap_or_default();
            let mut rest = text;
            while let Some(start) = rest.find(".route(") {
                rest = &rest[start + ".route(".len()..];
                let call = balanced(rest);
                let Some(path) = call
                    .trim_start()
                    .strip_prefix('"')
                    .and_then(|p| p.split_once('"'))
                    .map(|(p, _)| p.replace("{*", "{"))
                else {
                    continue;
                };
                for method in METHODS {
                    if has_method_call(call, method) {
                        routes.insert((method.to_string(), path.clone()));
                    }
                }
            }
        }
    }
    routes
}

/// The text up to the parenthesis that closes the one already opened.
fn balanced(text: &str) -> &str {
    let mut depth = 1;
    let mut in_string = false;
    let mut previous = '\0';
    for (index, c) in text.char_indices() {
        match c {
            '"' if previous != '\\' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return &text[..index];
                }
            }
            _ => {}
        }
        previous = c;
    }
    text
}

/// Whether `call` contains `get(`, `.get(` or `routing::get(` as a call.
fn has_method_call(call: &str, method: &str) -> bool {
    let needle = format!("{method}(");
    call.match_indices(&needle).any(|(index, _)| {
        call[..index]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

fn routes_in_spec() -> BTreeSet<(String, String)> {
    let spec: serde_json::Value =
        serde_json::from_str(include_str!("../../../apps/docs/public/openapi.json")).unwrap();
    let mut routes = BTreeSet::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for method in METHODS {
            if item.get(*method).is_some() {
                routes.insert((method.to_string(), path.clone()));
            }
        }
    }
    routes
}

#[test]
fn every_route_is_in_the_spec_and_every_spec_path_is_served() {
    let served = routes_in_source();
    let spec = routes_in_spec();
    assert!(served.len() > 200, "found only {} routes", served.len());

    // One router line serves every release action; the spec has a path per
    // action.
    let action_route = (
        "post".to_string(),
        "/v1/releases/{release_id}/{action}".to_string(),
    );
    let is_release_action = |(method, path): &(String, String)| {
        method == "post"
            && path
                .strip_prefix("/v1/releases/{release_id}/")
                .is_some_and(|action| !action.contains('/') && !action.contains('{'))
    };

    let missing_from_spec: Vec<_> = served
        .iter()
        .filter(|route| **route != action_route)
        .filter(|(_, path)| !NOT_IN_SPEC.contains(&path.as_str()))
        .filter(|route| !spec.contains(route))
        .collect();
    let not_served: Vec<_> = spec
        .iter()
        .filter(|route| !served.contains(route))
        .filter(|route| !(served.contains(&action_route) && is_release_action(route)))
        .collect();
    assert!(
        missing_from_spec.is_empty() && not_served.is_empty(),
        "routes missing from apps/docs/public/openapi.json: {missing_from_spec:?}\n\
         spec paths the daemon does not serve: {not_served:?}"
    );
}
