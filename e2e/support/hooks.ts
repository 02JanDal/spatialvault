import {
  BeforeAll,
  AfterAll,
  Before,
  setDefaultTimeout,
  setWorldConstructor,
} from "@cucumber/cucumber";
import { Client } from "pg";
import { startOidcServer, stopOidcServer, generateToken } from "./oidc";
import { startEnvironment, stopEnvironment, type Environment } from "./containers";
import { SpatialVaultWorld } from "./world";

setDefaultTimeout(120_000);
setWorldConstructor(SpatialVaultWorld);

let sharedEnv: Environment;

/** Exposed for steps that need direct DB access (e.g., role creation). */
export function getDbConnectionUrl(): string {
  return sharedEnv.dbConnectionUrl;
}

BeforeAll(async function () {
  const oidcPort = await startOidcServer();
  sharedEnv = await startEnvironment(oidcPort);
});

AfterAll(async function () {
  await stopEnvironment();
  await stopOidcServer();
});

Before(async function (this: SpatialVaultWorld) {
  this.baseUrl = sharedEnv.baseUrl;
  this.token = await generateToken();
  this.lastEtag = "";
  this.savedValues = new Map();

  const client = new Client({ connectionString: sharedEnv.dbConnectionUrl });
  try {
    await client.connect();

    // Delete all application data (foreign keys cascade)
    await client.query("DELETE FROM spatialvault.collection_aliases");
    await client.query("DELETE FROM spatialvault.collections");
    await client.query("DELETE FROM spatialvault.processes_jobs");

    // Drop user-created schemas and recreate them empty.
    // We keep the roles intact so ensure_role doesn't need to re-run,
    // which avoids issues with role dependencies and GRANTs.
    const schemas = await client.query(
      `SELECT schema_name FROM information_schema.schemata
       WHERE schema_name NOT IN ('public', 'information_schema', 'spatialvault', 'tiger', 'tiger_data', 'topology')
       AND schema_name NOT LIKE 'pg_%'`
    );
    for (const row of schemas.rows) {
      await client.query(`DROP SCHEMA "${row.schema_name}" CASCADE`);
      await client.query(
        `CREATE SCHEMA "${row.schema_name}" AUTHORIZATION "${row.schema_name}"`
      );
    }
  } finally {
    await client.end();
  }
});
