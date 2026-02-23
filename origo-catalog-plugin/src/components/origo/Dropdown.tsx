import type { JSX } from "solid-js";
import { createEffect, createMemo, createSignal, For } from "solid-js";
import Button from "./Button";
import Collapse from "./Collapse";
import { joinClasses, type StyleProp } from "./utils";

export interface DropdownItem {
  label: string;
  value: string;
}

export interface OrigoDropdownProps {
  cls?: string;
  containerCls?: string;
  contentCls?: string;
  contentStyle?: string;
  buttonCls?: string;
  buttonIconCls?: string;
  buttonContainerCls?: string;
  buttonTextCls?: string;
  style?: StyleProp;
  direction?: "down" | "up";
  ariaLabel?: string;
  text?: string;
  items?: Array<DropdownItem | string>;
  updateTextOnSelect?: boolean;
  onSelect?: (item: DropdownItem) => void;
}

export const Dropdown = (props: OrigoDropdownProps) => {
  const normalizedItems = createMemo(() =>
    (props.items ?? []).map((item) =>
      typeof item === "object" && "label" in item && "value" in item
        ? item
        : { label: String(item), value: String(item) },
    ),
  );

  const [expanded, setExpanded] = createSignal(false);
  const [text, setText] = createSignal(props.text ?? " ");

  createEffect(() => {
    if (props.text !== undefined) setText(props.text);
  });

  const toggle = () => setExpanded((prev) => !prev);

  const selectItem = (item: DropdownItem) => {
    if (props.updateTextOnSelect ?? true) {
      setText(item.label);
    }
    props.onSelect?.(item);
    setExpanded(false);
  };

  const position = () =>
    (props.direction ?? "down") === "down" ? "top" : "bottom";
  const header = () =>
    (props.direction ?? "down") === "down" ? (
      <div class={joinClasses(props.buttonContainerCls, "collapse-header")}>
        <Button
          text={text()}
          cls={props.buttonCls ?? "padding-small rounded light box-shadow"}
          style={{ padding: "0 .5rem", overflow: "hidden" }}
          icon={`#ic_arrow_drop_${props.direction ?? "down"}_24px`}
          iconCls={joinClasses(props.buttonIconCls, "icon-smaller flex")}
          ariaLabel={props.ariaLabel ?? ""}
          textCls={props.buttonTextCls ?? "flex"}
          onClick={toggle}
        />
      </div>
    ) : null;

  const footer = () =>
    (props.direction ?? "down") === "up" ? (
      <div class={joinClasses(props.buttonContainerCls, "collapse-header")}>
        <Button
          text={text()}
          cls={props.buttonCls ?? "padding-small rounded light box-shadow"}
          style={{ padding: "0 .5rem", overflow: "hidden" }}
          icon={`#ic_arrow_drop_${props.direction ?? "up"}_24px`}
          iconCls={joinClasses(props.buttonIconCls, "icon-smaller flex")}
          ariaLabel={props.ariaLabel ?? ""}
          textCls={props.buttonTextCls ?? "flex"}
          onClick={toggle}
        />
      </div>
    ) : null;

  return (
    <div
      class={joinClasses(props.cls, "relative")}
      style={props.style as JSX.CSSProperties}
    >
      <Collapse
        cls="dropdown"
        containerCls={props.containerCls ?? "collapse-container"}
        contentCls={props.contentCls ?? "bg-white"}
        contentStyle={`${position()}:calc(100% + 2px);${props.contentStyle ?? ""}`}
        collapseX={false}
        header={header()}
        footer={footer()}
        expanded={expanded()}
      >
        <ul>
          <For each={normalizedItems()}>
            {(item) => (
              <li onClick={() => selectItem(item)}>
                <span>{item.label}</span>
              </li>
            )}
          </For>
        </ul>
      </Collapse>
    </div>
  );
};

export default Dropdown;
