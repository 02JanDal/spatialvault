import type { JSX } from "solid-js";
import Button from "./Button";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoCollapseHeaderProps {
  cls?: string;
  icon?: string;
  title?: string;
  style?: StyleProp;
  onToggle?: () => void;
}

export const CollapseHeader = (props: OrigoCollapseHeaderProps) => {
  return (
    <div
      class={joinClasses(
        props.cls,
        "flex row align-center pointer collapse-header",
      )}
      style={props.style as JSX.CSSProperties}
      onClick={() => props.onToggle?.()}
    >
      <span class="grow  basis-0">{props.title ?? "Title"}</span>
      <Button
        cls="icon-small compact round"
        icon={props.icon ?? "#ic_chevron_right_24px"}
        iconCls="rotate grey"
        style={{ "align-self": "center" }}
      />
    </div>
  );
};

export default CollapseHeader;
