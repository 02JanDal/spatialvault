import type { JSX } from "solid-js";

export type StyleProp = JSX.CSSProperties | string | undefined;

export type IconType = "sprite" | "svg" | "img" | "image" | "";

export const typeOfIcon = (src?: string): IconType => {
  if (!src || src.length === 0) return "";
  if (src.startsWith("#")) return "sprite";
  if (src.startsWith("<svg")) return "svg";
  if (src.startsWith("<img")) return "img";
  return "image";
};

export const joinClasses = (...values: Array<string | undefined | false>) =>
  values.filter(Boolean).join(" ");

export const makeElementDraggable = (el: HTMLElement) => {
  const touchMode = "ontouchstart" in document.documentElement;
  const draggableEl =
    (el.getElementsByClassName("draggable")[0] as HTMLElement | undefined) ||
    el;
  let pos1 = 0;
  let pos2 = 0;
  let pos3 = 0;
  let pos4 = 0;

  const elementDrag = (evt: MouseEvent | TouchEvent) => {
    const e = evt as MouseEvent & TouchEvent;
    e.preventDefault();
    const clientX = e.clientX === undefined ? e.touches[0].clientX : e.clientX;
    const clientY = e.clientY === undefined ? e.touches[0].clientY : e.clientY;
    pos1 = pos3 - clientX;
    pos2 = pos4 - clientY;
    pos3 = clientX;
    pos4 = clientY;
    el.style.top = `${el.offsetTop - pos2}px`;
    el.style.left = `${el.offsetLeft - pos1}px`;
  };

  const closeDragElement = () => {
    draggableEl.classList.toggle("grabbing");
    if (touchMode) {
      draggableEl.ontouchend = null;
      draggableEl.ontouchmove = null;
    } else {
      document.onmouseup = null;
      document.onmousemove = null;
    }
  };

  const dragMouseDown = (evt: MouseEvent | TouchEvent) => {
    const e = evt as MouseEvent & TouchEvent;
    draggableEl.classList.toggle("grabbing");
    pos3 = e.clientX ?? e.touches[0].clientX;
    pos4 = e.clientY ?? e.touches[0].clientY;

    if (touchMode) {
      draggableEl.ontouchend = closeDragElement;
      draggableEl.ontouchmove = elementDrag;
    } else {
      document.onmouseup = closeDragElement;
      document.onmousemove = elementDrag;
    }
  };

  if (touchMode) {
    draggableEl.ontouchstart = dragMouseDown;
  } else {
    draggableEl.onmousedown = dragMouseDown;
  }

  return () => {
    if (touchMode) {
      draggableEl.ontouchstart = null;
      draggableEl.ontouchend = null;
      draggableEl.ontouchmove = null;
    } else {
      draggableEl.onmousedown = null;
      document.onmouseup = null;
      document.onmousemove = null;
    }
  };
};
