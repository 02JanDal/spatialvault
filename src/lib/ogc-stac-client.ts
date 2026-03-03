import {
  type CatalogCapability,
  CatalogClient,
  Collection,
  type CreateCollectionParams,
  type CreateCollectionResult,
  CurrentUser,
  type Link,
} from "./catalog-client.ts";

interface CollectionsResponse {
  collections: Array<{
    id: string;
    title?: string;
    description?: string;
    links?: Array<{ href: string; rel: string; type?: string; title?: string }>;
  }>;
  links?: Array<{ href: string; rel: string; type?: string }>;
}

type ConformanceClass =
  | "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/core"
  | "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/landing-page"
  | "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/json"
  | "http://www.opengis.net/spec/ogcapi-common-1/1.0/conf/oas30"
  | "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/core"
  | "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/geojson"
  | "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/oas30"
  | "http://www.opengis.net/spec/ogcapi-features-2/1.0/conf/crs"
  | "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/create-replace-delete"
  | "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/update"
  | "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/optimistic-locking-etags"
  | "http://www.opengis.net/spec/ogcapi-tiles-1/1.0/conf/core"
  | "http://www.opengis.net/spec/ogcapi-tiles-1/1.0/conf/tileset"
  | "http://www.opengis.net/spec/ogcapi-coverages-1/0.0/conf/core"
  | "http://www.opengis.net/spec/ogcapi-coverages-1/0.0/conf/geotiff"
  | "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/core"
  | "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/json"
  | "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/ogc-process-description"
  | "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/job-list"
  | "http://www.opengis.net/spec/ogcapi-processes-1/1.0/conf/dismiss"
  | "https://api.stacspec.org/v1.0.0/core"
  | "https://api.stacspec.org/v1.0.0/collections"
  | "https://api.stacspec.org/v1.0.0/ogcapi-features"
  | "https://api.stacspec.org/v1.0.0/item-search"
  | "https://api.stacspec.org/v1.0.0/collections/extensions/transaction"
  | "https://api.stacspec.org/v1.0.0/ogcapi-features/extensions/transaction"
  | "http://www.opengis.net/spec/cql2/1.0/conf/cql2-text"
  | "http://www.opengis.net/spec/cql2/1.0/conf/cql2-json"
  | "http://www.opengis.net/spec/ogcapi-features-3/1.0/conf/filter"
  | "http://www.opengis.net/spec/ogcapi-features-3/1.0/conf/queryables"
  | "http://02jandal.github.io/spatialvault/spec/0.1/conf/collection-vector-upload"
  | "http://02jandal.github.io/spatialvault/spec/0.1/conf/item-attachment"
  | "http://02jandal.github.io/spatialvault/spec/0.1/conf/ownership-model-spatialvault";
interface ConformanceResponse {
  conformsTo: (string | ConformanceClass)[];
}

interface LandingResponse {
  title: string;
  description?: string;
  links: Array<{ href: string; rel: string; type?: string }>;
  currentUser?: string;
}

export class OgcStacClient extends CatalogClient {
  readonly #title: string;
  public constructor(
    public readonly url: string,
    title: string,
  ) {
    super();
    this.#title = title;
  }

  public override get title(): Promise<string> {
    return Promise.resolve(this.#title);
  }
  public override get currentUser(): Promise<string | undefined> {
    return this.fetchLanding().then((landing) => landing.currentUser);
  }

  private parseCollection(
    raw: CollectionsResponse["collections"][number],
    currentUser: string | undefined,
  ): Collection {
    const segments = raw.id.split(":");
    const folderPath = segments.length > 1 ? segments.slice(0, -1) : [];
    const links: Link[] = (raw.links ?? []).map((l) => ({
      href: l.href,
      rel: l.rel,
      type: l.type,
      title: l.title,
    }));

    return new Collection(
      this,
      raw.id,
      raw.title ?? raw.id,
      raw.description,
      this.url,
      currentUser && folderPath[0] === currentUser
        ? [CurrentUser, ...folderPath.slice(1)]
        : folderPath,
      links,
    );
  }

  override async fetchCollections(): Promise<Collection[]> {
    const collections: Collection[] = [];
    let url: string | undefined = `${this.url}/collections`;

    const landing = await this.fetchLanding();
    const capabilities = await this.fetchCapabilities();
    const currentUser = capabilities.includes("ownership-model-spatialvault")
      ? landing.currentUser
      : undefined;

    while (url) {
      const resp = await fetch(url, {
        headers: { Accept: "application/json", Authorization: "User jandal" },
      });
      if (!resp.ok) {
        throw new Error(
          `Failed to fetch collections from ${url}: ${resp.status}`,
        );
      }
      const data: CollectionsResponse = await resp.json();
      for (const raw of data.collections) {
        collections.push(this.parseCollection(raw, currentUser));
      }
      url = data.links?.find((l) => l.rel === "next")?.href;
    }

    return collections;
  }

  private landingFetch: Promise<LandingResponse> | undefined;
  private async fetchLanding(): Promise<LandingResponse> {
    if (!this.landingFetch) {
      this.landingFetch = fetch(this.url, {
        headers: { Accept: "application/json", Authorization: "User jandal" },
      }).then((r) => {
        if (!r.ok) {
          throw new Error(
            `Failed to fetch landing page from ${this.url}: ${r.status}`,
          );
        }
        return r.json();
      });
    }
    return await this.landingFetch;
  }

  private conformanceFetch: Promise<ConformanceResponse> | undefined;
  override async fetchCapabilities(): Promise<CatalogCapability[]> {
    if (!this.conformanceFetch) {
      this.conformanceFetch = fetch(`${this.url}/conformance`, {
        headers: { Accept: "application/json" },
      }).then((r) => {
        if (!r.ok) {
          throw new Error(
            `Failed to fetch conformance classes from ${this.url}/conformance: ${r.status}`,
          );
        }
        return r.json();
      });
    }
    const data = await this.conformanceFetch;
    const capabilities: CatalogCapability[] = [];

    if (
      data.conformsTo.includes(
        "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/core",
      )
    ) {
      capabilities.push("type-vector");
    }
    if (
      data.conformsTo.includes(
        "https://api.stacspec.org/v1.0.0/ogcapi-features",
      )
    ) {
      capabilities.push("type-raster");
      capabilities.push("type-pointcloud");
    }
    if (
      capabilities.includes("type-vector") &&
      (capabilities.includes("type-raster") ||
        capabilities.includes("type-pointcloud"))
    ) {
      if (
        data.conformsTo.includes(
          "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/create-replace-delete",
        ) &&
        data.conformsTo.includes(
          "https://api.stacspec.org/v1.0.0/ogcapi-features/extensions/transaction",
        )
      ) {
        capabilities.push("manage-items");
      }
    } else if (capabilities.includes("type-vector")) {
      if (
        data.conformsTo.includes(
          "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/create-replace-delete",
        )
      ) {
        capabilities.push("manage-items");
      }
    } else if (
      capabilities.includes("type-raster") ||
      capabilities.includes("type-pointcloud")
    ) {
      if (
        data.conformsTo.includes(
          "https://api.stacspec.org/v1.0.0/ogcapi-features/extensions/transaction",
        )
      ) {
        capabilities.push("manage-items");
      }
    }
    if (
      data.conformsTo.includes(
        "http://www.opengis.net/spec/ogcapi-features-4/1.0/conf/create-replace-delete",
      )
    ) {
      capabilities.push("manage-items");
    }
    if (
      data.conformsTo.includes(
        "https://api.stacspec.org/v1.0.0/collections/extensions/transaction",
      )
    ) {
      capabilities.push("manage-collections");
    }
    if (capabilities.includes("type-vector")) {
      if (
        data.conformsTo.includes(
          "http://02jandal.github.io/spatialvault/spec/0.1/conf/collection-vector-upload",
        )
      ) {
        capabilities.push("import-vector");
      }
    }
    if (
      data.conformsTo.includes("https://api.stacspec.org/v1.0.0/core") &&
      data.conformsTo.includes(
        "http://02jandal.github.io/spatialvault/spec/0.1/conf/item-attachment",
      )
    ) {
      capabilities.push("item-update-upload");
    }
    if (
      data.conformsTo.includes(
        "http://02jandal.github.io/spatialvault/spec/0.1/conf/ownership-model-spatialvault",
      )
    ) {
      capabilities.push("ownership-model-spatialvault");
    }

    return capabilities;
  }

  override async createCollection(
    params: CreateCollectionParams,
  ): Promise<CreateCollectionResult> {
    const metadata = {
      id: params.id,
      title: params.title,
      description: params.description,
      collectionType: params.collectionType,
      owner: params.owner,
      crs: params.crs ?? 4326,
      columns: params.columns,
    };

    let response: Response;
    if (params.file) {
      const formData = new FormData();
      formData.append(
        "metadata",
        new Blob([JSON.stringify(metadata)], { type: "application/json" }),
      );
      formData.append("file", params.file);
      response = await fetch(`${this.url}/collections`, {
        method: "POST",
        headers: { Authorization: "User jandal" },
        body: formData,
      });
    } else {
      response = await fetch(`${this.url}/collections`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: "User jandal",
        },
        body: JSON.stringify(metadata),
      });
    }

    if (!response.ok) {
      const text = await response.text();
      throw new Error(
        `Failed to create collection: ${response.status} ${text}`,
      );
    }

    const body = await response.json();
    return { id: body.id, title: body.title };
  }
}
