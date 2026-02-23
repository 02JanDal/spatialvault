/* @refresh reload */
import { render } from "solid-js/web";
import Catalog from "./Catalog.tsx";
import Origo, { type Viewer } from "Origo";

import "Origo/build/css/style.css";

import "./index.css";

function CatalogPlugin(_options = {}) {
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
      render(() => <Catalog viewer={viewer} />, root!);
    },
  });
}

console.log("DEADBEEF");
const origo = Origo("index.json", {
  svgSpritePath: "/node_modules/Origo/build/css/svg/",
})!;
origo.on("load", (viewer) => {
  const catalog = CatalogPlugin({});
  viewer.addComponent(catalog);
});
