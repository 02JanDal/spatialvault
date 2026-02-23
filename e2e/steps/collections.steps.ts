import {Given, Then, When} from "@cucumber/cucumber";
import {strict as assert} from "node:assert";
import {Client} from "pg";
import {getDbConnectionUrl} from "../support/hooks";
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

Then(
    "the collection {string} should have table_name {string} in the database",
    async function (this: SpatialVaultWorld, canonicalName: string, expectedTableName: string) {
        const client = new Client({connectionString: getDbConnectionUrl()});
        try {
            await client.connect();
            const result = await client.query(
                "SELECT table_name FROM spatialvault.collections WHERE canonical_name = $1",
                [canonicalName]
            );
            assert.equal(result.rows.length, 1, `Collection "${canonicalName}" not found`);
            assert.equal(
                result.rows[0].table_name,
                expectedTableName,
                `Expected table_name "${expectedTableName}", got "${result.rows[0].table_name}"`
            );
        } finally {
            await client.end();
        }
    }
);

Then(
    "the database table {string}.{string} should exist",
    async function (this: SpatialVaultWorld, schemaName: string, tableName: string) {
        const client = new Client({connectionString: getDbConnectionUrl()});
        try {
            await client.connect();
            const result = await client.query(
                `SELECT 1 FROM information_schema.tables
                 WHERE table_schema = $1 AND table_name = $2`,
                [schemaName, tableName]
            );
            assert.equal(
                result.rows.length,
                1,
                `Expected table "${schemaName}"."${tableName}" to exist`
            );
        } finally {
            await client.end();
        }
    }
);

Then(
    "the database table {string}.{string} should not exist",
    async function (this: SpatialVaultWorld, schemaName: string, tableName: string) {
        const client = new Client({connectionString: getDbConnectionUrl()});
        try {
            await client.connect();
            const result = await client.query(
                `SELECT 1 FROM information_schema.tables
                 WHERE table_schema = $1 AND table_name = $2`,
                [schemaName, tableName]
            );
            assert.equal(
                result.rows.length,
                0,
                `Expected table "${schemaName}"."${tableName}" to not exist`
            );
        } finally {
            await client.end();
        }
    }
);
