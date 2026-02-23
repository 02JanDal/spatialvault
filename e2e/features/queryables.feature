Feature: Collection Queryables
  Collections expose queryable properties that can be used in CQL2 filter expressions.

  Background:
    Given I am authenticated as "testuser"

  Scenario: Get collection queryables returns valid schema
    Given a vector collection "with-queryables" exists
    When I send a GET request to "/collections/testuser:with-queryables/queryables"
    Then the response status should be 200
    And the response header "content-type" contains "application/schema+json"
    And the response "$schema" should exist
    And the response "$id" should exist
    And the response "type" should be "object"
    And the response "properties" should exist

  Scenario: Queryables includes id property with x-ogc-role id
    Given a vector collection "q-roles" exists
    When I send a GET request to "/collections/testuser:q-roles/queryables"
    Then the response status should be 200
    And the response "properties.id" should exist
    And the response "properties.id.x-ogc-role" should be "id"

  Scenario: Queryables includes geometry property with x-ogc-role primary-geometry
    Given a vector collection "q-geom" exists
    When I send a GET request to "/collections/testuser:q-geom/queryables"
    Then the response status should be 200
    And the response "properties.geometry" should exist
    And the response "properties.geometry.x-ogc-role" should be "primary-geometry"

  Scenario: Queryables reflects declared user columns
    Given a vector collection "q-cols" exists
    When I send a GET request to "/collections/testuser:q-cols/queryables"
    Then the response status should be 200
    And the response "properties.name" should exist
    And the response "properties.value" should exist

  Scenario: Collection detail contains queryables link
    Given a vector collection "q-linked" exists
    When I send a GET request to "/collections/testuser:q-linked"
    Then the response status should be 200
    And the response should contain a link with rel "http://www.opengis.net/def/rel/ogc/1.0/queryables"

  Scenario: Queryables for non-existent collection returns 404
    When I send a GET request to "/collections/testuser:nonexistent/queryables"
    Then the response status should be 404
