Feature: STAC API
  The API provides STAC catalog and item search endpoints.

  Scenario: STAC search with no matching results
    Given I am authenticated as "testuser"
    When I send a POST request to "/search" with JSON:
      """
      { "collections": "nonexistent", "limit": 10 }
      """
    Then the response status should be 200
    And the response "type" should be "FeatureCollection"
    And the response "features" should be an empty array

  Scenario: STAC search finds items in a collection
    Given I am authenticated as "testuser"
    And a vector collection "stac-search" exists
    And the collection "testuser:stac-search" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "Searchable", "value": 1, "datetime": "2024-01-01T00:00:00Z" }
      }
      """
    When I send a POST request to "/search" with JSON:
      """
      { "collections": "testuser:stac-search", "limit": 10 }
      """
    Then the response status should be 200
    And the response "type" should be "FeatureCollection"
    And the response "features" array should have 1 items

  Scenario: STAC search response includes context
    Given I am authenticated as "testuser"
    And a vector collection "stac-context" exists
    And the collection "testuser:stac-context" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "ContextTest", "value": 1 }
      }
      """
    When I send a POST request to "/search" with JSON:
      """
      { "collections": "testuser:stac-context", "limit": 10 }
      """
    Then the response status should be 200
    And the response "context.returned" should be 1
    And the response "context.limit" should be 10

  Scenario: STAC search via GET
    Given I am authenticated as "testuser"
    And a vector collection "stac-get" exists
    And the collection "testuser:stac-get" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "GetSearch", "value": 1, "datetime": "2024-01-01T00:00:00Z" }
      }
      """
    When I send a GET request to "/search?collections=testuser:stac-get"
    Then the response status should be 200
    And the response "features" array should have 1 items

  Scenario: STAC search with bbox filter
    Given I am authenticated as "testuser"
    And a vector collection "stac-bbox" exists
    And the collection "testuser:stac-bbox" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "InBox", "value": 1 }
      }
      """
    And the collection "testuser:stac-bbox" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [-100.0, -50.0] },
        "properties": { "name": "OutOfBox", "value": 2 }
      }
      """
    When I send a POST request to "/search" with JSON:
      """
      { "collections": "testuser:stac-bbox", "bbox": "5,45,15,55", "limit": 10 }
      """
    Then the response status should be 200
    And the response "features" array should have 1 items
