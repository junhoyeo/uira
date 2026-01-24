use astrape_sdk::{create_astrape_session, QueryParams, SdkBridge, SessionOptions};
use std::path::Path;

fn bridge_available() -> bool {
    let candidates = [
        "./packages/bridge/dist/index.js",
        "../packages/bridge/dist/index.js",
        "../../packages/bridge/dist/index.js",
        "../../../packages/bridge/dist/index.js",
    ];
    candidates.iter().any(|p| Path::new(p).exists())
}

#[test]
fn test_session_to_bridge_options() {
    let session = create_astrape_session(None);
    let bridge_options = session.to_bridge_options();

    assert!(bridge_options.system_prompt.is_some());
    assert!(bridge_options.agents.is_some());
    assert!(bridge_options.mcp_servers.is_some());
    assert!(bridge_options.allowed_tools.is_some());
}

#[test]
fn test_create_query_params() {
    let session = create_astrape_session(None);
    let params = session.create_query_params("Hello, Claude!");

    assert_eq!(params.prompt, "Hello, Claude!");
    assert!(params.options.is_some());

    let options = params.options.unwrap();
    assert!(options.system_prompt.is_some());
}

#[test]
fn test_session_with_options() {
    let options = SessionOptions {
        custom_system_prompt: Some("You are Astrape, a helpful assistant.".to_string()),
        skip_config_load: true,
        ..Default::default()
    };

    let session = create_astrape_session(Some(options));
    assert!(session
        .query_options
        .system_prompt
        .contains("You are Astrape"));
}

#[test]
#[ignore]
fn test_bridge_ping() {
    if !bridge_available() {
        eprintln!("Bridge not built. Run 'npm run build' in packages/bridge/ directory.");
        return;
    }

    let mut bridge = SdkBridge::new().expect("Failed to create bridge");
    let pong = bridge.ping().expect("Ping failed");
    assert!(pong, "Bridge should respond to ping");
}

#[test]
#[ignore]
fn test_full_integration() {
    if !bridge_available() {
        eprintln!("Bridge not built. Run 'npm run build' in packages/bridge/ directory.");
        return;
    }

    let session = create_astrape_session(None);
    let mut bridge = SdkBridge::new().expect("Failed to create bridge");

    assert!(bridge.ping().expect("Ping failed"));

    let params = QueryParams {
        prompt: "Say hello".to_string(),
        options: Some(session.to_bridge_options()),
    };

    eprintln!("Created query params: {:?}", params.prompt);
}
