import type React from "react";
import Icon from "./Icon";
import { joinClasses, type StyleProp } from "./utils";

export type ButtonState =
  | "initial"
  | "active"
  | "disabled"
  | "inactive"
  | "loading"
  | "tracking"
  | "hidden";

export interface OrigoButtonProps {
  icon?: string;
  text?: string;
  cls?: string;
  iconCls?: string;
  iconStyle?: StyleProp;
  textCls?: string;
  tooltipText?: string;
  tooltipPlacement?: string;
  title?: string;
  ariaLabel?: string;
  tabIndex?: number;
  state?: ButtonState;
  validStates?: ButtonState[];
  style?: StyleProp;
  onClick?: (evt: React.MouseEvent<HTMLButtonElement>) => void;
  onMouseEnter?: (evt: React.MouseEvent<HTMLButtonElement>) => void;
}

export const Button = (props: OrigoButtonProps) => {
  const validStates =
    props.validStates ?? [
      "initial",
      "active",
      "disabled",
      "inactive",
      "loading",
      "tracking",
    ];
  const stateClass =
    props.state &&
    props.state !== "initial" &&
    validStates.includes(props.state)
      ? props.state
      : "";
  const ariaLabel =
    props.ariaLabel ?? props.tooltipText ?? props.title ?? "";

  return (
    <button
      className={joinClasses(props.cls, "o-tooltip", stateClass)}
      style={props.style as React.CSSProperties}
      aria-label={ariaLabel}
      tabIndex={props.tabIndex ?? 0}
      onClick={(evt) => {
        evt.preventDefault();
        props.onClick?.(evt);
      }}
      onMouseEnter={(evt) => {
        evt.preventDefault();
        props.onMouseEnter?.(evt);
      }}
    >
      {props.icon && props.text && (
        <span className="flex row align-center justify-space-between">
          <span className={joinClasses(props.textCls, "margin-right-small")}>
            {props.text}
          </span>
          <span className={joinClasses("icon", props.iconCls)}>
            <Icon
              icon={props.icon}
              cls={props.iconCls}
              style={props.iconStyle}
              title={props.title}
            />
          </span>
        </span>
      )}
      {props.icon && !props.text && (
        <span className={joinClasses("icon", props.iconCls)}>
          <Icon
            icon={props.icon}
            cls={props.iconCls}
            style={props.iconStyle}
            title={props.title}
          />
        </span>
      )}
      {!props.icon && (
        <span className={props.textCls}>{props.text}</span>
      )}
      {props.tooltipText && (
        <span
          data-tooltip={props.tooltipText}
          data-placement={props.tooltipPlacement ?? "east"}
        />
      )}
    </button>
  );
};

export default Button;
