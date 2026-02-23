import type { JSX } from "solid-js";
import {
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  splitProps,
} from "solid-js";
import { Dynamic } from "solid-js/web";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoCollapseProps {
  cls?: string;
  expanded?: boolean;
  defaultExpanded?: boolean;
  toggleOnClick?: boolean;
  bubble?: boolean;
  collapseX?: boolean;
  collapseY?: boolean;
  contentCls?: string;
  contentStyle?: StyleProp;
  containerCls?: string;
  style?: StyleProp;
  tagName?: keyof JSX.IntrinsicElements;
  mainCls?: string;
  header?: JSX.Element;
  footer?: JSX.Element;
  children: JSX.Element;
  onToggle?: (expanded: boolean) => void;
}

export const Collapse = (props: OrigoCollapseProps) => {
  const [local, rest] = splitProps(props, [
    "expanded",
    "defaultExpanded",
    "toggleOnClick",
    "bubble",
    "collapseX",
    "collapseY",
    "contentCls",
    "contentStyle",
    "containerCls",
    "style",
    "tagName",
    "mainCls",
    "cls",
    "header",
    "footer",
    "children",
    "onToggle",
  ]);

  const collapseX = () => local.collapseX ?? true;
  const collapseY = () => local.collapseY ?? true;
  const [expanded, setExpanded] = createSignal(
    local.expanded ?? local.defaultExpanded ?? false,
  );

  let collapseEl: HTMLElement | undefined;
  let containerEl: HTMLDivElement | undefined;

  createEffect(() => {
    if (local.expanded !== undefined) setExpanded(local.expanded);
  });

  const setTabIndex = (idx: number) => {
    if (!containerEl) return;
    const buttons = containerEl.getElementsByTagName("button");
    for (let i = 0; i < buttons.length; i += 1) {
      const btn = buttons[i];
      if (btn.closest(".collapse-container") === containerEl) {
        btn.tabIndex = idx;
      }
    }
  };

  const onTransitionEnd = () => {
    if (!containerEl) return;
    containerEl.removeEventListener("transitionend", onTransitionEnd);
    if (collapseY()) containerEl.style.height = "";
    if (collapseX()) containerEl.style.width = "";
  };

  const expand = () => {
    if (!collapseEl || !containerEl) return;
    collapseEl.classList.add("expanded");
    const newHeight = containerEl.scrollHeight;
    const newWidth = containerEl.scrollWidth;
    if (collapseY()) containerEl.style.height = `${newHeight}px`;
    if (collapseX()) containerEl.style.width = `${newWidth}px`;
    containerEl.addEventListener("transitionend", onTransitionEnd);
    setTabIndex(0);
  };

  const collapse = () => {
    if (!collapseEl || !containerEl) return;
    collapseEl.classList.remove("expanded");
    const currentHeight = containerEl.scrollHeight;
    const currentWidth = containerEl.scrollWidth;
    const elementTransition = containerEl.style.transition;
    containerEl.style.transition = "";
    setTabIndex(-1);
    requestAnimationFrame(() => {
      const el = containerEl;
      if (!el) return;
      if (collapseY()) el.style.height = `${currentHeight}px`;
      if (collapseX()) el.style.width = `${currentWidth}px`;
      el.style.transition = elementTransition;

      requestAnimationFrame(() => {
        if (!el) return;
        if (collapseY()) el.style.height = "0px";
        if (collapseX()) el.style.width = "0px";
      });
    });
  };

  const toggle = (evt?: Event) => {
    evt?.preventDefault();
    if (!local.bubble) evt?.stopPropagation();
    const next = !expanded();
    setExpanded(next);
    local.onToggle?.(next);
  };

  createEffect(() => {
    if (!containerEl) return;
    if (expanded()) {
      expand();
    } else {
      collapse();
    }
  });

  onMount(() => {
    if (!containerEl) return;
    if (expanded()) {
      setTabIndex(0);
    } else {
      setTabIndex(-1);
      if (collapseY()) containerEl.style.height = "0px";
      if (collapseX()) containerEl.style.width = "0px";
    }
  });

  onCleanup(() => {
    if (containerEl)
      containerEl.removeEventListener("transitionend", onTransitionEnd);
  });

  return (
    <Dynamic
      component={local.tagName ?? "div"}
      ref={(el: HTMLElement) => {
        collapseEl = el;
      }}
      class={joinClasses(
        local.mainCls ?? "collapse",
        local.cls,
        expanded() ? "expanded" : "",
      )}
      style={local.style as JSX.CSSProperties}
      onClick={local.toggleOnClick ? toggle : undefined}
      {...rest}
    >
      {local.header}
      <div
        ref={(el: HTMLDivElement) => {
          containerEl = el;
        }}
        class={joinClasses(
          local.containerCls ?? "collapse-container",
          local.contentCls,
        )}
        style={local.contentStyle as JSX.CSSProperties}
      >
        {local.children}
      </div>
      {local.footer}
    </Dynamic>
  );
};

export default Collapse;
