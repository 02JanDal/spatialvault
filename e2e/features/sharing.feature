Feature: Collection Sharing
  Collection owners can share access with other users.

  Background:
    Given I am authenticated as "owner"
    And a vector collection "shared-data" exists

  Scenario: List shares on unshared collection
    When I send a GET request to "/collections/owner:shared-data/sharing"
    Then the response status should be 200
    And the response "shares" should be an empty array

  Scenario: Share a collection with another user
    When I share collection "owner:shared-data" with user "reader" for "read" access
    Then the response status should be 201
    When I send a GET request to "/collections/owner:shared-data/sharing"
    Then the response status should be 200
    And the response "shares" array should have 1 items

  Scenario: Remove a share
    When I share collection "owner:shared-data" with user "reader" for "read" access
    Then the response status should be 201
    When I send a DELETE request to "/collections/owner:shared-data/sharing/reader"
    Then the response status should be 204
    When I send a GET request to "/collections/owner:shared-data/sharing"
    Then the response status should be 200
    And the response "shares" should be an empty array
