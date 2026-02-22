Feature: Optimistic Locking with ETags
  The API uses ETags for optimistic concurrency control on collections and features.

  Background:
    Given I am authenticated as "testuser"

  Scenario: Update with wrong ETag returns 412 Precondition Failed
    Given a vector collection "etag-test" exists
    When I send a PATCH request to "/collections/testuser:etag-test" with ETag "\"999\"" and JSON:
      """
      { "title": "Should Fail" }
      """
    Then the response status should be 412

  Scenario: Stale ETag is rejected after a successful update
    Given a vector collection "etag-stale" exists
    And I store the ETag as "firstEtag"
    When I send a PATCH request to "/collections/testuser:etag-stale" with the stored ETag and JSON:
      """
      { "title": "First Update" }
      """
    Then the response status should be 200
    And the response should have an "etag" header
    When I send a PATCH request to "/collections/testuser:etag-stale" with saved ETag "firstEtag" and JSON:
      """
      { "title": "Stale Update" }
      """
    Then the response status should be 412

  Scenario: Feature update with wrong ETag returns 412
    Given a vector collection "feat-etag" exists
    And the collection "testuser:feat-etag" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "Original", "value": 1 }
      }
      """
    When I send a PATCH request to "/collections/testuser:feat-etag/items/{featureId}" with ETag "\"999\"" and JSON:
      """
      { "properties": { "name": "Should Fail" } }
      """
    Then the response status should be 412

  Scenario: ETag is returned on creation and get
    Given a vector collection "etag-create" exists
    And the collection "testuser:etag-create" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "EtagCheck", "value": 1 }
      }
      """
    Then the response should have an "etag" header
    When I send a GET request to "/collections/testuser:etag-create/items/{featureId}"
    Then the response status should be 200
    And the response should have an "etag" header
