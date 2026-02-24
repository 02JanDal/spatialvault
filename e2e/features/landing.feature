Feature: API Discovery
  The API provides standard OGC API discovery endpoints:
  a landing page, conformance declaration, and OpenAPI specification.

  Scenario: Landing page provides navigation links
    When I send a GET request to "/"
    Then the response status should be 200
    And the response "title" should exist
    And the response "links" should be a non-empty array
    And the response should contain a link with rel "self"
    And the response should contain a link with rel "conformance"
    And the response should contain a link with rel "data"
    And the response should contain a link with rel "service-desc"

  Scenario: Conformance declaration lists supported standards
    When I send a GET request to "/conformance"
    Then the response status should be 200
    And the response "conformsTo" should be a non-empty array

  Scenario: OpenAPI specification is available
    When I send a GET request to "/api"
    Then the response status should be 200
    And the response "openapi" should exist
    And the response "info" should exist
    And the response "paths" should exist

  Scenario: Landing page serves as STAC catalog root
    When I send a GET request to "/"
    Then the response status should be 200
    And the response "type" should be "Catalog"
    And the response "id" should exist
    And the response "stacVersion" should be "1.0.0"
    And the response "conformsTo" should be a non-empty array
    And the response should contain a link with rel "search"
    And the response should contain a link with rel "root"
