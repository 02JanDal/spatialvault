import type React from "react";
import { useEffect, useMemo, useState } from "react";
import Button from "./Button";
import Collapse from "./Collapse";
import { joinClasses, type StyleProp } from "./utils";

export interface DropdownItem {
  label: string;
  value: string;
}

export interface OrigoDropdownProps {
  id?: string;
  cls?: string;
  containerCls?: string;
  contentCls?: string;
  contentStyle?: React.CSSProperties;
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
  const normalizedItems = useMemo(
    () =>
      (props.items ?? []).map((item) =>
        typeof item === "object" && "label" in item && "value" in item
          ? item
          : { label: String(item), value: String(item) },
      ),
    [props.items],
  );

  const [expanded, setExpanded] = useState(false);
  const [text, setText] = useState(props.text ?? " ");

  useEffect(() => {
    if (props.text !== undefined) setText(props.text);
  }, [props.text]);

  const toggle = () => setExpanded((prev) => !prev);

  const selectItem = (item: DropdownItem) => {
    if (props.updateTextOnSelect ?? true) {
      setText(item.label);
    }
    props.onSelect?.(item);
    setExpanded(false);
  };

  const direction = props.direction ?? "down";
  const position = direction === "down" ? "top" : "bottom";

  const header =
    direction === "down" ? (
      <div className={joinClasses(props.buttonContainerCls, "collapse-header")}>
        <Button
          text={text}
          cls={props.buttonCls ?? "padding-small rounded light box-shadow"}
          style={{ padding: "0 .5rem", overflow: "hidden" }}
          icon={`#ic_arrow_drop_${direction}_24px`}
          iconCls={joinClasses(props.buttonIconCls, "icon-smaller flex")}
          ariaLabel={props.ariaLabel ?? ""}
          textCls={props.buttonTextCls ?? "flex"}
          onClick={toggle}
        />
      </div>
    ) : null;

  const footer =
    direction === "up" ? (
      <div className={joinClasses(props.buttonContainerCls, "collapse-header")}>
        <Button
          text={text}
          cls={props.buttonCls ?? "padding-small rounded light box-shadow"}
          style={{ padding: "0 .5rem", overflow: "hidden" }}
          icon={`#ic_arrow_drop_${direction}_24px`}
          iconCls={joinClasses(props.buttonIconCls, "icon-smaller flex")}
          ariaLabel={props.ariaLabel ?? ""}
          textCls={props.buttonTextCls ?? "flex"}
          onClick={toggle}
        />
      </div>
    ) : null;

  return (
    <div
      className={joinClasses(props.cls, "relative")}
      style={props.style as React.CSSProperties}
    >
      <Collapse
        cls="dropdown"
        containerCls={props.containerCls ?? "collapse-container"}
        contentCls={props.contentCls ?? "bg-white"}
        contentStyle={{ [position]: "calc(100% + 2px)", ...props.contentStyle }}
        collapseX={false}
        header={header}
        footer={footer}
        expanded={expanded}
      >
        <ul>
          {normalizedItems.map((item) => (
            <li key={item.value} onClick={() => selectItem(item)}>
              <span>{item.label}</span>
            </li>
          ))}
        </ul>
      </Collapse>
    </div>
  );
};

export default Dropdown;
