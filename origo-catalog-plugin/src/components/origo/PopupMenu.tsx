import type React from "react";
import { useEffect, useRef, useState } from "react";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoPopupMenuProps {
  cls?: string;
  style?: StyleProp;
  visible?: boolean;
  defaultVisible?: boolean;
  onUnfocus?: (evt: MouseEvent) => void;
  onVisibilityChange?: (visible: boolean) => void;
  children?: React.ReactNode;
}

export const PopupMenu = (props: OrigoPopupMenuProps) => {
  const [visible, setVisible] = useState(
    props.visible ?? props.defaultVisible ?? true,
  );
  const popupRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (props.visible !== undefined) setVisible(props.visible);
  }, [props.visible]);

  useEffect(() => {
    const handleWindowClick = (evt: MouseEvent) => {
      if (!popupRef.current) return;
      if (!popupRef.current.contains(evt.target as Node)) {
        props.onUnfocus?.(evt);
      }
    };

    window.addEventListener("click", handleWindowClick);
    return () => window.removeEventListener("click", handleWindowClick);
  }, [props.onUnfocus]);

  return (
    <div
      ref={popupRef}
      className={joinClasses(
        "popup-menu z-index-ontop-high",
        props.cls,
        visible ? "" : "hidden",
      )}
      style={props.style as React.CSSProperties}
    >
      {props.children}
    </div>
  );
};

export default PopupMenu;
