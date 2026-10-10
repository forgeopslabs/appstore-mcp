//! Invariants of the served tool surface.
//!
//! These replace an earlier CI check that asserted a hardcoded tool count. A
//! count breaks on every addition while proving almost nothing; what actually
//! matters is that every tool a client sees is well-formed, honestly labelled,
//! and that the configuration knobs really withhold what they claim to.

use appstore_mcp::config::{Config, ToolsConfig};
use appstore_mcp::server::catalog::{classify, Effect, Group};
use appstore_mcp::server::AppStoreServer;
use rmcp::model::Tool;

fn server_with(tools: ToolsConfig) -> AppStoreServer {
    AppStoreServer::new(Config::from_parts(None, None, None).with_tools(tools))
}

fn full_server() -> AppStoreServer {
    server_with(ToolsConfig::default())
}

fn names(tools: &[Tool]) -> Vec<String> {
    tools.iter().map(|t| t.name.to_string()).collect()
}

#[test]
fn every_tool_is_well_formed() {
    for tool in full_server().tools() {
        let name = &tool.name;
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{name}: tool names are snake_case"
        );
        let description = tool
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("{name}: has no description"));
        assert!(
            description.len() > 20,
            "{name}: description is too thin to pick the tool from: {description:?}"
        );
        assert_eq!(
            tool.input_schema.get("type").and_then(|t| t.as_str()),
            Some("object"),
            "{name}: input schema must be an object"
        );
    }
}

#[test]
fn tool_names_are_unique() {
    let tools = full_server().tools();
    let mut unique = names(&tools);
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), tools.len(), "duplicate tool name");
    // A floor, not a fixed count: adding a tool shouldn't fail the build.
    assert!(tools.len() > 100, "only {} tools served", tools.len());
}

#[test]
fn every_tool_carries_the_hints_a_client_uses_to_decide_what_to_confirm() {
    for tool in full_server().tools() {
        let name = &tool.name;
        let annotations = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: has no annotations"));
        assert!(
            annotations.read_only_hint.is_some(),
            "{name}: no readOnlyHint, so a client cannot tell it from a delete"
        );
        assert!(annotations.destructive_hint.is_some(), "{name}");
        assert!(annotations.idempotent_hint.is_some(), "{name}");
        assert_eq!(
            annotations.open_world_hint,
            Some(true),
            "{name}: every tool calls Apple"
        );
    }
}

#[test]
fn no_tool_escapes_classification() {
    // The guard against drift: a tool added with an unrecognised verb would be
    // served with no safety annotations at all, so fail here instead.
    let unclassified: Vec<String> = full_server()
        .tools()
        .iter()
        .filter(|t| classify(&t.name).is_none())
        .map(|t| t.name.to_string())
        .collect();
    assert!(
        unclassified.is_empty(),
        "unclassified tools {unclassified:?} — add their verb to server::catalog::VERBS"
    );
}

#[test]
fn exactly_the_expected_tools_are_marked_destructive() {
    // A snapshot on purpose: a new tool joining this list is a change that
    // deserves a human look, because clients use the hint to gate approval.
    let destructive: Vec<String> = full_server()
        .tools()
        .iter()
        .filter(|t| {
            t.annotations
                .as_ref()
                .and_then(|a| a.destructive_hint)
                .unwrap_or(false)
        })
        .map(|t| t.name.to_string())
        .collect();

    assert_eq!(
        destructive,
        vec![
            "appstore_request", // can reach every DELETE endpoint Apple has
            "delete_asset_library_image",
            "delete_asset_library_placement",
            "delete_asset_library_video",
            "delete_custom_product_page",
            "delete_in_app_purchase",
            "delete_preview_set",
            "delete_review_response",
            "delete_screenshot_set",
            "disable_bundle_id_capability",
            "expire_build",
            "remove_user",
        ]
    );
}

#[test]
fn asc_tools_serves_only_the_groups_asked_for() {
    let filtered = server_with(ToolsConfig {
        read_only: false,
        groups: Some("testflight".into()),
        discovery: false,
    });
    let served = names(&filtered.tools());

    assert!(served.contains(&"list_builds".to_string()));
    assert!(served.contains(&"create_beta_group".to_string()));
    assert!(
        !served.contains(&"list_apps".to_string()),
        "an unrequested group leaked in: {served:?}"
    );
    assert!(
        !served.contains(&"appstore_request".to_string()),
        "the escape hatch is a group like any other"
    );
    assert!(served.len() < full_server().tools().len());
}

#[test]
fn the_core_preset_covers_shipping_an_app() {
    let core = server_with(ToolsConfig {
        read_only: false,
        groups: Some("core".into()),
        discovery: false,
    });
    let served = names(&core.tools());

    for expected in [
        "appstore_request",
        "list_apps",
        "create_app_store_version",
        "upload_app_screenshot",
        "list_builds",
        "submit_review_submission",
    ] {
        assert!(served.contains(&expected.to_string()), "{expected} missing");
    }
    assert!(!served.contains(&"start_ci_build".to_string()));
    assert_eq!(Group::CORE.len(), 6);
}

#[test]
fn an_unrecognised_group_serves_nothing_rather_than_everything() {
    // Failing open would hand full write access to someone who asked for a
    // restricted server and made a typo.
    let typo = server_with(ToolsConfig {
        read_only: false,
        groups: Some("testflightt".into()),
        discovery: false,
    });
    assert!(typo.tools().is_empty());
}

#[test]
fn read_only_mode_withholds_every_tool_that_could_write() {
    let read_only = server_with(ToolsConfig {
        read_only: true,
        groups: None,
        discovery: false,
    });
    let tools = read_only.tools();

    assert!(!tools.is_empty(), "read-only mode served nothing");
    for tool in &tools {
        let effect = classify(&tool.name).expect("classified");
        assert!(
            matches!(effect, Effect::Read | Effect::MethodDependent),
            "{} can write but survived read-only mode",
            tool.name
        );
        assert_eq!(
            tool.annotations.as_ref().unwrap().read_only_hint,
            Some(true),
            "{}: annotated as writing while in read-only mode",
            tool.name
        );
    }

    let served = names(&tools);
    for writer in [
        "remove_user",
        "delete_in_app_purchase",
        "update_app",
        "create_bundle_id",
    ] {
        assert!(!served.contains(&writer.to_string()), "{writer} survived");
    }
    // Reads and the (GET-restricted) escape hatch remain.
    assert!(served.contains(&"list_apps".to_string()));
    assert!(served.contains(&"appstore_request".to_string()));
    assert!(served.contains(&"download_analytics_segment".to_string()));
}

#[test]
fn read_only_mode_composes_with_group_filtering() {
    let both = server_with(ToolsConfig {
        read_only: true,
        groups: Some("users".into()),
        discovery: false,
    });
    let served = names(&both.tools());
    assert_eq!(served, vec!["list_users"]);
}

#[test]
fn discovery_mode_exposes_only_three_conservatively_annotated_tools() {
    let server = server_with(ToolsConfig {
        discovery: true,
        ..ToolsConfig::default()
    });
    let tools = server.tools();
    assert!(tools.iter().all(|tool| classify(&tool.name).is_some()));
    assert_eq!(
        names(&tools),
        vec!["call_discovered_tool", "get_tool_details", "search_tools"]
    );
    let call = &tools[0];
    let hints = call.annotations.as_ref().unwrap();
    assert_eq!(hints.read_only_hint, Some(false));
    assert_eq!(hints.destructive_hint, Some(true));
    assert_eq!(hints.idempotent_hint, Some(false));
    for tool in &tools[1..] {
        assert_eq!(
            tool.annotations.as_ref().unwrap().read_only_hint,
            Some(true)
        );
    }
}

#[test]
fn discovery_mode_marks_execution_read_only_when_server_is_read_only() {
    let server = server_with(ToolsConfig {
        read_only: true,
        discovery: true,
        ..ToolsConfig::default()
    });
    let call = &server.tools()[0];
    let hints = call.annotations.as_ref().unwrap();
    assert_eq!(hints.read_only_hint, Some(true));
    assert_eq!(hints.destructive_hint, Some(false));
}
