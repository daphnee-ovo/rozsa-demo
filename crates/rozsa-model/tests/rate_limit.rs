use rozsa_model::rate_limit::parse_rate_limit_response_json;

#[test]
fn parses_wham_usage_primary_and_secondary_windows() {
    let input = r#"{
        "plan_type": "plus",
        "rate_limit": {
            "allowed": true,
            "limit_reached": false,
            "primary_window": {
                "used_percent": 42,
                "limit_window_seconds": 18000,
                "reset_after_seconds": 900,
                "reset_at": 1780000000
            },
            "secondary_window": {
                "used_percent": 67,
                "limit_window_seconds": 604800,
                "reset_after_seconds": 172800,
                "reset_at": 1780171900
            }
        }
    }"#;

    let snapshot = parse_rate_limit_response_json(input).expect("valid usage payload");

    assert_eq!(snapshot.plan_type.as_deref(), Some("plus"));
    assert!(snapshot.allowed);
    assert!(!snapshot.limit_reached);
    let primary = snapshot.primary.expect("primary window");
    assert_eq!(primary.used_percent, 42.0);
    assert_eq!(primary.window_duration_secs, 18_000);
    assert_eq!(primary.reset_after_secs, 900);
    assert_eq!(primary.reset_at, 1_780_000_000);
    let secondary = snapshot.secondary.expect("secondary window");
    assert_eq!(secondary.used_percent, 67.0);
    assert_eq!(secondary.window_duration_secs, 604_800);
    assert_eq!(secondary.reset_after_secs, 172_800);
}

#[test]
fn parses_camel_case_usage_payload() {
    let input = r#"{
        "planType": "pro",
        "rateLimit": {
            "allowed": false,
            "limitReached": true,
            "primaryWindow": {
                "usedPercent": 100,
                "limitWindowSeconds": 18000,
                "resetAfterSeconds": 60,
                "resetAt": 1780000000
            }
        }
    }"#;

    let snapshot = parse_rate_limit_response_json(input).expect("valid usage payload");

    assert_eq!(snapshot.plan_type.as_deref(), Some("pro"));
    assert!(!snapshot.allowed);
    assert!(snapshot.limit_reached);
    assert_eq!(
        snapshot.primary.expect("primary window").used_percent,
        100.0
    );
    assert!(snapshot.secondary.is_none());
}

#[test]
fn classifies_a_weekly_primary_window_as_weekly_not_hourly() {
    let input = r#"{
        "rate_limit": {
            "primary_window": {
                "used_percent": 50,
                "limit_window_seconds": 604800,
                "reset_after_seconds": 3600,
                "reset_at": 1780000000
            }
        }
    }"#;

    let snapshot = parse_rate_limit_response_json(input).expect("valid usage payload");

    assert!(snapshot.primary.is_none());
    assert_eq!(
        snapshot.secondary.expect("weekly window").used_percent,
        50.0
    );
}
