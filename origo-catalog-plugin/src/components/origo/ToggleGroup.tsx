import type { JSX } from "solid-js";
import { For } from "solid-js";
import { Dynamic } from "solid-js/web";
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
  tagName?: keyof JSX.IntrinsicElements;
  items: ToggleGroupItem[];
  value?: string;
  onChange?: (value: string) => void;
}

export const ToggleGroup = (props: OrigoToggleGroupProps) => {
  const activeValue = () => props.value ?? "";

  const handleClick = (item: ToggleGroupItem) => {
    if (item.value !== activeValue()) props.onChange?.(item.value);
  };

  return (
    <div>
      <Dynamic
        component={props.tagName ?? "div"}
        class={joinClasses(props.cls, "toggle-group")}
        style={props.style as JSX.CSSProperties}
      >
        <For each={props.items}>
          {(item) => (
            <Button
              cls={item.buttonCls}
              text={item.label}
              icon={item.icon}
              iconCls={item.iconCls}
              textCls={item.textCls}
              state={item.value === activeValue() ? "active" : "initial"}
              onClick={() => handleClick(item)}
            />
          )}
        </For>
      </Dynamic>
    </div>
  );
};

export default ToggleGroup;
