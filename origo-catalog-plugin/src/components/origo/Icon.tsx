import type { JSX } from "solid-js";
import { Show } from "solid-js";
import { typeOfIcon, type StyleProp } from "./utils";

export interface OrigoIconProps {
  icon?: string;
  cls?: string;
  title?: string;
  style?: StyleProp;
}

export const Icon = (props: OrigoIconProps) => {
  const iconType = () => typeOfIcon(props.icon);

  return (
    <>
      <Show when={iconType() === "image"}>
        <img
          class={props.cls}
          style={props.style as JSX.CSSProperties}
          src={props.icon}
          title={props.title}
          alt={props.title}
        />
      </Show>
      <Show when={iconType() === "sprite"}>
        <svg class={props.cls} style={props.style as JSX.CSSProperties}>
          <Show when={props.title}>
            <title>{props.title}</title>
          </Show>
          <use href={props.icon} />
        </svg>
      </Show>
      <Show when={iconType() === "svg" || iconType() === "img"}>
        {/* Origo allows raw SVG/IMG markup strings */}
        <span
          class={props.cls}
          style={props.style as JSX.CSSProperties}
          innerHTML={props.icon}
        />
      </Show>
    </>
  );
};

export default Icon;
