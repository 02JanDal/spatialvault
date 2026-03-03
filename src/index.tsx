import Origo from "Origo";

import "Origo/build/css/style.css";
import CatalogPlugin from "./CatalogPlugin.tsx";

const origo = Origo("index.json", {
  svgSpritePath: "/node_modules/Origo/build/css/svg/",
})!;
origo.on("load", (viewer) => {
  const catalog = CatalogPlugin({
    catalogs: [
      {
        url: "http://localhost:8080",
        type: "ogc-stac",
        name: "Local SpatialVault",
      },
    ],
  });
  viewer.addComponent(catalog);
});
