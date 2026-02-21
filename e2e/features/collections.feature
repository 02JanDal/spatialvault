Feature: Collections API

  Scenario: List collections on empty server
    When I send a GET request to "/collections"
    Then the response status should be 200
    And the response should contain "collections" as an empty array
