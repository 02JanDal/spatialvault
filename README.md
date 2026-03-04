# Origo Catalog Plugin

A catalog browser UI for SpatialVault, built as an [Origo](https://github.com/origo-map/origo) web map plugin using
Solid.js and TypeScript. It connects to OGC API / STAC backends and lets users browse, search, and add collections to
the map.

## Architecture

```
Catalog (main component)
├── Sidebar
│   ├── "Search" button
│   ├── "Upload" button
│   └── Folder tree (derived from collection ID prefixes, e.g. "area:sub:layer")
└── Content area
    ├── SearchView    — search across all collections (debounced)
    ├── UploadView    — file upload skeleton (follow-up)
    └── FolderView    — collections in a selected folder
```

Collections are fetched via `GET {url}/collections` (with pagination). Each collection's `id` is split on `:` to derive
a folder hierarchy. Collections are displayed as cards with title, description, and an "Add to map" button.

### Adding layers to the map

When "Add to map" is clicked, the plugin inspects the collection's links:

1. **Tiles link** (`rel=tiles`) → MVT vector tile layer or XYZ raster tile layer (based on content type)
2. **Items link** (`rel=items`) → OGC API Features layer with GeoJSON format and bbox loading

## Configuration

The plugin accepts an array of catalog sources:

```typescript
CatalogPlugin({
    catalogs: [
        {
            url: "https://my-spatialvault.example.com",
            type: "ogc-stac",
            name: "My Catalog",
        },
    ],
});
```

## Development

```bash
npm install
npm run dev
```

This starts a Vite dev server with a local Origo instance. By default it points at `http://localhost:8484` as the
catalog source — adjust in `src/app.tsx` or run a local SpatialVault instance.

## Build

```bash
npm run build
```

Outputs to `dist/`.

## Project structure

```
src/
├── app.tsx                  # Plugin entry point + Origo integration
├── index.css                  # All styles
├── Catalog.tsx                # Main component, sidebar, data loading
├── components/
│   ├── CollectionCard.tsx     # Collection card with "Add to map"
│   ├── FolderView.tsx         # Folder contents + breadcrumb
│   ├── SearchView.tsx         # Search input + results
│   ├── UploadView.tsx         # Upload skeleton (drag-and-drop)
│   └── origo/                 # Solid.js wrappers for Origo UI components
└── lib/
    ├── catalog-client.ts      # Shared types (Collection, CatalogSource, etc.)
    ├── ogc-stac-client.ts     # API functions (fetchCollections, fetchCapabilities)
    ├── collections.ts         # Pure utilities (folder tree, filtering, search)
    └── add-layer.ts           # OpenLayers layer creation helpers
```
