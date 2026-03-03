import {
    GenericContainer,
    Network,
    Wait,
    type StartedTestContainer,
    type StartedNetwork, TestContainers,
} from "testcontainers";
import { S3Client, CreateBucketCommand } from "@aws-sdk/client-s3";
import * as path from "node:path";

const S3_ACCESS_KEY = "accessKey1";
const S3_SECRET_KEY = "verySecretKey1";
const S3_BUCKET = "spatialvault-test";

let postgisContainer: StartedTestContainer;
let cloudserverContainer: StartedTestContainer;
let appContainer: StartedTestContainer;
let network: StartedNetwork;

export interface Environment {
    baseUrl: string;
    dbConnectionUrl: string;
}

export async function startEnvironment(oidcPort: number): Promise<Environment> {
    await TestContainers.exposeHostPorts(oidcPort);
    network = await new Network().start();

    // Start PostGIS and CloudServer in parallel
    console.log("Starting PostGIS and CloudServer containers...");
    const [postgis, cloudserver] = await Promise.all([
        new GenericContainer("postgis/postgis:16-3.4")
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
            .start(),
        new GenericContainer("zenko/cloudserver")
            .withNetwork(network)
            .withNetworkAliases("cloudserver")
            .withEnvironment({
                SCALITY_ACCESS_KEY_ID: S3_ACCESS_KEY,
                SCALITY_SECRET_ACCESS_KEY: S3_SECRET_KEY,
                S3DATA: "mem",
            })
            .withExposedPorts(8000)
            .withWaitStrategy(Wait.forLogMessage(/server started/))
            .start(),
    ]);
    postgisContainer = postgis;
    cloudserverContainer = cloudserver;

    // Create the test bucket
    const s3Port = cloudserverContainer.getMappedPort(8000);
    const s3Endpoint = `http://127.0.0.1:${s3Port}`;
    console.log(`CloudServer running at ${s3Endpoint}`);

    const s3Client = new S3Client({
        endpoint: s3Endpoint,
        region: "us-east-1",
        credentials: {
            accessKeyId: S3_ACCESS_KEY,
            secretAccessKey: S3_SECRET_KEY,
        },
        forcePathStyle: true,
    });
    await s3Client.send(new CreateBucketCommand({ Bucket: S3_BUCKET }));
    console.log(`Created S3 bucket: ${S3_BUCKET}`);

    const dbUrl = `postgresql://postgres:postgres@postgis:5432/spatialvault_test`;

    // Use pre-built image if SPATIALVAULT_IMAGE is set, otherwise build from Dockerfile
    let appImage: GenericContainer;
    if (process.env.SPATIALVAULT_IMAGE) {
        console.log(`Using pre-built image: ${process.env.SPATIALVAULT_IMAGE}`);
        appImage = new GenericContainer(process.env.SPATIALVAULT_IMAGE);
    } else {
        const projectRoot = path.resolve(__dirname, "../..");
        console.log("Building SpatialVault Docker image...");
        appImage = await GenericContainer.fromDockerfile(projectRoot).build();
    }

    console.log("Starting SpatialVault container...");
    appContainer = await appImage
        .withNetwork(network)
        .withNetworkAliases("spatialvault")
        .withEnvironment({
            SPATIALVAULT__DATABASE__URL: dbUrl,
            SPATIALVAULT__OIDC__ISSUER_URL: `http://host.testcontainers.internal:${oidcPort}`,
            SPATIALVAULT__OIDC__AUDIENCE: "spatialvault",
            SPATIALVAULT__S3__BUCKET: S3_BUCKET,
            SPATIALVAULT__S3__REGION: "us-east-1",
            SPATIALVAULT__S3__ENDPOINT: "http://cloudserver:8000",
            SPATIALVAULT__S3__ACCESS_KEY_ID: S3_ACCESS_KEY,
            SPATIALVAULT__S3__SECRET_ACCESS_KEY: S3_SECRET_KEY,
            SPATIALVAULT__BASE_URL: "http://localhost:8080",
        })
        .withExposedPorts(8080)
        .withWaitStrategy(Wait.forHttp("/", 8080).forStatusCode(200))
        .withStartupTimeout(5_000)
        .withLogConsumer((stream) => {
            if (process.env.DEBUG) {
                stream.pipe(process.stdout);
            }
        })
        .start();

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
    if (cloudserverContainer) {
        await cloudserverContainer.stop();
        console.log("CloudServer container stopped");
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
