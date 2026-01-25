use astrape_sdk::{create_astrape_session, SessionOptions};

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
