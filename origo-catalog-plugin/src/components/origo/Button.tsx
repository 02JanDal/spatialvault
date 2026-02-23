import type { JSX } from "solid-js";
import { Show } from "solid-js";
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
  onClick?: (evt: MouseEvent) => void;
  onMouseEnter?: (evt: MouseEvent) => void;
}

export const Button = (props: OrigoButtonProps) => {
  const validStates = () =>
    props.validStates ?? [
      "initial",
      "active",
      "disabled",
      "inactive",
      "loading",
      "tracking",
    ];
  const stateClass = () =>
    props.state &&
    props.state !== "initial" &&
    validStates().includes(props.state)
      ? props.state
      : "";
  const ariaLabel = () =>
    props.ariaLabel ?? props.tooltipText ?? props.title ?? "";

  return (
    <button
      class={joinClasses(props.cls, "o-tooltip", stateClass())}
      style={props.style as JSX.CSSProperties}
      aria-label={ariaLabel()}
      tabindex={props.tabIndex ?? 0}
      onClick={(evt) => {
        evt.preventDefault();
        props.onClick?.(evt);
      }}
      onMouseEnter={(evt) => {
        evt.preventDefault();
        props.onMouseEnter?.(evt);
      }}
    >
      <Show when={props.icon && props.text}>
        <span class="flex row align-center justify-space-between">
          <span class={joinClasses(props.textCls, "margin-right-small")}>
            {props.text}
          </span>
          <span class={joinClasses("icon", props.iconCls)}>
            <Icon
              icon={props.icon}
              cls={props.iconCls}
              style={props.iconStyle}
              title={props.title}
            />
          </span>
        </span>
      </Show>
      <Show when={props.icon && !props.text}>
        <span class={joinClasses("icon", props.iconCls)}>
          <Icon
            icon={props.icon}
            cls={props.iconCls}
            style={props.iconStyle}
            title={props.title}
          />
        </span>
      </Show>
      <Show when={!props.icon}>
        <span class={props.textCls}>{props.text}</span>
      </Show>
      <Show when={props.tooltipText}>
        <span
          data-tooltip={props.tooltipText}
          data-placement={props.tooltipPlacement ?? "east"}
        />
      </Show>
    </button>
  );
};

export default Button;
