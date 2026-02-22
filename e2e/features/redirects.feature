Feature: Collection Rename and Redirects
  Renamed collections create aliases that redirect requests to the new name.

  Background:
    Given I am authenticated as "testuser"

  Scenario: Rename a collection via PATCH
    Given a vector collection "original" exists
    When I send a PATCH request to "/collections/testuser:original" with the stored ETag and JSON:
      """
      { "id": "testuser:renamed" }
      """
    Then the response status should be 200
    And the response "id" should be "testuser:renamed"

  Scenario: Old name redirects after rename
    Given a vector collection "before" exists
    When I send a PATCH request to "/collections/testuser:before" with the stored ETag and JSON:
      """
      { "id": "testuser:after" }
      """
    Then the response status should be 200
    When I send a GET request to "/collections/testuser:before"
    Then the response status should be 307
    And the response header "location" ends with "/collections/testuser:after"

  Scenario: Alias redirects work for sub-resources
    Given a vector collection "old-name" exists
    When I send a PATCH request to "/collections/testuser:old-name" with the stored ETag and JSON:
      """
      { "id": "testuser:new-name" }
      """
    Then the response status should be 200
    When I send a GET request to "/collections/testuser:old-name/items"
    Then the response status should be 307
    And the response should have a "location" header
    When I send a GET request to "/collections/testuser:old-name/schema"
    Then the response status should be 307

  Scenario: Rename updates the table_name in the collections registry
    Given a vector collection "tbl-before" exists
    When I send a PATCH request to "/collections/testuser:tbl-before" with the stored ETag and JSON:
      """
      { "id": "testuser:tbl-after" }
      """
    Then the response status should be 200
    And the collection "testuser:tbl-after" should have table_name "tbl_after" in the database
    And the database table "testuser"."tbl_after" should exist
    And the database table "testuser"."tbl_before" should not exist
