Feature: Canonical request generation and stable hashing

  Scenario: Equivalent requests produce identical hashes
    Given two equivalent recorded requests with different query ordering
    When both requests are canonicalized
    Then both stable hashes are identical

  Scenario: Materially different requests produce different hashes
    Given two materially different recorded requests
    When both requests are canonicalized
    Then the stable hashes differ

  Scenario: Ignore paths remove metadata drift from hashing
    Given two requests that differ only in metadata run ids
    And ignore paths configured as "/metadata/run_id"
    When both requests are canonicalized
    Then both stable hashes are identical
