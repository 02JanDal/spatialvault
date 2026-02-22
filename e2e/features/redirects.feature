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

  Scenario: Features remain accessible after collection rename
    Given a vector collection "pre-rename" exists
    And the collection "testuser:pre-rename" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "Test Feature", "value": 1 }
      }
      """
    When I send a PATCH request to "/collections/testuser:pre-rename" with the stored ETag and JSON:
      """
      { "id": "testuser:post-rename" }
      """
    Then the response status should be 200
    And the response "id" should be "testuser:post-rename"
    When I send a GET request to "/collections/testuser:post-rename/items"
    Then the response status should be 200
    And the response "numberMatched" should be "1"
