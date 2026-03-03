import { useState } from "react";
import { Button, Dropdown, Input, InputFile, ToggleGroup } from "./origo";
import {
  CatalogClient,
  type CreateCollectionParams,
  type CreateCollectionResult,
} from "../lib/catalog-client.ts";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQueries,
} from "@tanstack/react-query";
import type { Viewer } from "Origo";
import Origo from "Origo";

interface ColumnDef {
  name: string;
  type: "string" | "integer" | "real" | "date" | "datetime" | "bool";
  nullable: boolean;
  defaultValue: string;
}

const COLUMN_TYPES: ColumnDef["type"][] = [
  "string",
  "integer",
  "real",
  "date",
  "datetime",
  "bool",
];

const CRS_OPTIONS = ["EPSG:4326", "EPSG:3006", "EPSG:3857"];

interface UploadViewProps {
  sources: CatalogClient[];
  viewer: Viewer;
  onCreated?: (result: CreateCollectionResult) => void;
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9-]/g, "");
}

export function UploadView(props: UploadViewProps) {
  const queryClient = useQueryClient();

  const [selectedSource, setSelectedSource] = useState<
    CatalogClient | undefined
  >(props.sources[0]);
  const [dragOver, setDragOver] = useState(false);
  const [tab, setTab] = useState("upload");

  // Shared form state
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [folder, setFolder] = useState("");

  // Upload tab state
  const [file, setFile] = useState<File | null>(null);

  // "From map" state
  const [selectedLayer, setSelectedLayer] = useState<string | undefined>();

  // "New collection" state
  const [crs, setCrs] = useState("EPSG:4326");
  const [columns, setColumns] = useState<ColumnDef[]>([]);

  const titles = useSuspenseQueries({
    queries: props.sources.map((s) => ({
      queryKey: ["catalog", s.url, "title"],
      queryFn: async () => (await s.title) ?? null,
    })),
    combine: (result) => result.map((r) => r.data),
  });

  const dropdownItems = props.sources.map((s, idx) => ({
    label: titles[idx],
    value: s.url,
  }));

  const { data: currentUser } = useQuery({
    queryKey: ["catalog", selectedSource?.url, "currentUser"],
    enabled: selectedSource !== undefined,
    queryFn: () => selectedSource?.currentUser,
  });

  const resetForm = () => {
    setTitle("");
    setDescription("");
    setFolder("");
    setFile(null);
    setSelectedLayer(undefined);
    setCrs("EPSG:4326");
    setColumns([]);
  };

  const mutation = useMutation({
    mutationFn: (params: CreateCollectionParams) =>
      selectedSource!.createCollection(params),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ["collections"] });
      props.onCreated?.(result);
      resetForm();
    },
  });

  const buildCollectionId = (): string => {
    const slug = slugify(title);
    const parts: string[] = [];
    if (folder.trim()) {
      parts.push(...folder.trim().split(/[:/]/).filter(Boolean));
    }
    parts.push(slug);
    return parts.join(":");
  };

  const handleSubmit = () => {
    if (!selectedSource || !title.trim()) return;

    const id = buildCollectionId();
    const base: CreateCollectionParams = {
      id,
      title: title.trim(),
      description: description.trim() || undefined,
      collectionType: "vector",
      owner: currentUser || undefined,
    };

    if (tab === "upload") {
      mutation.mutate({ ...base, file: file ?? undefined });
    } else if (tab === "from-map") {
      if (!selectedLayer) return;
      const layer = props.viewer.getLayer(selectedLayer);
      const source = layer.getSource();
      const features = source.getFeatures();
      const sourceProjection =
        source.getProjection?.()?.getCode?.() ??
        props.viewer.getProjectionCode() ??
        "EPSG:3857";
      const format = new Origo.ol.format.GeoJSON();
      const geojson = format.writeFeaturesObject(features, {
        featureProjection: sourceProjection,
        dataProjection: "EPSG:4326",
      });
      const blob = new Blob([JSON.stringify(geojson)], {
        type: "application/geo+json",
      });
      const geojsonFile = new File([blob], `${selectedLayer}.geojson`, {
        type: "application/geo+json",
      });
      mutation.mutate({ ...base, file: geojsonFile });
    } else if (tab === "new") {
      const crsCode = parseInt(crs.replace("EPSG:", ""), 10);
      mutation.mutate({
        ...base,
        crs: crsCode,
        columns: columns
          .filter((c) => c.name.trim())
          .map((c) => ({
            name: c.name.trim(),
            type: c.type === "bool" ? "boolean" : c.type,
            nullable: c.nullable,
            default: c.defaultValue.trim() || undefined,
          })),
      });
    }
  };

  const isSubmitDisabled =
    !selectedSource ||
    !title.trim() ||
    (tab === "from-map" && !selectedLayer) ||
    (tab === "upload" && !file);

  const buttonState = mutation.isPending
    ? "loading"
    : isSubmitDisabled
      ? "disabled"
      : "initial";

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(true);
  };

  const handleDragLeave = () => {
    setDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    const droppedFile = e.dataTransfer.files[0];
    if (droppedFile) {
      setFile(droppedFile);
    }
  };

  const handleFileChange = (evt: React.ChangeEvent<HTMLInputElement>) => {
    const selected = evt.target.files?.[0];
    if (selected) {
      setFile(selected);
    }
  };

  const mapLayers = props.viewer
    .getLayers()
    .filter((l: any) => {
      const type = l.get("type");
      return type === "WFS" || type === "GEOJSON" || type === "AGS_FEATURE";
    })
    .map((l: any) => ({
      label: l.get("title") || l.get("name"),
      value: l.get("name"),
    }));

  const addColumn = () => {
    setColumns((prev) => [
      ...prev,
      { name: "", type: "string", nullable: true, defaultValue: "" },
    ]);
  };

  const updateColumn = (index: number, updates: Partial<ColumnDef>) => {
    setColumns((prev) =>
      prev.map((col, i) => (i === index ? { ...col, ...updates } : col)),
    );
  };

  const removeColumn = (index: number) => {
    setColumns((prev) => prev.filter((_, i) => i !== index));
  };

  return (
    <div className="upload-view">
      {props.sources.length > 1 && (
        <div className="upload-catalog-select">
          <label className="text-smaller">Upload to:</label>
          <Dropdown
            items={dropdownItems}
            text={
              (selectedSource &&
                titles[props.sources.indexOf(selectedSource)]) ??
              "Select catalog"
            }
            onSelect={(item) => {
              const source = props.sources.find((s) => s.url === item.value);
              setSelectedSource(source);
            }}
          />
        </div>
      )}

      <div className="upload-tabs">
        <ToggleGroup
          items={[
            { id: "upload", label: "Upload file", value: "upload" },
            { id: "from-map", label: "From map", value: "from-map" },
            { id: "new", label: "New collection", value: "new" },
          ]}
          value={tab}
          onChange={setTab}
        />
      </div>

      {tab === "upload" && (
        <div
          className={`upload-dropzone ${dragOver ? "drag-over" : ""}`}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
        >
          <div className="upload-dropzone-content">
            <span className="upload-dropzone-icon">
              <svg
                width="48"
                height="48"
                viewBox="0 0 24 24"
                fill="currentColor"
              >
                <path d="M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM14 13v4h-4v-4H7l5-5 5 5h-3z" />
              </svg>
            </span>
            {file ? (
              <p>{file.name}</p>
            ) : (
              <>
                <p>Drag and drop files here</p>
                <p className="text-smaller grey">or</p>
              </>
            )}
            <InputFile
              labelCls="upload-browse-label"
              onChange={handleFileChange}
            />
          </div>
        </div>
      )}

      {tab === "from-map" && (
        <div className="from-map-tab">
          <label>Choose a layer from the map:</label>
          <Dropdown
            items={mapLayers}
            text={
              mapLayers.find(
                (l: { value: string }) => l.value === selectedLayer,
              )?.label ?? "Select layer"
            }
            onSelect={(item) => setSelectedLayer(item.value)}
          />
          {mapLayers.length === 0 && (
            <p className="text-smaller grey">
              No vector layers found on the map.
            </p>
          )}
        </div>
      )}

      {tab === "new" && (
        <div className="new-collection-tab">
          <label>CRS:</label>
          <Dropdown
            items={CRS_OPTIONS}
            text={crs}
            onSelect={(item) => setCrs(item.value)}
          />

          <label className="margin-top">Columns:</label>
          <div className="columns-editor">
            {columns.map((col, i) => (
              <div className="column-row" key={i}>
                <Input
                  placeholder="Name"
                  value={col.name}
                  onChange={(v) => updateColumn(i, { name: v })}
                />
                <Dropdown
                  items={COLUMN_TYPES}
                  text={col.type}
                  onSelect={(item) =>
                    updateColumn(i, { type: item.value as ColumnDef["type"] })
                  }
                />
                <ToggleGroup
                  items={[
                    { id: `nullable-yes-${i}`, label: "Null", value: "yes" },
                    { id: `nullable-no-${i}`, label: "Not null", value: "no" },
                  ]}
                  value={col.nullable ? "yes" : "no"}
                  onChange={(v) => updateColumn(i, { nullable: v === "yes" })}
                />
                <Input
                  placeholder="Default"
                  value={col.defaultValue}
                  onChange={(v) => updateColumn(i, { defaultValue: v })}
                />
                <Button
                  text="×"
                  cls="padding-small"
                  onClick={() => removeColumn(i)}
                />
              </div>
            ))}
            <Button
              text="Add column"
              cls="padding-small light box-shadow"
              onClick={addColumn}
            />
          </div>
        </div>
      )}

      <hr />
      <label htmlFor="collection-title">Titel</label>
      <Input
        id="collection-title"
        placeholder="Enter title for the collection"
        value={title}
        onChange={setTitle}
      />
      <label htmlFor="collection-folder">Mapp</label>
      <Input
        id="collection-folder"
        placeholder="Folder path (e.g. my-folder)"
        value={folder}
        onChange={setFolder}
      />
      <label htmlFor="collection-description">Description</label>
      <Input
        id="collection-description"
        placeholder="Enter description for the collection"
        value={description}
        onChange={setDescription}
      />
      <label htmlFor="collection-owner">Ägare</label>
      <Dropdown
        id="collection-owner"
        items={[{ label: currentUser!, value: currentUser! }]}
        text={currentUser ?? ""}
      />
      <Button
        text="Ladda upp"
        state={buttonState}
        cls="padding-small light box-shadow margin-top"
        onClick={handleSubmit}
      />
      {mutation.error && (
        <p className="text-smaller" style={{ color: "red", marginTop: "8px" }}>
          {mutation.error.message}
        </p>
      )}
    </div>
  );
}
