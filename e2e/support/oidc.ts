import * as http from "node:http";
import {generateKeyPair, exportJWK, SignJWT, CryptoKey} from "jose";

let server: http.Server | null = null;
let privateKey: CryptoKey;
let publicJwk: object;
let issuerUrl: string;

export interface TokenOptions {
    sub?: string;
    preferred_username?: string;
    groups?: string[];
    audience?: string;
}

export async function startOidcServer(): Promise<number> {
    const keyPair = await generateKeyPair("RS256");
    privateKey = keyPair.privateKey;
    const jwk = await exportJWK(keyPair.publicKey);
    jwk.kid = "test-key-1";
    jwk.use = "sig";
    jwk.alg = "RS256";
    publicJwk = jwk;

    return new Promise((resolve, reject) => {
        server = http.createServer((req, res) => {
            res.setHeader("Content-Type", "application/json");

            if (req.url === "/.well-known/openid-configuration") {
                res.end(
                    JSON.stringify({
                        issuer: issuerUrl,
                        authorization_endpoint: `${issuerUrl}/authorize`,
                        token_endpoint: `${issuerUrl}/token`,
                        jwks_uri: `${issuerUrl}/jwks`,
                        response_types_supported: ["code"],
                        subject_types_supported: ["public"],
                        id_token_signing_alg_values_supported: ["RS256"],
                    })
                );
            } else if (req.url === "/jwks") {
                res.end(JSON.stringify({keys: [publicJwk]}));
            } else {
                res.statusCode = 404;
                res.end(JSON.stringify({error: "not found"}));
            }
        });

        server.listen(0, "0.0.0.0", () => {
            const addr = server!.address();
            if (typeof addr === "object" && addr !== null) {
                const port = addr.port;
                issuerUrl = `http://host.testcontainers.internal:${port}`;
                console.log(`OIDC mock server started on port ${port}`);
                resolve(port);
            } else {
                reject(new Error("Failed to get server address"));
            }
        });
    });
}

export async function stopOidcServer(): Promise<void> {
    if (server) {
        return new Promise((resolve) => {
            server!.close(() => {
                server = null;
                console.log("OIDC mock server stopped");
                resolve();
            });
        });
    }
}

export async function generateToken(options: TokenOptions = {}): Promise<string> {
    const {
        sub = "test-user",
        preferred_username = "testuser",
        groups = ["admin"],
        audience = "spatialvault",
    } = options;

    return new SignJWT({
        sub,
        preferred_username,
        groups,
    })
        .setProtectedHeader({alg: "RS256", kid: "test-key-1"})
        .setIssuer(issuerUrl)
        .setAudience(audience)
        .setIssuedAt()
        .setExpirationTime("1h")
        .sign(privateKey);
}
