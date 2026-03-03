/**
 * Seed script for conformance testing.
 *
 * Creates collections and features that external validators (stac-api-validator,
 * OGC CITE) need to exercise the API.
 *
 * Usage:
 *   npx tsx seed.ts                              # default: http://localhost:8080
 *   npx tsx seed.ts --base-url http://host:8080
 */

const args = process.argv.slice(2);
let baseUrl = "http://localhost:8080";
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--base-url" && args[i + 1]) {
    baseUrl = args[i + 1];
    i++;
  }
}

// Strip trailing slash
baseUrl = baseUrl.replace(/\/$/, "");

async function request(
  method: string,
  path: string,
  body?: unknown,
): Promise<{ status: number; data: unknown }> {
  const url = `${baseUrl}${path}`;
  const res = await fetch(url, {
    method,
    headers: body ? { "Content-Type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    data = text;
  }
  if (!res.ok) {
    console.error(`${method} ${path} → ${res.status}`, data);
  }
  return { status: res.status, data };
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

const collections = [
  {
    id: "cities",
    title: "World Cities",
    description: "Major cities around the world (points)",
    collectionType: "vector",
    crs: 4326,
    columns: [
      { name: "name", type: "string" },
      { name: "population", type: "integer" },
      { name: "country", type: "string" },
    ],
  },
  {
    id: "lakes",
    title: "Notable Lakes",
    description: "Notable lakes around the world (polygons)",
    collectionType: "vector",
    crs: 4326,
    columns: [
      { name: "name", type: "string" },
      { name: "area_km2", type: "real" },
      { name: "depth_m", type: "real" },
    ],
  },
  {
    id: "observations",
    title: "Field Observations",
    description: "Timestamped field observations for temporal testing",
    collectionType: "vector",
    crs: 4326,
    columns: [
      { name: "observer", type: "string" },
      { name: "category", type: "string" },
      { name: "value", type: "real" },
    ],
  },
];

// ---------------------------------------------------------------------------
// Features
// ---------------------------------------------------------------------------

const cityFeatures = [
  { name: "Oslo", population: 709000, country: "Norway", coords: [10.75, 59.91] },
  { name: "Stockholm", population: 975000, country: "Sweden", coords: [18.07, 59.33] },
  { name: "Copenhagen", population: 794000, country: "Denmark", coords: [12.57, 55.68] },
  { name: "Helsinki", population: 656000, country: "Finland", coords: [24.94, 60.17] },
  { name: "Reykjavik", population: 131000, country: "Iceland", coords: [-21.90, 64.14] },
  { name: "London", population: 8982000, country: "UK", coords: [-0.12, 51.51] },
  { name: "Paris", population: 2161000, country: "France", coords: [2.35, 48.86] },
  { name: "Berlin", population: 3645000, country: "Germany", coords: [13.40, 52.52] },
  { name: "Tokyo", population: 13960000, country: "Japan", coords: [139.69, 35.69] },
  { name: "New York", population: 8336000, country: "USA", coords: [-74.01, 40.71] },
  { name: "Sydney", population: 5312000, country: "Australia", coords: [151.21, -33.87] },
  { name: "Cape Town", population: 4618000, country: "South Africa", coords: [18.42, -33.93] },
].map((c) => ({
  type: "Feature" as const,
  geometry: { type: "Point" as const, coordinates: c.coords },
  properties: { name: c.name, population: c.population, country: c.country },
}));

function lakePolygon(cx: number, cy: number, r: number) {
  // Simple square approximation for testing
  return {
    type: "Polygon" as const,
    coordinates: [
      [
        [cx - r, cy - r],
        [cx + r, cy - r],
        [cx + r, cy + r],
        [cx - r, cy + r],
        [cx - r, cy - r],
      ],
    ],
  };
}

const lakeFeatures = [
  { name: "Mjøsa", area_km2: 365, depth_m: 468, cx: 11.0, cy: 60.7, r: 0.2 },
  { name: "Vänern", area_km2: 5655, depth_m: 106, cx: 13.5, cy: 58.9, r: 0.5 },
  { name: "Saimaa", area_km2: 4400, depth_m: 82, cx: 28.5, cy: 61.5, r: 0.4 },
  { name: "Mälaren", area_km2: 1140, depth_m: 64, cx: 17.1, cy: 59.5, r: 0.3 },
  { name: "Bodensee", area_km2: 536, depth_m: 254, cx: 9.4, cy: 47.6, r: 0.15 },
  { name: "Garda", area_km2: 370, depth_m: 346, cx: 10.7, cy: 45.6, r: 0.1 },
  { name: "Geneva", area_km2: 580, depth_m: 310, cx: 6.5, cy: 46.4, r: 0.15 },
  { name: "Balaton", area_km2: 594, depth_m: 12.2, cx: 17.7, cy: 46.8, r: 0.2 },
  { name: "Loch Ness", area_km2: 56, depth_m: 230, cx: -4.5, cy: 57.3, r: 0.08 },
  { name: "Zurich", area_km2: 88, depth_m: 136, cx: 8.7, cy: 47.3, r: 0.06 },
].map((l) => ({
  type: "Feature" as const,
  geometry: lakePolygon(l.cx, l.cy, l.r),
  properties: { name: l.name, area_km2: l.area_km2, depth_m: l.depth_m },
}));

const observationFeatures = (() => {
  const categories = ["bird", "mammal", "plant", "insect", "fish"];
  const observers = ["alice", "bob", "charlie"];
  const features = [];
  for (let i = 0; i < 15; i++) {
    const lng = 5 + Math.random() * 20;
    const lat = 55 + Math.random() * 10;
    const day = String(i + 1).padStart(2, "0");
    features.push({
      type: "Feature" as const,
      geometry: { type: "Point" as const, coordinates: [+lng.toFixed(4), +lat.toFixed(4)] },
      properties: {
        datetime: `2025-06-${day}T12:00:00Z`,
        observer: observers[i % observers.length],
        category: categories[i % categories.length],
        value: +(Math.random() * 100).toFixed(1),
      },
    });
  }
  return features;
})();

const featuresByCollection: Record<string, object[]> = {
  cities: cityFeatures,
  lakes: lakeFeatures,
  observations: observationFeatures,
};

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function seed() {
  console.log(`Seeding ${baseUrl} ...`);

  // Wait for the server to be ready (up to 30s)
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const res = await fetch(`${baseUrl}/`);
      if (res.ok) break;
    } catch {
      // not ready yet
    }
    if (attempt === 29) {
      console.error("Server not ready after 30 seconds, giving up.");
      process.exit(1);
    }
    await new Promise((r) => setTimeout(r, 1000));
  }

  for (const col of collections) {
    console.log(`Creating collection: ${col.id}`);
    const { status, data } = await request("POST", "/collections", col);
    if (status !== 201 && status !== 409) {
      console.error(`  Failed to create collection ${col.id} (status ${status})`);
      continue;
    }

    // The server auto-prepends the owner to the collection id (e.g. "anonymous:cities")
    // Use the canonical id from the response for subsequent requests.
    let canonicalId: string;
    if (status === 409) {
      console.log(`  Collection ${col.id} already exists, checking for features...`);
      // Look up the actual canonical id from the collections list
      const { data: listData } = await request("GET", "/collections");
      const existing = (listData as any)?.collections?.find(
        (c: any) => c.id === col.id || c.id.endsWith(`:${col.id}`),
      );
      if (!existing) {
        console.error(`  Could not find existing collection matching ${col.id}`);
        continue;
      }
      canonicalId = existing.id;
      // Check if features already exist
      const { data: itemsData } = await request(
        "GET",
        `/collections/${canonicalId}/items?limit=1`,
      );
      if ((itemsData as any)?.features?.length > 0) {
        console.log(`  Collection ${canonicalId} already has features, skipping.`);
        continue;
      }
    } else {
      canonicalId = (data as any)?.id ?? col.id;
    }

    const features = featuresByCollection[col.id] ?? [];
    console.log(`  Inserting ${features.length} features into ${canonicalId}...`);
    for (const feature of features) {
      await request("POST", `/collections/${canonicalId}/items`, feature);
    }
  }

  console.log("Seed complete.");
}

seed().catch((err) => {
  console.error(err);
  process.exit(1);
});
