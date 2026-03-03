import type React from "react";
import { createElement } from "react";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoElementProps {
  tagName?: keyof React.JSX.IntrinsicElements;
  cls?: string;
  style?: StyleProp;
  attributes?: Record<string, string>;
  innerHTML?: string;
  children?: React.ReactNode;
}

export const Element = (props: OrigoElementProps) => {
  const tag = props.tagName ?? "div";
  const baseProps: Record<string, unknown> = {
    className: joinClasses(props.cls),
    style: props.style as React.CSSProperties,
    ...(props.attributes ?? {}),
  };

  if (props.innerHTML) {
    baseProps.dangerouslySetInnerHTML = { __html: props.innerHTML };
    return createElement(tag, baseProps);
  }

  return createElement(tag, baseProps, props.children);
};

export default Element;
