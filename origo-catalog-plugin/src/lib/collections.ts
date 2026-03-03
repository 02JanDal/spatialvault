import {
  type CatalogCapability,
  type Collection,
  CurrentUser,
  type FolderNode,
  type Link,
} from "./catalog-client.ts";

export function buildFolderTree(
  collections: Collection[],
  capabilities: CatalogCapability[],
): FolderNode[] {
  const root: FolderNode = { id: "", label: "", children: [] };

  for (const col of collections) {
    let current = root;
    for (let i = 0; i < col.folderPath.length; i++) {
      const segment = col.folderPath[i];
      let child = current.children.find((c) => c.id === segment);
      if (!child) {
        child = {
          id: segment,
          label: segment === CurrentUser ? "" : segment,
          children: [],
        };
        current.children.push(child);
      }
      current = child;
    }
  }

  if (capabilities.includes("ownership-model-spatialvault")) {
    // move CurrentUser node to the start of root's children
    const userIndex = root.children.findIndex((c) => c.id === CurrentUser);
    if (userIndex > -1) {
      const [userNode] = root.children.splice(userIndex, 1);
      root.children.unshift(userNode);
    } else {
      // if CurrentUser node doesn't exist, add it at the start
      root.children.unshift({
        id: CurrentUser,
        label: "",
        children: [],
      });
    }
  }

  return root.children;
}

export function filterByFolder(
  collections: Collection[],
  path: string[],
): Collection[] {
  return collections.filter((col) => {
    if (col.folderPath.length < path.length) return false;
    return path.every((seg, i) => col.folderPath[i] === seg);
  });
}

export function searchCollections(
  collections: Collection[],
  query: string,
): Collection[] {
  if (!query.trim()) return collections;
  const lower = query.toLowerCase();
  return collections.filter(
    (col) =>
      col.title.toLowerCase().includes(lower) ||
      col.description?.toLowerCase().includes(lower),
  );
}

export function findTilesLink(collection: Collection): Link | undefined {
  return collection.links.find(
    (l) =>
      l.rel === "tiles" ||
      l.rel === "[ogc-rel]http://www.opengis.net/def/rel/ogc/1.0/tilesets" ||
      l.rel === "http://www.opengis.net/def/rel/ogc/1.0/tilesets-vector" ||
      l.rel === "http://www.opengis.net/def/rel/ogc/1.0/tilesets-map",
  );
}

export function findItemsLink(collection: Collection): Link | undefined {
  return collection.links.find(
    (l) =>
      l.rel === "items" ||
      l.rel === "http://www.opengis.net/def/rel/ogc/1.0/items",
  );
}
