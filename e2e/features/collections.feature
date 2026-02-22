Feature: Collections API
  Users can create, read, update, and delete geospatial data collections.

  Background:
    Given I am authenticated as "testuser"

  Scenario: List collections when none exist
    When I send a GET request to "/collections"
    Then the response status should be 200
    And the response "collections" should be an empty array

  Scenario: Create a vector collection
    When I create a vector collection "cities" titled "World Cities"
    Then the response status should be 201
    And the response "id" should be "testuser:cities"
    And the response "title" should be "World Cities"
    And the response should have an "etag" header
    And the response should have a "location" header

  Scenario Outline: Create collections of different types
    When I create a <type> collection "<id>" titled "Test <type>"
    Then the response status should be 201
    And the response "id" should be "testuser:<id>"

    Examples:
      | type       | id         |
      | vector     | my-vectors |
      | raster     | my-rasters |
      | pointcloud | my-points  |

  Scenario: Get a collection
    Given a vector collection "parks" exists
    When I send a GET request to "/collections/testuser:parks"
    Then the response status should be 200
    And the response "id" should be "testuser:parks"
    And the response should have an "etag" header
    And the response should contain a link with rel "self"
    And the response should contain a link with rel "items"
    And the response should contain a link with rel "parent"

  Scenario: Get a non-existent collection returns 404
    When I send a GET request to "/collections/testuser:nonexistent"
    Then the response status should be 404

  Scenario: Update a collection title
    Given a vector collection "parks" exists
    When I send a PATCH request to "/collections/testuser:parks" with the stored ETag and JSON:
      """
      { "title": "Updated Parks" }
      """
    Then the response status should be 200
    And the response "title" should be "Updated Parks"
    And the response should have an "etag" header

  Scenario: Delete a collection
    Given a vector collection "temp" exists
    When I send a DELETE request to "/collections/testuser:temp" with the stored ETag
    Then the response status should be 204
    When I send a GET request to "/collections/testuser:temp"
    Then the response status should be 404

  Scenario: Created collection appears in list
    Given a vector collection "visible" exists
    When I send a GET request to "/collections"
    Then the response status should be 200
    And the response "collections" array should have 1 items

  Scenario: Collection detail has describedby link
    Given a vector collection "detailed" exists
    When I send a GET request to "/collections/testuser:detailed"
    Then the response status should be 200
    And the response should contain a link with rel "describedby"

  Scenario: Vector collection has tiles link
    Given a vector collection "tiled" exists
    When I send a GET request to "/collections/testuser:tiled"
    Then the response status should be 200
    And the response should contain a link with rel "tiles"

  Scenario: Raster collection has coverage and tiles links
    Given a raster collection "satellite" exists
    When I send a GET request to "/collections/testuser:satellite"
    Then the response status should be 200
    And the response should contain a link with rel "coverage"
    And the response should contain a link with rel "tiles"
