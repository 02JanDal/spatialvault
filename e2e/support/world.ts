import { World, type IWorldOptions } from "@cucumber/cucumber";
import { generateToken } from "./oidc";

export class SpatialVaultWorld extends World {
  public baseUrl: string = "";
  public token: string = "";
  public response: Response | null = null;
  public responseBody: any = null;
  public responseStatus: number = 0;
  public responseHeaders: Headers | null = null;
  public lastEtag: string = "";
  public savedValues: Map<string, string> = new Map();

  constructor(options: IWorldOptions) {
    super(options);
  }

  async authenticateAs(username: string, groups: string[] = []): Promise<void> {
    this.token = await generateToken({
      sub: username,
      preferred_username: username,
      groups,
    });
  }

  resolvePath(path: string): string {
    return path.replace(/\{(\w+)\}/g, (match, name) => {
      const value = this.savedValues.get(name);
      if (!value) return match;
      return value;
    });
  }

  async request(
    method: string,
    path: string,
    body?: any,
    extraHeaders?: Record<string, string>
  ): Promise<void> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {};

    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }

    if (body !== undefined && !extraHeaders?.["Content-Type"]) {
      headers["Content-Type"] = "application/json";
    }

    if (extraHeaders) {
      Object.assign(headers, extraHeaders);
    }

    this.response = await fetch(url, {
      method,
      headers,
      body:
        body !== undefined
          ? typeof body === "string"
            ? body
            : JSON.stringify(body)
          : undefined,
      redirect: "manual",
    });

    this.responseStatus = this.response.status;
    this.responseHeaders = this.response.headers;

    const etag = this.response.headers.get("etag");
    if (etag) {
      this.lastEtag = etag;
    }

    const contentType = this.response.headers.get("content-type") || "";
    if (contentType.includes("json")) {
      this.responseBody = await this.response.json();
    } else {
      this.responseBody = await this.response.text();
    }
  }

  getNestedValue(path: string): any {
    const parts = path.split(".");
    let current = this.responseBody;
    for (const part of parts) {
      if (current == null) return undefined;
      const arrayMatch = part.match(/^(\w+)\[(\d+)\]$/);
      if (arrayMatch) {
        current = current[arrayMatch[1]];
        if (Array.isArray(current)) {
          current = current[parseInt(arrayMatch[2])];
        } else {
          return undefined;
        }
      } else {
        current = current[part];
      }
    }
    return current;
  }
}
