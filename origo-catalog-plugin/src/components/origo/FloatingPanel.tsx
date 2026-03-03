import type React from "react";
import { useEffect, useRef } from "react";
import Button from "./Button";
import { joinClasses, makeElementDraggable, type StyleProp } from "./utils";

export interface OrigoFloatingPanelProps {
  title?: string;
  cls?: string;
  type?: "floating" | "left";
  isActive?: boolean;
  closeIcon?: string;
  removeOnClose?: boolean;
  style?: StyleProp;
  onClose?: () => void;
  onHide?: () => void;
  onShow?: () => void;
  children?: React.ReactNode;
}

export const FloatingPanel = (props: OrigoFloatingPanelProps) => {
  const panelRef = useRef<HTMLDivElement>(null);

  const panelClass = (() => {
    const base =
      "absolute flex column control bg-white overflow-hidden z-index-top no-select";
    const faded = props.isActive === false ? "faded" : "";
    const left = props.type === "left" ? "top-left no-margin height-full" : "";
    return joinClasses(base, faded, left, props.cls);
  })();

  const panelStyle = (() => {
    if (props.type === "left") return props.style;
    return (
      props.style ?? "top: 4rem; left: 4rem; max-height: calc(100% - (6rem))"
    );
  })();

  const headerClass = joinClasses(
    "flex row justify-end grey-lightest",
    props.type === "left" ? "" : "draggable move",
  );

  useEffect(() => {
    const panelEl = panelRef.current;
    if (!panelEl) return;
    if (props.isActive === false) {
      panelEl.classList.add("faded");
      props.onHide?.();
    } else {
      panelEl.classList.remove("faded");
      props.onShow?.();
    }
  }, [props.isActive]);

  useEffect(() => {
    const panelEl = panelRef.current;
    if (panelEl && props.type !== "left") {
      const cleanup = makeElementDraggable(panelEl);
      return cleanup;
    }
  }, [props.type]);

  return (
    <div
      ref={panelRef}
      className={panelClass}
      style={panelStyle as React.CSSProperties}
    >
      <div className={headerClass} style={{ width: "100%" }}>
        <div
          className="flex row justify-start margin-top-small margin-left text-weight-bold padding-right"
          style={{ width: "100%" }}
        >
          {props.title ?? "Inforuta"}
        </div>
        <Button
          cls="small round margin-top-small margin-right-small icon-smaller grey-lightest o-tooltip"
          ariaLabel="Stäng"
          icon={props.closeIcon ?? "#ic_close_24px"}
          onClick={() => props.onClose?.()}
        />
      </div>
      <div className="padding-y-small overflow-auto text-small">
        {props.children}
      </div>
    </div>
  );
};

export default FloatingPanel;
