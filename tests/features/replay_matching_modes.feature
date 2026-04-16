Feature: Replay matching modes

  Scenario: Sequential strict mode serves interactions in order
    Given a cassette with three recorded interactions
    And the replay engine is in sequential strict mode
    When three requests arrive with matching hashes in recorded order
    Then all three requests receive the corresponding recorded interaction

  Scenario: Sequential strict mode rejects a mismatched request
    Given a cassette with three recorded interactions
    And the replay engine is in sequential strict mode
    When a request arrives with a hash that does not match the next interaction
    Then the engine returns a mismatch diagnostic
    And the diagnostic contains the expected interaction ID
    And the diagnostic contains the expected and observed hashes
    And the diagnostic contains a field-level diff summary

  Scenario: Keyed mode permits out-of-order requests
    Given a cassette with three recorded interactions with distinct hashes
    And the replay engine is in keyed mode
    When three requests arrive with matching hashes in reversed order
    Then all three requests receive the corresponding recorded interaction

  Scenario: Keyed mode consumes duplicate hashes in recorded order
    Given a cassette with two interactions sharing the same hash
    And the replay engine is in keyed mode
    When two requests arrive with the shared hash
    Then the first request receives the first recorded interaction
    And the second request receives the second recorded interaction

  Scenario: Replay engine rejects requests after cassette exhaustion
    Given a cassette with one recorded interaction
    And the replay engine is in sequential strict mode
    When the first request matches and consumes the interaction
    And a second request arrives
    Then the engine returns a mismatch diagnostic indicating exhaustion
