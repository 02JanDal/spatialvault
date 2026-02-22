import { Given, When } from "@cucumber/cucumber";
import { Client } from "pg";
import { SpatialVaultWorld } from "../support/world";
import { getDbConnectionUrl } from "../support/hooks";

/**
 * Ensure a user's PostgreSQL role exists by calling ensure_role directly.
 * This is needed before sharing a collection with a user who hasn't
 * yet made any requests to the API.
 */
async function ensureRoleExists(username: string): Promise<void> {
  const client = new Client({ connectionString: getDbConnectionUrl() });
  try {
    await client.connect();
    await client.query("SELECT spatialvault.ensure_role($1)", [username]);
  } finally {
    await client.end();
  }
}

Given(
  "user {string} exists",
  async function (this: SpatialVaultWorld, username: string) {
    await ensureRoleExists(username);
  }
);

When(
  "I share collection {string} with user {string} for {string} access",
  async function (
    this: SpatialVaultWorld,
    collectionId: string,
    principal: string,
    permission: string
  ) {
    await ensureRoleExists(principal);
    const body = {
      principal,
      principal_type: "user",
      permission,
    };
    await this.request("POST", `/collections/${collectionId}/sharing`, body);
  }
);
