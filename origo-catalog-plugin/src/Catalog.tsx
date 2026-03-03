import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Collapse, CollapseHeader } from "./components/origo";
import type { Viewer } from "Origo";
import {
  CatalogClient,
  type CatalogSource,
  Collection,
  type CreateCollectionResult,
  CurrentUser,
  type FolderNode,
} from "./lib/catalog-client.ts";
import { OgcStacClient } from "./lib/ogc-stac-client.ts";
import { buildFolderTree } from "./lib/collections.ts";
import { SearchView } from "./components/SearchView.tsx";
import { UploadView } from "./components/UploadView.tsx";
import { FolderView } from "./components/FolderView.tsx";

type Selection =
  | { type: "search" }
  | { type: "upload" }
  | { type: "folder"; catalog: CatalogClient; path: (string | CurrentUser)[] };

function FolderTreeNode(props: {
  node: FolderNode;
  path: (string | CurrentUser)[];
  selectedPath: (string | CurrentUser)[] | null;
  onSelect: (path: (string | CurrentUser)[]) => void;
}) {
  const fullPath: (string | CurrentUser)[] = [...props.path, props.node.id];
  const [expanded] = useState(props.path.length < 1);

  const isSelected =
    props.selectedPath !== null &&
    fullPath.length === props.selectedPath.length &&
    fullPath.every((seg, i) => seg === props.selectedPath![i]);

  return (
    <Collapse
      cls=""
      expanded={expanded}
      collapseX={false}
      header={
        <CollapseHeader
          title={props.node.id === CurrentUser ? "Dina data" : props.node.label}
          hasChildren={props.node.children.length > 0}
          style={{
            height: "24px",
            textDecoration: isSelected ? "underline" : "none",
          }}
          onToggle={() => props.onSelect(fullPath)}
        />
      }
    >
      {props.node.children.length > 0 && (
        <ul className="divider-start padding-left padding-top-small">
          {props.node.children.map((child) => (
            <li key={typeof child.id === "string" ? child.id : "__currentuser"}>
              <FolderTreeNode
                node={child}
                path={fullPath}
                selectedPath={props.selectedPath}
                onSelect={props.onSelect}
              />
            </li>
          ))}
        </ul>
      )}
    </Collapse>
  );
}

function FolderTree(props: {
  catalog: CatalogClient;
  collections: Collection[];
  selection: Selection;
  onSelect: (sel: Selection) => void;
}) {
  const selectedFolderPath =
    props.selection.type === "folder" ? props.selection.path : null;

  const { data: title = "" } = useQuery({
    queryKey: ["catalo", props.catalog.url, "title"],
    queryFn: () => props.catalog.title,
  });
  const { data: capabilities = [] } = useQuery({
    queryKey: ["catalog", props.catalog.url, "capabilities"],
    queryFn: () => props.catalog.fetchCapabilities(),
  });

  const folderTree = useMemo(
    () => buildFolderTree(props.collections ?? [], capabilities),
    [props.collections, capabilities],
  );

  return (
    <div className="catalog">
      <strong>{title}</strong>
      {folderTree.map((node) => (
        <FolderTreeNode
          key={typeof node.id === "string" ? node.id : "__currentuser"}
          node={node}
          path={[]}
          selectedPath={selectedFolderPath}
          onSelect={(path) =>
            props.onSelect({ type: "folder", catalog: props.catalog, path })
          }
        />
      ))}
    </div>
  );
}

function Catalog(props: { viewer: Viewer; sources: CatalogSource[] }) {
  const clients = useMemo(
    () =>
      props.sources.map((source) => {
        if (source.type === "ogc-stac")
          return new OgcStacClient(source.url, source.name);
        throw new Error(`Unsupported catalog source type: ${source.type}`);
      }),
    [props.sources],
  );

  const {
    data: collections,
    isLoading: collectionsLoading,
    error: collectionsError,
  } = useQuery({
    queryKey: ["collections", clients.map((c) => c.url)],
    queryFn: () =>
      Promise.all(
        clients.map(async (c) => [c, await c.fetchCollections()] as const),
      ),
  });

  const [selection, setSelection] = useState<Selection>({
    type: "search",
  });

  return (
    <>
      <div className="catalog-sidebar">
        <FolderTreeNode
          node={{
            id: "__search",
            label: "Search",
            children: [],
          }}
          path={[]}
          selectedPath={selection.type === "search" ? ["__search"] : null}
          onSelect={() => setSelection({ type: "search" })}
        />
        <FolderTreeNode
          node={{
            id: "__upload",
            label: "Upload",
            children: [],
          }}
          path={[]}
          selectedPath={selection.type === "upload" ? ["__upload"] : null}
          onSelect={() => setSelection({ type: "upload" })}
        />
        {collections?.map(([client, cols], i) => (
          <FolderTree
            key={i}
            catalog={client}
            collections={cols}
            selection={selection}
            onSelect={setSelection}
          />
        ))}
      </div>
      <div className="catalog-page">
        {collectionsLoading && (
          <div className="loading-state">Loading collections...</div>
        )}
        {collectionsError && (
          <div className="error-state">
            Failed to load collections: {collectionsError?.message}
          </div>
        )}
        {!collectionsLoading && !collectionsError && (
          <>
            {selection.type === "search" && (
              <SearchView
                collections={collections?.flatMap(([_, c]) => c) ?? []}
                viewer={props.viewer}
              />
            )}
            {selection.type === "upload" && (
              <UploadView
                sources={clients}
                viewer={props.viewer}
                onCreated={(result: CreateCollectionResult) => {
                  const segments = result.id.split(":");
                  const path = segments.slice(0, -1);
                  if (path.length > 0) {
                    setSelection({
                      type: "folder",
                      catalog: clients[0],
                      path,
                    });
                  } else {
                    setSelection({ type: "search" });
                  }
                }}
              />
            )}
            {selection.type === "folder" && (
              <FolderView
                collections={
                  collections
                    ?.find(
                      ([catalog]) =>
                        (selection as { catalog: CatalogClient }).catalog ===
                        catalog,
                    )?.[1]
                    .filter((col) =>
                      (selection as { path: string[] }).path.every(
                        (v, idx) => col.folderPath[idx] === v,
                      ),
                    ) ?? []
                }
                folderPath={(selection as { path: string[] }).path}
                viewer={props.viewer}
              />
            )}
          </>
        )}
      </div>
    </>
  );
}

export default Catalog;
