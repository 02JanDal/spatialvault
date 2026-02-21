import { World, type IWorldOptions } from "@cucumber/cucumber";

export class SpatialVaultWorld extends World {
  public baseUrl: string = "";
  public token: string = "";
  public response: Response | null = null;
  public responseBody: any = null;
  public responseStatus: number = 0;

  constructor(options: IWorldOptions) {
    super(options);
  }

  async request(
    method: string,
    path: string,
    body?: object
  ): Promise<void> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {};

    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }

    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
    }

    this.response = await fetch(url, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });

    this.responseStatus = this.response.status;

    const contentType = this.response.headers.get("content-type") || "";
    if (contentType.includes("application/json")) {
      this.responseBody = await this.response.json();
    } else {
      this.responseBody = await this.response.text();
    }
  }
}
