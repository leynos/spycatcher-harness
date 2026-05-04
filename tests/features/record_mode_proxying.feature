Feature: Record mode proxying for chat completions

  Scenario: Successful non-stream proxying records one interaction
    Given a stub upstream that returns a successful chat completion
    And a record-mode harness configured for that upstream
    When the harness is started
    And a non-stream chat completions request is sent to the harness
    Then the client receives the upstream response unchanged
    And the upstream receives the request body unchanged
    And the cassette contains one recorded interaction
    And the background services shut down cleanly

  Scenario: Redacted headers are not persisted
    Given a stub upstream that returns a successful chat completion
    And a record-mode harness configured for that upstream with header redaction
    When the harness is started
    And a non-stream chat completions request with header x-session-secret is sent to the harness
    Then the upstream receives the header x-session-secret
    And the cassette request headers omit x-session-secret
    And the background services shut down cleanly

  Scenario: Authorization is redacted by default
    Given a stub upstream that returns a successful chat completion
    And a record-mode harness configured for that upstream
    When the harness is started
    And a non-stream chat completions request with header Authorization is sent to the harness
    Then the client receives the upstream response unchanged
    And the upstream does not receive the header Authorization
    And the cassette request headers omit Authorization
    And the background services shut down cleanly

  Scenario: Streaming requests are rejected until streaming support lands
    Given a stub upstream that returns a successful chat completion
    And a record-mode harness configured for that upstream
    When the harness is started
    And a streaming chat completions request is sent to the harness
    Then the harness rejects the request as unsupported streaming
    And the cassette remains empty
    And the background services shut down cleanly

  Scenario: Upstream transport failures do not write to the cassette
    Given a record-mode harness configured with an unavailable upstream
    When the harness is started
    And a non-stream chat completions request is sent to the harness
    Then the harness returns a bad gateway error
    And the cassette remains empty
    And the background services shut down cleanly
