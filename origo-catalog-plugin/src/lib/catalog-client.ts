export interface Collection {
  id: string;
  tile: string;
  description: string | undefined;
  folderPath: string[];
}

export type CatalogCapability =
  | "manage-collections"
  | "manage-items"
  | "import-vector"
  | "type-vector"
  | "type-raster"
  | "type-pointcloud"
  | "item-update-upload";

export interface CatalogClient {
  capabilities: CatalogCapability[];
  folders: () => Promise<string[] | undefined>;
  collections: (folder: string) => Promise<Collection[]>;
}
