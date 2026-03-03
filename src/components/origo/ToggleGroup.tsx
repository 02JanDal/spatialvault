import type React from "react";
import { createElement } from "react";
import Button, { type ButtonState } from "./Button";
import { joinClasses, type StyleProp } from "./utils";

export interface ToggleGroupItem {
  id: string;
  label?: string;
  icon?: string;
  value: string;
  buttonCls?: string;
  iconCls?: string;
  textCls?: string;
  state?: ButtonState;
}

export interface OrigoToggleGroupProps {
  cls?: string;
  style?: StyleProp;
  tagName?: keyof React.JSX.IntrinsicElements;
  items: ToggleGroupItem[];
  value?: string;
  onChange?: (value: string) => void;
}

export const ToggleGroup = (props: OrigoToggleGroupProps) => {
  const activeValue = props.value ?? "";

  const handleClick = (item: ToggleGroupItem) => {
    if (item.value !== activeValue) props.onChange?.(item.value);
  };

  const Tag = props.tagName ?? "div";

  return (
    <div>
      {createElement(
        Tag,
        {
          className: joinClasses(props.cls, "toggle-group"),
          style: props.style as React.CSSProperties,
        },
        props.items.map((item) => (
          <Button
            key={item.id}
            cls={item.buttonCls}
            text={item.label}
            icon={item.icon}
            iconCls={item.iconCls}
            textCls={item.textCls}
            state={item.value === activeValue ? "active" : "initial"}
            onClick={() => handleClick(item)}
          />
        )),
      )}
    </div>
  );
};

export default ToggleGroup;
