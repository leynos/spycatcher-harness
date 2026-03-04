Feature: Harness CLI layered configuration

  Scenario: Replay precedence favours CLI over env and file
    Given a replay command with cassette name from_cli
    And config file sets replay cassette name to from_file
    And environment sets replay cassette name to from_env
    When the layered command configuration is loaded
    Then replay cassette name is from_cli

  Scenario: Record command merges cmds.record upstream values
    Given a record command with no CLI overrides
    And config file sets record upstream base URL to https://example.invalid/api
    When the layered command configuration is loaded
    Then record upstream base URL is https://example.invalid/api

  Scenario: Verify command merges cmds.verify cassette value
    Given a verify command with no CLI overrides
    And config file sets verify cassette name to verify_config
    When the layered command configuration is loaded
    Then verify cassette name is verify_config

  Scenario: Invalid environment value fails loading
    Given a replay command with no CLI overrides
    And environment sets replay listen to invalid value not-an-address
    When the layered command configuration is loaded
    Then command configuration loading fails
