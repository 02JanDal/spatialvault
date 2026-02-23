import type { JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoElementProps {
  tagName?: keyof JSX.IntrinsicElements;
  cls?: string;
  style?: StyleProp;
  attributes?: Record<string, string>;
  innerHTML?: string;
  children?: JSX.Element;
}

export const Element = (props: OrigoElementProps) => {
  return (
    <Dynamic
      component={props.tagName ?? "div"}
      class={joinClasses(props.cls)}
      style={props.style as JSX.CSSProperties}
      {...(props.attributes ?? {})}
      innerHTML={props.innerHTML}
    >
      {props.innerHTML ? null : props.children}
    </Dynamic>
  );
};

export default Element;
