Feature: Authorization
  Collections are only visible to their owner and explicitly shared users.
  Access level is enforced by the database via PostgreSQL roles.

  Scenario: Owner has full access to their collection
    Given I am authenticated as "alice"
    And a vector collection "private" exists
    When I add a feature to "alice:private" with:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "Test", "value": 1 }
      }
      """
    Then the response status should be 201
    When I send a GET request to "/collections/alice:private/items"
    Then the response status should be 200
    And the response "features" array should have 1 items

  Scenario: Non-owner cannot see unshared collection
    Given I am authenticated as "alice"
    And a vector collection "secret" exists
    And user "bob" exists
    When I am authenticated as "bob"
    And I send a GET request to "/collections/alice:secret"
    Then the response status should be 404
    When I send a GET request to "/collections/alice:secret/items"
    Then the response status should be 404

  Scenario: Non-owner cannot write to unshared collection
    Given I am authenticated as "alice"
    And a vector collection "closed" exists
    And user "bob" exists
    When I am authenticated as "bob"
    And I add a feature to "alice:closed" with:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "Intruder", "value": 0 }
      }
      """
    Then the response status should be 404

  Scenario: Read share grants read-only access
    Given I am authenticated as "alice"
    And a vector collection "readable" exists
    And the collection "alice:readable" has a feature:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "Existing", "value": 1 }
      }
      """
    When I share collection "alice:readable" with user "bob" for "read" access
    Then the response status should be 201
    When I am authenticated as "bob"
    And I send a GET request to "/collections/alice:readable"
    Then the response status should be 200
    When I send a GET request to "/collections/alice:readable/items"
    Then the response status should be 200
    When I add a feature to "alice:readable" with:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [1.0, 1.0] },
        "properties": { "name": "Unauthorized", "value": 99 }
      }
      """
    Then the response status should be 403

  Scenario: Write share grants item modification but not collection modification
    Given I am authenticated as "alice"
    And a vector collection "writable" exists
    When I share collection "alice:writable" with user "bob" for "write" access
    Then the response status should be 201
    When I am authenticated as "bob"
    And I add a feature to "alice:writable" with:
      """
      {
        "type": "Feature",
        "geometry": { "type": "Point", "coordinates": [0.0, 0.0] },
        "properties": { "name": "ByBob", "value": 42 }
      }
      """
    Then the response status should be 201
    When I send a PATCH request to "/collections/alice:writable" without an ETag and JSON:
      """
      { "title": "Unauthorized Change" }
      """
    Then the response status should be 403

  Scenario: Revoking a share removes access
    Given I am authenticated as "alice"
    And a vector collection "temporary" exists
    When I share collection "alice:temporary" with user "bob" for "read" access
    Then the response status should be 201
    When I am authenticated as "bob"
    And I send a GET request to "/collections/alice:temporary"
    Then the response status should be 200
    When I am authenticated as "alice"
    And I send a DELETE request to "/collections/alice:temporary/sharing/bob"
    Then the response status should be 204
    When I am authenticated as "bob"
    And I send a GET request to "/collections/alice:temporary"
    Then the response status should be 404

  Scenario: Only owner can delete a collection
    Given I am authenticated as "alice"
    And a vector collection "protected" exists
    When I share collection "alice:protected" with user "bob" for "write" access
    Then the response status should be 201
    When I am authenticated as "bob"
    And I send a GET request to "/collections/alice:protected"
    Then the response status should be 200
    When I send a DELETE request to "/collections/alice:protected" with the stored ETag
    Then the response status should be 403
    When I am authenticated as "alice"
    And I send a GET request to "/collections/alice:protected"
    Then the response status should be 200
    When I send a DELETE request to "/collections/alice:protected" with the stored ETag
    Then the response status should be 204
