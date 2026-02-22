import { Given, When } from "@cucumber/cucumber";
import { strict as assert } from "node:assert";
import { SpatialVaultWorld } from "../support/world";

Given(
  "the collection {string} has a feature:",
  async function (this: SpatialVaultWorld, collectionId: string, docString: string) {
    const body = JSON.parse(docString);
    await this.request("POST", `/collections/${collectionId}/items`, body);
    assert.equal(
      this.responseStatus,
      201,
      `Failed to create feature: ${JSON.stringify(this.responseBody)}`
    );
    this.savedValues.set("featureId", this.responseBody.id);
  }
);

Given(
  "the collection {string} has {int} features",
  async function (this: SpatialVaultWorld, collectionId: string, count: number) {
    for (let i = 0; i < count; i++) {
      const body = {
        type: "Feature",
        geometry: { type: "Point", coordinates: [i * 10.0, i * 5.0] },
        properties: { name: `Feature ${i + 1}`, value: i + 1 },
      };
      await this.request("POST", `/collections/${collectionId}/items`, body);
      assert.equal(
        this.responseStatus,
        201,
        `Failed to create feature ${i + 1}: ${JSON.stringify(this.responseBody)}`
      );
    }
  }
);

When(
  "I add a feature to {string} with:",
  async function (this: SpatialVaultWorld, collectionId: string, docString: string) {
    const body = JSON.parse(docString);
    await this.request("POST", `/collections/${collectionId}/items`, body);
    if (this.responseStatus === 201 && this.responseBody?.id) {
      this.savedValues.set("featureId", this.responseBody.id);
    }
  }
);
