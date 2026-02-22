import {Given, When} from "@cucumber/cucumber";
import {strict as assert} from "node:assert";
import {SpatialVaultWorld} from "../support/world";

Given(
    "a {word} collection {string} exists",
    async function (this: SpatialVaultWorld, type: string, id: string) {
        const body: any = {
            id,
            title: `Test ${id}`,
            collectionType: type,
            crs: 4326,
        };
        if (type === "vector") {
            body.columns = [
                {name: "name", type: "string"},
                {name: "value", type: "integer"},
            ];
        }
        await this.request("POST", "/collections", body);
        assert.equal(
            this.responseStatus,
            201,
            `Failed to create ${type} collection "${id}": ${JSON.stringify(this.responseBody)}`
        );
    }
);

When(
    "I create a {word} collection {string} titled {string}",
    async function (this: SpatialVaultWorld, type: string, id: string, title: string) {
        const body: any = {
            id,
            title,
            collectionType: type,
            crs: 4326,
        };
        if (type === "vector") {
            body.columns = [
                {name: "name", type: "string"},
                {name: "value", type: "integer"},
            ];
        }
        await this.request("POST", "/collections", body);
    }
);
