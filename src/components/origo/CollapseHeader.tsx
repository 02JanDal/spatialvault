import type React from "react";
import Button from "./Button";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoCollapseHeaderProps {
  cls?: string;
  icon?: string;
  title?: string;
  style?: StyleProp;
  onToggle?: () => void;
  hasChildren: boolean;
}

export const CollapseHeader = (props: OrigoCollapseHeaderProps) => {
  return (
    <div
      className={joinClasses(
        props.cls,
        "flex row align-center pointer collapse-header",
      )}
      style={props.style as React.CSSProperties}
      onClick={() => props.onToggle?.()}
    >
      <span className="grow  basis-0">{props.title ?? "Title"}</span>
      {props.hasChildren && (
        <Button
          cls="icon-small compact round"
          icon={props.icon ?? "#ic_chevron_right_24px"}
          iconCls="rotate grey"
          style={{ "alignSelf": "center" }}
        />
      )}
    </div>
  );
};

export default CollapseHeader;
