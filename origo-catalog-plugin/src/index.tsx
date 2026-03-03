import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import Catalog from "./Catalog.tsx";
import Origo, { type Viewer } from "Origo";
import type { CatalogSource } from "./lib/catalog-client.ts";

const queryClient = new QueryClient();

import "Origo/build/css/style.css";

import "./index.css";

interface CatalogPluginOptions {
  catalogs?: CatalogSource[];
}

function CatalogPlugin(options: CatalogPluginOptions = {}) {
  const sources: CatalogSource[] = options.catalogs ?? [];

  return Origo.ui.Component({
    name: "catalog",
    onAdd(e) {
      const viewer: Viewer = e.target;

      const modal = Origo.ui.Modal({
        title: "Katalog",
        content: "<div id='catalog-root'></div>",
        target: viewer.getId(),
        cls: "catalog-modal",
      });
      // replace default close button - we want to hide instead of destroying
      const defaultClose = modal.getComponents()[1].getComponents()[1];
      modal.getComponents()[1].removeComponent(defaultClose);
      const newClose = Origo.ui.Button({
        cls: "small round margin-top-smaller margin-bottom-auto margin-right icon-smaller grey-lightest no-shrink",
        icon: "#ic_close_24px",
        validStates: ["initial", "hidden"],
        click() {
          modal.hide();
        },
        ariaLabel: "Stäng",
      });
      modal.getComponents()[1].addComponent(newClose);
      Origo.ui.dom.replace(
        document.getElementById(defaultClose.getId())!,
        newClose.render!() as string,
      );
      // hook up events etc.
      newClose.onRender!();

      modal.hide();
      this.addComponent!(modal);

      viewer.getControlByName("legend")!.addButtonToTools(
        Origo.ui.Button({
          cls: "round compact primary icon-small margin-x-smaller",
          click() {
            modal.show();
          },
          style: {
            "align-self": "center",
          },
          title: "Hantera lager",
          icon: "#o_add_24px",
          iconStyle: {
            fill: "#fff",
          },
        }),
      );

      const root = document.getElementById("catalog-root");
      createRoot(root!).render(
        <QueryClientProvider client={queryClient}>
          <Catalog viewer={viewer} sources={sources} />
        </QueryClientProvider>,
      );
    },
  });
}

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
