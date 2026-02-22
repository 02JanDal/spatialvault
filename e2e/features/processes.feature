Feature: Processes API
  The API provides OGC API Processes for listing available processes and managing jobs.

  Background:
    Given I am authenticated as "testuser"

  Scenario: List available processes
    When I send a GET request to "/processes"
    Then the response status should be 200
    And the response "processes" should be a non-empty array

  Scenario: Get import-raster process description
    When I send a GET request to "/processes/import-raster"
    Then the response status should be 200
    And the response "id" should be "import-raster"
    And the response "title" should exist

  Scenario: Get import-pointcloud process description
    When I send a GET request to "/processes/import-pointcloud"
    Then the response status should be 200
    And the response "id" should be "import-pointcloud"
    And the response "title" should exist

  Scenario: Get non-existent process returns 404
    When I send a GET request to "/processes/nonexistent"
    Then the response status should be 404

  Scenario: List jobs when none exist
    When I send a GET request to "/jobs"
    Then the response status should be 200
    And the response "jobs" should be an empty array
