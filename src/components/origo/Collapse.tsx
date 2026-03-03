import type React from "react";
import { useCallback, useEffect, useRef, useState } from "react";
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
  tagName?: keyof React.JSX.IntrinsicElements;
  mainCls?: string;
  header?: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
  onToggle?: (expanded: boolean) => void;
}

export const Collapse = (props: OrigoCollapseProps) => {
  const doCollapseX = props.collapseX ?? true;
  const doCollapseY = props.collapseY ?? true;

  const [expanded, setExpanded] = useState(
    props.expanded ?? props.defaultExpanded ?? false,
  );

  const collapseRef = useRef<HTMLElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (props.expanded !== undefined) setExpanded(props.expanded);
  }, [props.expanded]);

  const setTabIndex = useCallback(
    (idx: number) => {
      const containerEl = containerRef.current;
      if (!containerEl) return;
      const buttons = containerEl.getElementsByTagName("button");
      for (let i = 0; i < buttons.length; i += 1) {
        const btn = buttons[i];
        if (btn.closest(".collapse-container") === containerEl) {
          btn.tabIndex = idx;
        }
      }
    },
    [],
  );

  const onTransitionEnd = useCallback(() => {
    const containerEl = containerRef.current;
    if (!containerEl) return;
    containerEl.removeEventListener("transitionend", onTransitionEnd);
    if (doCollapseY) containerEl.style.height = "";
    if (doCollapseX) containerEl.style.width = "";
  }, [doCollapseX, doCollapseY]);

  const expand = useCallback(() => {
    const collapseEl = collapseRef.current;
    const containerEl = containerRef.current;
    if (!collapseEl || !containerEl) return;
    collapseEl.classList.add("expanded");
    const newHeight = containerEl.scrollHeight;
    const newWidth = containerEl.scrollWidth;
    if (doCollapseY) containerEl.style.height = `${newHeight}px`;
    if (doCollapseX) containerEl.style.width = `${newWidth}px`;
    containerEl.addEventListener("transitionend", onTransitionEnd);
    setTabIndex(0);
  }, [doCollapseX, doCollapseY, onTransitionEnd, setTabIndex]);

  const collapse = useCallback(() => {
    const collapseEl = collapseRef.current;
    const containerEl = containerRef.current;
    if (!collapseEl || !containerEl) return;
    collapseEl.classList.remove("expanded");
    const currentHeight = containerEl.scrollHeight;
    const currentWidth = containerEl.scrollWidth;
    const elementTransition = containerEl.style.transition;
    containerEl.style.transition = "";
    setTabIndex(-1);
    requestAnimationFrame(() => {
      if (!containerEl) return;
      if (doCollapseY) containerEl.style.height = `${currentHeight}px`;
      if (doCollapseX) containerEl.style.width = `${currentWidth}px`;
      containerEl.style.transition = elementTransition;

      requestAnimationFrame(() => {
        if (!containerEl) return;
        if (doCollapseY) containerEl.style.height = "0px";
        if (doCollapseX) containerEl.style.width = "0px";
      });
    });
  }, [doCollapseX, doCollapseY, setTabIndex]);

  const toggle = (evt?: React.MouseEvent) => {
    evt?.preventDefault();
    if (!props.bubble) evt?.stopPropagation();
    const next = !expanded;
    setExpanded(next);
    props.onToggle?.(next);
  };

  // Expand/collapse effect
  useEffect(() => {
    const containerEl = containerRef.current;
    if (!containerEl) return;
    if (expanded) {
      expand();
    } else {
      collapse();
    }
  }, [expanded, expand, collapse]);

  // Initial mount setup
  useEffect(() => {
    const containerEl = containerRef.current;
    if (!containerEl) return;
    if (expanded) {
      setTabIndex(0);
    } else {
      setTabIndex(-1);
      if (doCollapseY) containerEl.style.height = "0px";
      if (doCollapseX) containerEl.style.width = "0px";
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Cleanup
  useEffect(() => {
    const containerEl = containerRef.current;
    return () => {
      if (containerEl)
        containerEl.removeEventListener("transitionend", onTransitionEnd);
    };
  }, [onTransitionEnd]);

  const Tag = (props.tagName ?? "div") as "div";

  return (
    <Tag
      ref={(el) => { collapseRef.current = el; }}
      className={joinClasses(
        props.mainCls ?? "collapse",
        props.cls,
        expanded ? "expanded" : "",
      )}
      style={props.style as React.CSSProperties}
      onClick={props.toggleOnClick ? toggle : undefined}
    >
      {props.header}
      <div
        ref={(el) => { containerRef.current = el; }}
        className={joinClasses(
          props.containerCls ?? "collapse-container",
          props.contentCls,
        )}
        style={props.contentStyle as React.CSSProperties}
      >
        {props.children}
      </div>
      {props.footer}
    </Tag>
  );
};

export default Collapse;
