import type { Viewer } from "Origo";
import type { Collection } from "../lib/catalog-client.ts";
import { CollectionCard } from "./CollectionCard.tsx";

interface FolderViewProps {
  collections: Collection[];
  folderPath: string[];
  viewer: Viewer;
}

export function FolderView(props: FolderViewProps) {
  return (
    <div className="folder-view">
      {props.folderPath.length > 0 && (
        <div className="breadcrumb">
          {props.folderPath.map((segment, i) => (
            <span key={i}>
              {i > 0 && <span className="breadcrumb-separator">/</span>}
              <span className="breadcrumb-segment">{segment}</span>
            </span>
          ))}
        </div>
      )}
      {props.collections.length > 0 ? (
        <div className="collection-list">
          {props.collections.map((col) => (
            <CollectionCard
              key={col.id}
              collection={col}
              viewer={props.viewer}
            />
          ))}
        </div>
      ) : (
        <div className="empty-state">No collections in this folder.</div>
      )}
    </div>
  );
}
