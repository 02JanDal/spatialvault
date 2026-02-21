import { When, Then } from "@cucumber/cucumber";
import { strict as assert } from "node:assert";
import { SpatialVaultWorld } from "../support/world";

When(
  "I send a GET request to {string}",
  async function (this: SpatialVaultWorld, path: string) {
    await this.request("GET", path);
  }
);

Then(
  "the response status should be {int}",
  async function (this: SpatialVaultWorld, expectedStatus: number) {
    assert.equal(
      this.responseStatus,
      expectedStatus,
      `Expected status ${expectedStatus} but got ${this.responseStatus}. Body: ${JSON.stringify(this.responseBody)}`
    );
  }
);

Then(
  "the response should contain {string} as an empty array",
  async function (this: SpatialVaultWorld, key: string) {
    assert.ok(this.responseBody, "Response body is empty");
    assert.ok(
      Array.isArray(this.responseBody[key]),
      `Expected "${key}" to be an array, got: ${typeof this.responseBody[key]}`
    );
    assert.equal(
      this.responseBody[key].length,
      0,
      `Expected "${key}" to be empty, got ${this.responseBody[key].length} items`
    );
  }
);
