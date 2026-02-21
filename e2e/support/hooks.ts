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

  // Clean database between scenarios
  const client = new Client({ connectionString: sharedEnv.dbConnectionUrl });
  try {
    await client.connect();

    // Drop user-created schemas (collections create per-collection schemas)
    const schemas = await client.query(
      `SELECT schema_name FROM information_schema.schemata
       WHERE schema_name NOT IN ('public', 'information_schema', 'spatialvault')
       AND schema_name NOT LIKE 'pg_%'`
    );
    for (const row of schemas.rows) {
      await client.query(`DROP SCHEMA "${row.schema_name}" CASCADE`);
    }

    // Truncate collections table
    await client.query(
      `DELETE FROM spatialvault.collections`
    );
  } finally {
    await client.end();
  }
});
