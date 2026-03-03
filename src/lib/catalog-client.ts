export const CurrentUser = Symbol("CurrentUser");
export type CurrentUser = typeof CurrentUser;

export class Collection {
  public constructor(
    public readonly client: CatalogClient,
    public readonly id: string,
    public readonly title: string,
    public readonly description: string | undefined,
    public readonly catalogUrl: string,
    public readonly folderPath: (string | CurrentUser)[],
    public readonly links: Link[],
  ) {}

  public findTilesLink(): Link | undefined {
    return this.links.find(
      (l) =>
        l.rel === "tiles" ||
        l.rel === "[ogc-rel]http://www.opengis.net/def/rel/ogc/1.0/tilesets" ||
        l.rel === "http://www.opengis.net/def/rel/ogc/1.0/tilesets-vector" ||
        l.rel === "http://www.opengis.net/def/rel/ogc/1.0/tilesets-map",
    );
  }

  public findItemsLink(): Link | undefined {
    return this.links.find(
      (l) =>
        l.rel === "items" ||
        l.rel === "http://www.opengis.net/def/rel/ogc/1.0/items",
    );
  }
}

export interface Link {
  href: string;
  rel: string;
  type?: string;
  title?: string;
}

export interface FolderNode {
  id: string | CurrentUser;
  label: string;
  children: FolderNode[];
}

export interface CatalogSource {
  url: string;
  type: "ogc-stac";
  name: string;
}

export type CatalogCapability =
  | "manage-collections"
  | "manage-items"
  | "import-vector"
  | "type-vector"
  | "type-raster"
  | "type-pointcloud"
  | "item-update-upload"
  | "ownership-model-spatialvault";

export interface CreateCollectionParams {
  id: string;
  title: string;
  description?: string;
  collectionType: string;
  owner?: string;
  crs?: number;
  columns?: ColumnDef[];
  file?: File;
}

export interface ColumnDef {
  name: string;
  type: "string" | "integer" | "real" | "date" | "datetime" | "boolean";
  nullable: boolean;
  default?: unknown;
}

export interface CreateCollectionResult {
  id: string;
  title: string;
}

export abstract class CatalogClient {
  public abstract readonly url: string;
  public abstract get title(): Promise<string>;
  public get currentUser(): Promise<string | undefined> {
    return Promise.resolve(undefined);
  }
  public abstract fetchCollections(): Promise<Collection[]>;
  public abstract fetchCapabilities(): Promise<CatalogCapability[]>;
  public abstract createCollection(
    params: CreateCollectionParams,
  ): Promise<CreateCollectionResult>;
}
