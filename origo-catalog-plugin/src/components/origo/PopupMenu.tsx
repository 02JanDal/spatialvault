import type { JSX } from "solid-js";
import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoPopupMenuProps {
  cls?: string;
  style?: StyleProp;
  visible?: boolean;
  defaultVisible?: boolean;
  onUnfocus?: (evt: MouseEvent) => void;
  onVisibilityChange?: (visible: boolean) => void;
  children?: JSX.Element;
}

export const PopupMenu = (props: OrigoPopupMenuProps) => {
  const [visible, setVisible] = createSignal(
    props.visible ?? props.defaultVisible ?? true,
  );
  let popupRef: HTMLDivElement | undefined;

  createEffect(() => {
    if (props.visible !== undefined) setVisible(props.visible);
  });

  const handleWindowClick = (evt: MouseEvent) => {
    if (!popupRef) return;
    if (!popupRef.contains(evt.target as Node)) {
      props.onUnfocus?.(evt);
    }
  };

  onMount(() => window.addEventListener("click", handleWindowClick));
  onCleanup(() => window.removeEventListener("click", handleWindowClick));

  return (
    <div
      ref={popupRef}
      class={joinClasses(
        "popup-menu z-index-ontop-high",
        props.cls,
        visible() ? "" : "hidden",
      )}
      style={props.style as JSX.CSSProperties}
    >
      {props.children}
    </div>
  );
};

export default PopupMenu;
