import {
    GenericContainer,
    Network,
    Wait,
    type StartedTestContainer,
    type StartedNetwork, TestContainers,
} from "testcontainers";
import * as path from "node:path";

let postgisContainer: StartedTestContainer;
let appContainer: StartedTestContainer;
let network: StartedNetwork;

export interface Environment {
    baseUrl: string;
    dbConnectionUrl: string;
}

export async function startEnvironment(oidcPort: number): Promise<Environment> {
    await TestContainers.exposeHostPorts(oidcPort);
    network = await new Network().start();

    // Start PostGIS
    console.log("Starting PostGIS container...");
    postgisContainer = await new GenericContainer("postgis/postgis:16-3.4")
        .withNetwork(network)
        .withNetworkAliases("postgis")
        .withEnvironment({
            POSTGRES_DB: "spatialvault_test",
            POSTGRES_USER: "postgres",
            POSTGRES_PASSWORD: "postgres",
        })
        .withExposedPorts(5432)
        .withWaitStrategy(
            Wait.forLogMessage(/database system is ready to accept connections/, 2)
        )
        .start();

    const dbUrl = `postgresql://postgres:postgres@postgis:5432/spatialvault_test`;
    const projectRoot = path.resolve(__dirname, "../..");

    // Build and start spatialvault from Dockerfile
    console.log("Building SpatialVault Docker image...");
    appContainer = await GenericContainer.fromDockerfile(projectRoot)
        .build()
        .then((image) => {
                console.log("Starting SpatialVault container...");
                return image
                    .withNetwork(network)
                    .withNetworkAliases("spatialvault")
                    .withEnvironment({
                        SPATIALVAULT__DATABASE__URL: dbUrl,
                        SPATIALVAULT__OIDC__ISSUER_URL: `http://host.testcontainers.internal:${oidcPort}`,
                        SPATIALVAULT__OIDC__AUDIENCE: "spatialvault",
                        SPATIALVAULT__S3__BUCKET: "test",
                        SPATIALVAULT__S3__REGION: "us-east-1",
                        SPATIALVAULT__S3__ENDPOINT: "http://localhost:9000",
                        SPATIALVAULT__S3__ACCESS_KEY_ID: "minioadmin",
                        SPATIALVAULT__S3__SECRET_ACCESS_KEY: "minioadmin",
                        SPATIALVAULT__BASE_URL: "http://localhost:8080",
                    })
                    .withExposedPorts(8080)
                    .withWaitStrategy(Wait.forHttp("/", 8080).forStatusCode(200))
                    .withStartupTimeout(5_000)
                    .withLogConsumer((stream) => {
                        // output to stdout
                        if (process.env.DEBUG) {
                            stream.pipe(process.stdout);
                        }
                    })
                    .start();
            }
        );

    const appHost = appContainer.getHost();
    const appPort = appContainer.getMappedPort(8080);
    const baseUrl = `http://${appHost}:${appPort}`;

    const pgHost = postgisContainer.getHost();
    const pgPort = postgisContainer.getMappedPort(5432);
    const dbConnectionUrl = `postgresql://postgres:postgres@${pgHost}:${pgPort}/spatialvault_test`;

    console.log(`SpatialVault running at ${baseUrl}`);
    console.log(`PostGIS available at ${pgHost}:${pgPort}`);

    return {baseUrl, dbConnectionUrl};
}

export async function stopEnvironment(): Promise<void> {
    if (appContainer) {
        await appContainer.stop();
        console.log("App container stopped");
    }
    if (postgisContainer) {
        await postgisContainer.stop();
        console.log("PostGIS container stopped");
    }
    if (network) {
        await network.stop();
        console.log("Network stopped");
    }
}
