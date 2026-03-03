import Origo, { type Viewer } from "Origo";
import type { Collection } from "./catalog-client.ts";
import { findItemsLink, findTilesLink } from "./collections.ts";

const MVT_TYPES = [
  "application/vnd.mapbox-vector-tile",
  "application/x-protobuf",
];

function isMvtType(type: string | undefined): boolean {
  if (!type) return false;
  return MVT_TYPES.some((t) => type.includes(t));
}

export function addCollectionToMap(
  viewer: Viewer,
  collection: Collection,
): void {
  const existing = viewer.getLayer(collection.id);
  if (existing) return;

  /* TODO: re-enable when better tested
  const tilesLink = findTilesLink(collection);
  if (tilesLink) {
    addTileLayer(viewer, collection, tilesLink);
    return;
  }*/

  const itemsLink = findItemsLink(collection);
  if (itemsLink) {
    addFeaturesLayer(viewer, collection, itemsLink);
    return;
  }
}

function addTileLayer(
  viewer: Viewer,
  collection: Collection,
  link: { href: string; type?: string },
): void {
  const map = viewer.getMap();

  if (isMvtType(link.type)) {
    const vtSource = new Origo.ol.source.VectorTile({
      format: new Origo.ol.format.MVT(),
      url: link.href + "/{z}/{x}/{y}.pbf",
    });
    const vtLayer = new Origo.ol.layer.VectorTile({
      source: vtSource,
      properties: {
        name: collection.id,
        title: collection.title,
        removable: true,
        visible: true,
      },
    });
    map.addLayer(vtLayer);
  } else {
    const xyzSource = new Origo.ol.source.XYZ({
      url: link.href + "/{z}/{x}/{y}.png",
    });
    const tileLayer = new Origo.ol.layer.Tile({
      source: xyzSource,
      properties: {
        name: collection.id,
        title: collection.title,
        removable: true,
        visible: true,
      },
    });
    map.addLayer(tileLayer);
  }
}

function addFeaturesLayer(
  viewer: Viewer,
  collection: Collection,
  link: { href: string },
): void {
  const map = viewer.getMap();
  const format = new Origo.ol.format.GeoJSON();

  const vectorSource = new Origo.ol.source.Vector({
    format,
    url: (extent: number[]) => {
      const bbox = extent.join(",");
      const separator = link.href.includes("?") ? "&" : "?";
      return `${link.href}${separator}f=json&bbox=${bbox}`;
    },
  });

  const vectorLayer = new Origo.ol.layer.Vector({
    source: vectorSource,
    properties: {
      name: collection.id,
      title: collection.title,
      removable: true,
      visible: true,
    },
  });
  map.addLayer(vectorLayer);
}
