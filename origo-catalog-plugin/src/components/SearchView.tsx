import { useRef, useState } from "react";
import type { Viewer } from "Origo";
import { Input } from "./origo";
import type { Collection } from "../lib/catalog-client.ts";
import { CollectionCard } from "./CollectionCard.tsx";

interface SearchViewProps {
  collections: Collection[];
  viewer: Viewer;
}

export function SearchView(props: SearchViewProps) {
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const [query, setQuery] = useState("");

  const handleInput = (value: string) => {
    clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      setQuery(value);
    }, 300);
  };

  return (
    <div className="search-view">
      <div className="search-bar">
        <Input
          cls="search-input"
          placeholderText="Search collections..."
          value={query}
          onChange={handleInput}
        />
      </div>
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
        <div className="empty-state">
          {query.trim()
            ? "No collections match your search."
            : "No collections available."}
        </div>
      )}
    </div>
  );
}
