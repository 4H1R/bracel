use serde_json::Value;
use std::process::{Command, Output};

fn invoke(args: &[&str], settings: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bracel-starter"))
        .args(args)
        .env_clear()
        .envs(settings.iter().copied())
        .output()
        .unwrap()
}

fn report(output: &Output) -> Value {
    assert!(
        output.stderr.is_empty(),
        "diagnostics must not leak logs to stderr"
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("secret-sentinel"));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    report
}

#[test]
fn cli_reports_invalid_config_and_rejects_unknown_arguments_without_echoing_them() {
    for args in [["doctor", "--json"], ["inspect", "--json"]] {
        for settings in [vec![], vec![("DATABASE_URL", "secret-sentinel")]] {
            let output = invoke(&args, &settings);
            assert_eq!(output.status.code(), Some(1));
            let value = report(&output);
            assert_eq!(value["ok"], false);
            assert_eq!(value["checks"][0]["code"], "CONFIG.INVALID");
            assert!(
                value["checks"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("DATABASE_URL")
            );
            if args[0] == "inspect" {
                assert!(value["application"]["routes"][0]["enabled"].is_null());
            }
        }
    }
    for args in [
        vec!["migrate", "--secret-sentinel"],
        vec!["doctor", "--json", "--json"],
        vec!["inspect", "--deploy"],
        vec!["secret-sentinel"],
    ] {
        let output = invoke(&args, &[]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-sentinel"));
    }
    assert!(invoke(&["--help"], &[]).status.success());
}

#[test]
fn offline_inventory_is_secret_safe_and_distinguishes_enabled_routes() {
    // No server exists on port 1: success proves offline inspection does not connect.
    for enabled in ["false", "true"] {
        let settings = [
            (
                "DATABASE_URL",
                "postgres://secret-sentinel:secret-sentinel@127.0.0.1:1/private",
            ),
            ("ENABLE_EXAMPLE", enabled),
        ];
        let output = invoke(&["inspect", "--json"], &settings);
        assert!(output.status.success());
        let value = report(&output);
        assert_eq!(value["database_status"], "not_checked");
        assert_eq!(value["migrations"][0]["status"], "not_checked");
        let application = &value["application"];
        assert!(
            application["locked_packages"]["axum"][0]
                .as_str()
                .unwrap()
                .starts_with("0.8.")
        );
        assert_eq!(application["capabilities"]["identity"], "bearer_jwt");
        let routes = application["routes"].as_array().unwrap();
        assert!(routes.iter().any(|r| r["path"] == "/healthz"));
        assert_eq!(
            routes
                .iter()
                .filter(|r| r["path"].as_str().unwrap().starts_with("/example/"))
                .count(),
            3
        );
        for route in routes {
            let is_example = route["path"].as_str().unwrap().starts_with("/example/");
            assert_eq!(route["enabled"], !is_example || enabled == "true");
        }
        let output = invoke(&["doctor", "--json", "--deploy"], &settings);
        assert_eq!(output.status.success(), enabled == "false");
        let value = report(&output);
        assert_eq!(value["ok"], enabled == "false");
        assert!(
            value["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["code"] == "DEPLOY.EXTERNAL_REVIEW")
        );
    }
}

#[test]
fn database_failure_is_redacted_and_human_output_is_useful() {
    let settings = [(
        "DATABASE_URL",
        "postgres://secret-sentinel:secret-sentinel@127.0.0.1:1/private",
    )];
    let output = invoke(&["doctor", "--json", "--database"], &settings);
    assert_eq!(output.status.code(), Some(1));
    let value = report(&output);
    assert_eq!(value["database_status"], "unavailable");
    assert!(
        value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["code"] == "DATABASE.UNAVAILABLE")
    );
    let output = invoke(&["doctor"], &settings);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("DATABASE.NOT_CHECKED"));
    assert!(text.contains("--database"));
    assert!(!text.contains("secret-sentinel"));
}
