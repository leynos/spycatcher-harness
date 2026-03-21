Feature: Replay cassette schema validation

  Scenario: Replay startup succeeds with a supported cassette
    Given a replay configuration with a supported cassette
    When the replay harness is started
    Then the replay harness is running
    And the replay cassette path matches the configured directory and name

  Scenario: Replay startup rejects an unsupported cassette version
    Given a replay configuration with cassette format version 9
    When the replay harness is started
    Then startup fails with an unsupported cassette format error
    And the error mentions format version 9
