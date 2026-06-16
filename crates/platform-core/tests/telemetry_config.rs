use platform_core::{LogFormat, TelemetryConfig};

#[test]
fn log_format_defaults_to_terminal_compact_output() {
    assert_eq!(LogFormat::default(), LogFormat::Compact);
}

#[test]
fn log_format_accepts_json_for_machine_readable_output() {
    assert_eq!(LogFormat::from_env_value("json"), Some(LogFormat::Json));
    assert_eq!(LogFormat::from_env_value("JSON"), Some(LogFormat::Json));
}

#[test]
fn log_format_accepts_compact_for_terminal_output() {
    assert_eq!(
        LogFormat::from_env_value("compact"),
        Some(LogFormat::Compact)
    );
    assert_eq!(
        LogFormat::from_env_value("terminal"),
        Some(LogFormat::Compact)
    );
}

#[test]
fn telemetry_config_can_carry_log_format() {
    let config = TelemetryConfig {
        log_level: "debug".to_owned(),
        log_format: LogFormat::Json,
        otlp_endpoint: None,
    };

    assert_eq!(config.log_format, LogFormat::Json);
}
