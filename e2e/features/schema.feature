Feature: Collection Schema
  Collections expose a JSON Schema describing their feature properties.

  Background:
    Given I am authenticated as "testuser"

  Scenario: Get collection schema
    Given a vector collection "with-schema" exists
    When I send a GET request to "/collections/testuser:with-schema/schema"
    Then the response status should be 200
    And the response "type" should be "object"
    And the response "properties" should exist

  Scenario: Schema reflects declared columns
    Given a vector collection "typed-cols" exists
    When I send a GET request to "/collections/testuser:typed-cols/schema"
    Then the response status should be 200
    And the response "properties.name" should exist
    And the response "properties.value" should exist
