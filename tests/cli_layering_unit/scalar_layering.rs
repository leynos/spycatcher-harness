//! No-CLI witnesses for scalar environment-over-file precedence.

use rstest::rstest;

use super::{ModeSnapshot, load_with_child};

#[rstest]
#[case("record", ModeSnapshot::Record)]
#[case("verify", ModeSnapshot::Verify)]
fn environment_overrides_file_scalars_without_cli_flags(
    #[case] subcommand: &str,
    #[case] expected_mode: ModeSnapshot,
) {
    let config = format!(
        concat!(
            "[cmds.{}]\n",
            "listen = \"127.0.0.1:8787\"\n",
            "cassette_dir = \"from_file\"\n",
            "cassette_name = \"from_file\"\n",
        ),
        subcommand,
    );
    let env_prefix = format!("SPYCATCHER_HARNESS_CMDS_{}", subcommand.to_uppercase());
    let listen_env = format!("{env_prefix}_LISTEN");
    let cassette_dir_env = format!("{env_prefix}_CASSETTE_DIR");
    let cassette_name_env = format!("{env_prefix}_CASSETTE_NAME");
    let loaded = load_with_child(
        &["spycatcher-harness", subcommand],
        Some(&config),
        &[
            (listen_env.as_str(), "127.0.0.1:9876"),
            (cassette_dir_env.as_str(), "from_env"),
            (cassette_name_env.as_str(), "from_env"),
        ],
    )
    .expect("environment and file values should merge");

    assert_eq!(loaded.mode, expected_mode);
    assert_eq!(loaded.listen, "127.0.0.1:9876");
    assert_eq!(loaded.cassette_dir, "from_env");
    assert_eq!(loaded.cassette_name, "from_env");
}
