Feature: Features API
  Users can create, read, update, and delete features within vector collections.

  Background:
    Given I am authenticated as "testuser"
    And a vector collection "places" exists

  Scenario: List features in empty collection
    When I send a GET request to "/collections/testuser:places/items"
    Then the response status should be 200
    And the response "type" should be "FeatureCollection"
    And the response "features" should be an empty array
    And the response "numberReturned" should be "0"

  Scenario: Create a feature
    When I add a feature to "testuser:places" with:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "Berlin", "value": 100 }
      }
      """
    Then the response status should be 201
    And the response "type" should be "Feature"
    And the response "id" should exist
    And the response "geometry.type" should be "Point"
    And the response "properties.name" should be "Berlin"
    And the response should have an "etag" header

  Scenario: Get a feature by ID
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [2.35, 48.86] },
        "properties": { "name": "Paris", "value": 200 }
      }
      """
    When I send a GET request to "/collections/testuser:places/items/{featureId}"
    Then the response status should be 200
    And the response "type" should be "Feature"
    And the response "properties.name" should be "Paris"
    And the response should have an "etag" header
    And the response should have a "content-crs" header

  Scenario: Update a feature with PATCH
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "Origin", "value": 1 }
      }
      """
    When I send a PATCH request to "/collections/testuser:places/items/{featureId}" with the stored ETag and JSON:
      """
      { "properties": { "name": "Updated Origin" } }
      """
    Then the response status should be 200
    And the response "properties.name" should be "Updated Origin"

  Scenario: Replace a feature with PUT
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "Original", "value": 1 }
      }
      """
    When I send a PUT request to "/collections/testuser:places/items/{featureId}" with the stored ETag and JSON:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [1.0, 1.0] },
        "properties": { "name": "Replaced", "value": 99 }
      }
      """
    Then the response status should be 200
    And the response "properties.name" should be "Replaced"

  Scenario: Delete a feature
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "ToDelete", "value": 0 }
      }
      """
    When I send a DELETE request to "/collections/testuser:places/items/{featureId}" with the stored ETag
    Then the response status should be 204

  Scenario: Pagination with limit and offset
    Given the collection "testuser:places" has 5 features
    When I send a GET request to "/collections/testuser:places/items?limit=2"
    Then the response status should be 200
    And the response "features" array should have 2 items
    And the response "numberMatched" should be "5"
    And the response "numberReturned" should be "2"
    And the response should contain a link with rel "next"

  Scenario: Filter features by bounding box
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "InBox", "value": 1 }
      }
      """
    And the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [-100.0, -50.0] },
        "properties": { "name": "OutOfBox", "value": 2 }
      }
      """
    When I send a GET request to "/collections/testuser:places/items?bbox=5,45,15,55"
    Then the response status should be 200
    And the response "features" array should have 1 items

  Scenario: Filter features with CQL2 text filter
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [2.35, 48.86] },
        "properties": { "name": "Paris", "value": 200 }
      }
      """
    And the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [13.4, 52.5] },
        "properties": { "name": "Berlin", "value": 300 }
      }
      """
    When I send a GET request to "/collections/testuser:places/items?filter=name='Paris'&filter-lang=cql2-text"
    Then the response status should be 200
    And the response "features" array should have 1 items
    And the response "features[0].properties.name" should be "Paris"

  Scenario: Filter features with CQL2 text filter using comparison operator
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [2.35, 48.86] },
        "properties": { "name": "LowValue", "value": 50 }
      }
      """
    And the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [13.4, 52.5] },
        "properties": { "name": "HighValue", "value": 500 }
      }
      """
    When I send a GET request to "/collections/testuser:places/items?filter=value>100&filter-lang=cql2-text"
    Then the response status should be 200
    And the response "features" array should have 1 items
    And the response "features[0].properties.name" should be "HighValue"

  Scenario: CQL2 filter defaults to cql2-text when filter-lang is omitted
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "FilterDefault", "value": 42 }
      }
      """
    When I send a GET request to "/collections/testuser:places/items?filter=name='FilterDefault'"
    Then the response status should be 200
    And the response "features" array should have 1 items

  Scenario: Content-Crs header in feature list response
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "CrsTest", "value": 1 }
      }
      """
    When I send a GET request to "/collections/testuser:places/items"
    Then the response status should be 200
    And the response should have a "content-crs" header

  Scenario: Request features in a different CRS
    Given the collection "testuser:places" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [10.0, 50.0] },
        "properties": { "name": "CrsTransform", "value": 1 }
      }
      """
    When I send a GET request to "/collections/testuser:places/items?crs=http://www.opengis.net/def/crs/EPSG/0/3857"
    Then the response status should be 200
    And the response header "content-crs" should be "<http://www.opengis.net/def/crs/EPSG/0/3857>"
