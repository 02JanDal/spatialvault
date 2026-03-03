import type React from "react";
import { typeOfIcon, type StyleProp } from "./utils";

export interface OrigoIconProps {
  icon?: string;
  cls?: string;
  title?: string;
  style?: StyleProp;
}

export const Icon = (props: OrigoIconProps) => {
  const iconType = typeOfIcon(props.icon);

  return (
    <>
      {iconType === "image" && (
        <img
          className={props.cls}
          style={props.style as React.CSSProperties}
          src={props.icon}
          title={props.title}
          alt={props.title}
        />
      )}
      {iconType === "sprite" && (
        <svg className={props.cls} style={props.style as React.CSSProperties}>
          {props.title && <title>{props.title}</title>}
          <use href={props.icon} />
        </svg>
      )}
      {(iconType === "svg" || iconType === "img") && (
        <span
          className={props.cls}
          style={props.style as React.CSSProperties}
          dangerouslySetInnerHTML={{ __html: props.icon! }}
        />
      )}
    </>
  );
};

export default Icon;
