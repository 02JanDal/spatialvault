import {Given, When, Then, defineParameterType} from "@cucumber/cucumber";
import {strict as assert} from "node:assert";
import {SpatialVaultWorld} from "../support/world";

Given(
    "I am authenticated as {string}",
    async function (this: SpatialVaultWorld, username: string) {
        await this.authenticateAs(username);
    }
);

When(
    "I send a GET request to {string}",
    async function (this: SpatialVaultWorld, path: string) {
        await this.request("GET", this.resolvePath(path));
    }
);

When(
    "I send a POST request to {string} with JSON:",
    async function (this: SpatialVaultWorld, path: string, docString: string) {
        await this.request("POST", this.resolvePath(path), JSON.parse(docString));
    }
);

When(
    "I send a PUT request to {string} with the stored ETag and JSON:",
    async function (this: SpatialVaultWorld, path: string, docString: string) {
        await this.request("PUT", this.resolvePath(path), JSON.parse(docString), {
            "If-Match": this.lastEtag,
        });
    }
);

When(
    "I send a PATCH request to {string} with the stored ETag and JSON:",
    async function (this: SpatialVaultWorld, path: string, docString: string) {
        await this.request("PATCH", this.resolvePath(path), JSON.parse(docString), {
            "Content-Type": "application/merge-patch+json",
            "If-Match": this.lastEtag,
        });
    }
);

When(
    "I send a PATCH request to {string} without an ETag and JSON:",
    async function (this: SpatialVaultWorld, path: string, docString: string) {
        await this.request("PATCH", this.resolvePath(path), JSON.parse(docString), {
            "Content-Type": "application/merge-patch+json",
        });
    }
);

When(
    "I send a PATCH request to {string} with ETag {string} and JSON:",
    async function (this: SpatialVaultWorld, path: string, etag: string, docString: string) {
        await this.request("PATCH", this.resolvePath(path), JSON.parse(docString), {
            "Content-Type": "application/merge-patch+json",
            "If-Match": etag,
        });
    }
);

When(
    "I send a PATCH request to {string} with saved ETag {string} and JSON:",
    async function (this: SpatialVaultWorld, path: string, etagName: string, docString: string) {
        const etag = this.savedValues.get(etagName);
        assert.ok(etag, `No saved ETag named "${etagName}"`);
        await this.request("PATCH", this.resolvePath(path), JSON.parse(docString), {
            "Content-Type": "application/merge-patch+json",
            "If-Match": etag!,
        });
    }
);

When(
    "I send a DELETE request to {string} with the stored ETag",
    async function (this: SpatialVaultWorld, path: string) {
        await this.request("DELETE", this.resolvePath(path), undefined, {
            "If-Match": this.lastEtag,
        });
    }
);

When(
    "I send a DELETE request to {string}",
    async function (this: SpatialVaultWorld, path: string) {
        await this.request("DELETE", this.resolvePath(path));
    }
);

Then(
    "the response status should be {int}",
    function (this: SpatialVaultWorld, expectedStatus: number) {
        assert.equal(
            this.responseStatus,
            expectedStatus,
            `Expected status ${expectedStatus} but got ${this.responseStatus}. Body: ${JSON.stringify(this.responseBody)}`
        );
    }
);

Then(
    "the response {string} should be {string}",
    function (this: SpatialVaultWorld, path: string, expected: string) {
        const value = this.getNestedValue(path);
        assert.equal(
            String(value),
            expected,
            `Expected "${path}" to be "${expected}", got: ${JSON.stringify(value)}`
        );
    }
);
Then(
    "the response {string} should be {int}",
    function (this: SpatialVaultWorld, path: string, expected: number) {
        const value = this.getNestedValue(path);
        assert.equal(
            Number(value),
            expected,
            `Expected "${path}" to be "${expected}", got: ${JSON.stringify(value)}`
        );
    }
);

Then(
    "the response {string} should exist",
    function (this: SpatialVaultWorld, path: string) {
        const value = this.getNestedValue(path);
        assert.ok(
            value !== undefined && value !== null,
            `Expected "${path}" to exist in response`
        );
    }
);

Then(
    "the response should have a(n) {string} header",
    function (this: SpatialVaultWorld, headerName: string) {
        const value = this.responseHeaders?.get(headerName);
        assert.ok(value, `Expected response to have "${headerName}" header`);
    }
);

type Comparisons = "equals" | "contains" | "startsWith" | "endsWith";
defineParameterType({
    name: "comparison",
    regexp: /should be|should equal|is|contains|should contain|starts with|should start with|ends with|should end with/,
    transformer: (s): Comparisons => {
        if (/(should )?(be|equal|is)/.test(s)) return "equals";
        if (/(should )?contains?/.test(s)) return "contains";
        if (/(should )?starts? with/.test(s)) return "startsWith";
        if (/(should )?ends? with/.test(s)) return "endsWith";
        throw new Error(`Unknown comparison type: ${s}`);
    },
});

Then(
    "the response header {string} {comparison} {string}",
    function (this: SpatialVaultWorld, headerName: string, comparison: Comparisons, expected: string) {
        const value = this.responseHeaders?.get(headerName);
        assert.ok(value, `Expected "${headerName}" header to exist`);
        if (comparison === "equals") {
            assert.equal(
                value, expected,
                `Expected "${headerName}" header to be "${expected}", got: ${value}`
            );
        } else if (comparison === "contains") {
            assert.ok(
                value.includes(expected),
                `Expected "${headerName}" header to contain "${expected}", got: ${value}`
            );
        } else if (comparison === "startsWith") {
            assert.ok(
                value.startsWith(expected),
                `Expected "${headerName}" header to start with "${expected}", got: ${value}`
            );
        } else if (comparison === "endsWith") {
            assert.ok(
                value.endsWith(expected),
                `Expected "${headerName}" header to end with "${expected}", got: ${value}`
            );
        }
    }
);

Then(
    "the response {string} should be a non-empty array",
    function (this: SpatialVaultWorld, path: string) {
        const value = this.getNestedValue(path);
        assert.ok(Array.isArray(value), `Expected "${path}" to be an array, got: ${typeof value}`);
        assert.ok(value.length > 0, `Expected "${path}" to be non-empty`);
    }
);

Then(
    "the response {string} should be an empty array",
    function (this: SpatialVaultWorld, path: string) {
        const value = this.getNestedValue(path);
        assert.ok(Array.isArray(value), `Expected "${path}" to be an array, got: ${typeof value}`);
        assert.equal(value.length, 0, `Expected "${path}" to be empty, got ${value.length} items`);
    }
);

Then(
    "the response {string} array should have {int} items",
    function (this: SpatialVaultWorld, path: string, count: number) {
        const value = this.getNestedValue(path);
        assert.ok(Array.isArray(value), `Expected "${path}" to be an array`);
        assert.equal(
            value.length,
            count,
            `Expected "${path}" to have ${count} items, got ${value.length}`
        );
    }
);

Then(
    "the response should contain a link with rel {string}",
    function (this: SpatialVaultWorld, rel: string) {
        const links = this.responseBody?.links;
        assert.ok(Array.isArray(links), "Response should have links array");
        const found = links.some((l: any) => l.rel === rel);
        assert.ok(
            found,
            `Expected link with rel "${rel}" in: ${JSON.stringify(links.map((l: any) => l.rel))}`
        );
    }
);

Given(
    "I store the ETag as {string}",
    function (this: SpatialVaultWorld, name: string) {
        assert.ok(this.lastEtag, "No ETag to store");
        this.savedValues.set(name, this.lastEtag);
    }
);
