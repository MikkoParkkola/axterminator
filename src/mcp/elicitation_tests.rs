use super::*;

// -----------------------------------------------------------------------
// ElicitResponse helpers
// -----------------------------------------------------------------------

#[test]
fn into_accepted_returns_content_on_accept() {
    // GIVEN: accepted response with content
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"answer": 42})),
    };
    // WHEN: into_accepted
    let v = resp.into_accepted().unwrap();
    // THEN: content returned
    assert_eq!(v["answer"], 42);
}

#[test]
fn into_accepted_returns_empty_object_when_no_content() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: None,
    };
    assert_eq!(resp.into_accepted().unwrap(), json!({}));
}

#[test]
fn into_accepted_returns_declined_error() {
    let resp = ElicitResponse {
        action: ElicitAction::Decline,
        content: None,
    };
    assert!(matches!(
        resp.into_accepted(),
        Err(ElicitError::Declined(_))
    ));
}

#[test]
fn into_accepted_returns_cancelled_error() {
    let resp = ElicitResponse {
        action: ElicitAction::Cancel,
        content: None,
    };
    assert_eq!(resp.into_accepted(), Err(ElicitError::Cancelled));
}

// -----------------------------------------------------------------------
// Scenario 1 — ambiguous app
// -----------------------------------------------------------------------

#[test]
fn elicit_ambiguous_app_message_contains_query() {
    // GIVEN: two matching apps
    let req = elicit_ambiguous_app(
        "Chrome",
        &[
            ("Google Chrome".into(), "com.google.Chrome".into()),
            ("Chrome Canary".into(), "com.google.Chrome.canary".into()),
        ],
    );
    // THEN: message includes the query
    assert!(req.params.message.contains("Chrome"));
}

#[test]
fn elicit_ambiguous_app_schema_has_correct_choices() {
    // GIVEN: two apps
    let req = elicit_ambiguous_app(
        "Mail",
        &[
            ("Mail (Apple)".into(), "com.apple.mail".into()),
            ("Mailspring".into(), "com.mailspring.Mailspring".into()),
        ],
    );
    // THEN: two oneOf entries
    let choices = req.params.requested_schema["properties"]["app"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[0]["const"], "com.apple.mail");
    assert_eq!(choices[1]["title"], "Mailspring");
}

#[test]
fn parse_ambiguous_app_extracts_bundle_id_on_accept() {
    // GIVEN: accepted response with app selection
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"app": "com.google.Chrome"})),
    };
    // THEN: bundle ID returned
    assert_eq!(parse_ambiguous_app(resp).unwrap(), "com.google.Chrome");
}

#[test]
fn parse_ambiguous_app_returns_missing_field_when_no_app_key() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({})),
    };
    assert_eq!(
        parse_ambiguous_app(resp),
        Err(ElicitError::MissingField("app".into()))
    );
}

#[test]
fn parse_ambiguous_app_returns_cancelled_on_cancel() {
    let resp = ElicitResponse {
        action: ElicitAction::Cancel,
        content: None,
    };
    assert_eq!(parse_ambiguous_app(resp), Err(ElicitError::Cancelled));
}

// -----------------------------------------------------------------------
// Scenario 2 — element not found
// -----------------------------------------------------------------------

#[test]
fn elicit_element_not_found_message_mentions_query_and_app() {
    let req = elicit_element_not_found("Submit", "Safari", &["Submit Form", "Cancel"]);
    assert!(req.params.message.contains("Submit"));
    assert!(req.params.message.contains("Safari"));
}

#[test]
fn elicit_element_not_found_schema_caps_candidates_at_three() {
    // GIVEN: five candidates
    let candidates = vec!["A", "B", "C", "D", "E"];
    let req = elicit_element_not_found("query", "App", &candidates);
    // THEN: schema has 3 + 1 ("custom") = 4 entries
    let choices = req.params.requested_schema["properties"]["choice"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(choices.len(), 4); // 3 candidates + custom
}

#[test]
fn elicit_element_not_found_no_candidates_uses_description_schema() {
    let req = elicit_element_not_found("query", "App", &[] as &[&str]);
    assert!(req.params.requested_schema["properties"]["description"].is_object());
}

#[test]
fn parse_element_not_found_returns_candidate() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"choice": "Submit Form", "use_visual": false})),
    };
    assert_eq!(
        parse_element_not_found(resp).unwrap(),
        ElementChoice::Candidate("Submit Form".into())
    );
}

#[test]
fn parse_element_not_found_returns_custom_when_custom_selected() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"choice": "__custom__", "custom_query": "My Button"})),
    };
    assert_eq!(
        parse_element_not_found(resp).unwrap(),
        ElementChoice::Custom("My Button".into())
    );
}

#[test]
fn parse_element_not_found_returns_visual_when_use_visual_true() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"choice": "btn", "use_visual": true})),
    };
    assert_eq!(
        parse_element_not_found(resp).unwrap(),
        ElementChoice::UseVisual
    );
}

#[test]
fn parse_element_not_found_no_candidates_custom_description() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"description": "the red save button", "use_visual": false})),
    };
    assert_eq!(
        parse_element_not_found(resp).unwrap(),
        ElementChoice::Custom("the red save button".into())
    );
}

#[test]
fn parse_element_not_found_no_candidates_use_visual() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"use_visual": true})),
    };
    assert_eq!(
        parse_element_not_found(resp).unwrap(),
        ElementChoice::UseVisual
    );
}

// -----------------------------------------------------------------------
// Scenario 3 — destructive action
// -----------------------------------------------------------------------

#[test]
fn is_destructive_element_detects_delete() {
    assert!(is_destructive_element("Delete All Data"));
}

#[test]
fn is_destructive_element_detects_format() {
    assert!(is_destructive_element("Format Drive"));
}

#[test]
fn is_destructive_element_is_case_insensitive() {
    assert!(is_destructive_element("ERASE EVERYTHING"));
    assert!(is_destructive_element("Quit Application"));
}

#[test]
fn is_destructive_element_false_for_safe_text() {
    assert!(!is_destructive_element("Save Document"));
    assert!(!is_destructive_element("Submit Form"));
    assert!(!is_destructive_element("Next"));
}

#[test]
fn elicit_destructive_action_message_contains_element_and_app() {
    let req = elicit_destructive_action("Delete All", "Finder");
    assert!(req.params.message.contains("Delete All"));
    assert!(req.params.message.contains("Finder"));
}

#[test]
fn elicit_destructive_action_schema_requires_confirm() {
    let req = elicit_destructive_action("Delete", "App");
    assert_eq!(req.params.requested_schema["required"], json!(["confirm"]));
}

#[test]
fn parse_destructive_action_returns_ok_when_confirmed() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"confirm": true})),
    };
    assert!(parse_destructive_action(resp).is_ok());
}

#[test]
fn parse_destructive_action_returns_declined_when_false() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"confirm": false})),
    };
    assert!(matches!(
        parse_destructive_action(resp),
        Err(ElicitError::Declined(_))
    ));
}

#[test]
fn parse_destructive_action_returns_missing_field_when_no_confirm() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({})),
    };
    assert_eq!(
        parse_destructive_action(resp),
        Err(ElicitError::MissingField("confirm".into()))
    );
}

#[test]
fn parse_destructive_action_returns_cancelled_on_cancel() {
    let resp = ElicitResponse {
        action: ElicitAction::Cancel,
        content: None,
    };
    assert_eq!(parse_destructive_action(resp), Err(ElicitError::Cancelled));
}

// -----------------------------------------------------------------------
// Scenario 4 — permissions missing
// -----------------------------------------------------------------------

#[test]
fn elicit_permissions_missing_message_mentions_accessibility() {
    let req = elicit_permissions_missing();
    // Message begins with "Accessibility permissions are not enabled…"
    assert!(
        req.params
            .message
            .to_ascii_lowercase()
            .contains("accessibility"),
        "message should mention accessibility: {}",
        req.params.message
    );
}

#[test]
fn elicit_permissions_missing_schema_has_three_actions() {
    let req = elicit_permissions_missing();
    let choices = req.params.requested_schema["properties"]["action"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(choices.len(), 3);
}

#[test]
fn parse_permissions_missing_open_settings() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"action": "open_settings"})),
    };
    assert_eq!(
        parse_permissions_missing(resp).unwrap(),
        PermissionAction::OpenSettings
    );
}

#[test]
fn parse_permissions_missing_show_instructions() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"action": "show_instructions"})),
    };
    assert_eq!(
        parse_permissions_missing(resp).unwrap(),
        PermissionAction::ShowInstructions
    );
}

#[test]
fn parse_permissions_missing_cancel_action() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"action": "cancel"})),
    };
    assert_eq!(parse_permissions_missing(resp), Err(ElicitError::Cancelled));
}

#[test]
fn parse_permissions_missing_dialog_cancel() {
    let resp = ElicitResponse {
        action: ElicitAction::Cancel,
        content: None,
    };
    assert_eq!(parse_permissions_missing(resp), Err(ElicitError::Cancelled));
}

#[test]
fn parse_permissions_missing_unknown_action_returns_missing_field() {
    let resp = ElicitResponse {
        action: ElicitAction::Accept,
        content: Some(json!({"action": "fly_to_moon"})),
    };
    assert!(matches!(
        parse_permissions_missing(resp),
        Err(ElicitError::MissingField(_))
    ));
}

// -----------------------------------------------------------------------
// ElicitRequest serialization round-trip
// -----------------------------------------------------------------------

#[test]
fn elicit_request_round_trips_via_json() {
    // GIVEN: a request
    let req = elicit_destructive_action("Delete", "App");
    // WHEN: serialized and deserialized
    let json = serde_json::to_string(&req).unwrap();
    let back: ElicitRequest = serde_json::from_str(&json).unwrap();
    // THEN: structurally identical
    assert_eq!(req, back);
}

#[test]
fn elicit_response_deserializes_from_wire_format() {
    // GIVEN: wire JSON as a client would send
    let wire = r#"{"action":"accept","content":{"confirm":true}}"#;
    let resp: ElicitResponse = serde_json::from_str(wire).unwrap();
    assert_eq!(resp.action, ElicitAction::Accept);
    assert_eq!(resp.content.unwrap()["confirm"], true);
}

// -----------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------

#[test]
fn accessibility_settings_url_is_valid_apple_url() {
    assert!(ACCESSIBILITY_SETTINGS_URL.starts_with("x-apple.systempreferences:"));
}
