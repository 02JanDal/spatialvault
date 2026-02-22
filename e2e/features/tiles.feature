Feature: Tiles API
  The API provides OGC API Tiles for serving vector and raster tiles.

  Background:
    Given I am authenticated as "testuser"

  Scenario: List tile matrix sets
    When I send a GET request to "/tileMatrixSets"
    Then the response status should be 200
    And the response "tileMatrixSets" should be a non-empty array

  Scenario: Get tileset metadata for a vector collection
    Given a vector collection "tileable" exists
    When I send a GET request to "/collections/testuser:tileable/tiles"
    Then the response status should be 200
    And the response "dataType" should be "vector"
    And the response "links" should be a non-empty array
