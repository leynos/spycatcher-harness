Feature: Chat completions replay

  Scenario: Non-stream replay returns the recorded response without upstream access
    Given a stub upstream that returns a successful chat completion for replay
    And a record-mode harness configured for replay setup
    When the record harness is started
    And the baseline non-stream request is sent to the record harness
    And the record harness is stopped
    And a replay-mode harness is configured from the recorded cassette
    And the replay harness is started
    And the baseline non-stream request is sent to the replay harness
    Then the replay client receives the recorded response unchanged
    And the stub upstream saw no replay request
    And the replay harness is stopped
    And the replay stub upstream is stopped

  Scenario: Replay mismatch returns a conflict diagnostic
    Given a stub upstream that returns a successful chat completion for replay
    And a record-mode harness configured for replay setup
    When the record harness is started
    And the baseline non-stream request is sent to the record harness
    And the record harness is stopped
    And a replay-mode harness is configured from the recorded cassette
    And the replay harness is started
    And a different non-stream request is sent to the replay harness
    Then the replay client receives a request mismatch diagnostic
    And the replay harness is stopped
    And the replay stub upstream is stopped

  Scenario: Replay rejects streaming requests
    Given a stub upstream that returns a successful chat completion for replay
    And a record-mode harness configured for replay setup
    When the record harness is started
    And the baseline non-stream request is sent to the record harness
    And the record harness is stopped
    And a replay-mode harness is configured from the recorded cassette
    And the replay harness is started
    And a streaming request is sent to the replay harness
    Then the replay client receives an unsupported streaming response
    And the replay harness is stopped
    And the replay stub upstream is stopped
