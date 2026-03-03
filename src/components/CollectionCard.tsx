import { useMemo } from "react";
import type { Viewer } from "Origo";
import type { Collection } from "../lib/catalog-client.ts";
import { addCollectionToMap } from "../lib/add-layer.ts";

interface CollectionCardProps {
  collection: Collection;
  viewer: Viewer;
}

export function CollectionCard(props: CollectionCardProps) {
  const isAdded = useMemo(() => {
    return !!props.viewer.getLayer(props.collection.id);
  }, [props.viewer, props.collection.id]);

  const handleAdd = () => {
    addCollectionToMap(props.viewer, props.collection);
  };

  return (
    <div className="collection-card">
      <div className="collection-card-body">
        <div className="collection-card-title">{props.collection.title}</div>
        {props.collection.description && (
          <div className="collection-card-description">
            {props.collection.description}
          </div>
        )}
      </div>
      <div className="collection-card-actions">
        <button onClick={handleAdd}>{isAdded ? "Added" : "Add to map"}</button>
      </div>
    </div>
  );
}
