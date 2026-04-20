Feature: Harness startup and shutdown

  Scenario: Start harness with valid configuration
    Given a valid harness configuration
    When the harness is started
    Then the harness is running
    And the cassette path matches the configured directory and name

  Scenario: Start harness with empty cassette name fails
    Given a harness configuration with an empty cassette name
    When the harness is started
    Then the startup fails with an invalid configuration error
    And the error message mentions the cassette name

  Scenario: Shutdown a running harness
    Given a valid harness configuration
    And the harness has been started
    When the harness is shut down
    Then the shutdown succeeds

  Scenario: Start harness binds an OS-selected localhost port
    Given a harness configuration with listen address 127.0.0.1:0
    When the harness is started
    Then the harness address is bound on 127.0.0.1
