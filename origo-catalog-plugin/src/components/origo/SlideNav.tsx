import type React from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import Button from "./Button";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoSlideNavProps {
  backIcon?: string;
  cls?: string;
  style?: StyleProp;
  main: React.ReactNode;
  secondary: React.ReactNode;
  secondaryTitle?: string;
  secondaryLabelCls?: string;
  initial?: "main" | "secondary";
  onSlide?: (state: "main" | "secondary") => void;
}

export const SlideNav = (props: OrigoSlideNavProps) => {
  const [active, setActive] = useState(props.initial ?? "main");
  const slidenavRef = useRef<HTMLDivElement>(null);
  const mainRef = useRef<HTMLDivElement>(null);
  const secondaryRef = useRef<HTMLDivElement>(null);
  const [absoluteMain, setAbsoluteMain] = useState(false);
  const [absoluteSecondary, setAbsoluteSecondary] = useState(true);

  const slideToMain = () => setActive("main");

  const onTransitionEnd = useCallback(() => {
    const slidenavEl = slidenavRef.current;
    if (!slidenavEl || !mainRef.current || !secondaryRef.current) return;
    slidenavEl.removeEventListener("transitionend", onTransitionEnd);
    slidenavEl.style.height = "";
    setAbsoluteMain((value) => !value);
    setAbsoluteSecondary((value) => !value);
  }, []);

  const animateHeight = useCallback(
    (currentSlide: HTMLElement, newSlide: HTMLElement) => {
      const slidenavEl = slidenavRef.current;
      if (!slidenavEl) return;
      const newHeight = newSlide.scrollHeight;
      const currentHeight = currentSlide.scrollHeight;
      const elementTransition = slidenavEl.style.transition;
      slidenavEl.style.transition = "";

      requestAnimationFrame(() => {
        slidenavEl.style.height = `${currentHeight}px`;
        slidenavEl.style.transition = elementTransition;

        requestAnimationFrame(() => {
          slidenavEl.style.height = `${newHeight}px`;
        });
      });
    },
    [],
  );

  const applySecondary = useCallback(() => {
    const slidenavEl = slidenavRef.current;
    const mainEl = mainRef.current;
    const secondaryEl = secondaryRef.current;
    if (!slidenavEl || !mainEl || !secondaryEl) return;
    slidenavEl.classList.add("slide-secondary");
    animateHeight(mainEl, secondaryEl);
    slidenavEl.addEventListener("transitionend", onTransitionEnd);
    props.onSlide?.("secondary");
  }, [animateHeight, onTransitionEnd, props.onSlide]);

  const applyMain = useCallback(() => {
    const slidenavEl = slidenavRef.current;
    const mainEl = mainRef.current;
    const secondaryEl = secondaryRef.current;
    if (!slidenavEl || !mainEl || !secondaryEl) return;
    slidenavEl.classList.remove("slide-secondary");
    animateHeight(secondaryEl, mainEl);
    slidenavEl.addEventListener("transitionend", onTransitionEnd);
    props.onSlide?.("main");
  }, [animateHeight, onTransitionEnd, props.onSlide]);

  useEffect(() => {
    if (!slidenavRef.current || !mainRef.current || !secondaryRef.current)
      return;
    if (active === "secondary") {
      applySecondary();
    } else {
      applyMain();
    }
  }, [active, applySecondary, applyMain]);

  useEffect(() => {
    if (active === "secondary") {
      applySecondary();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const slidenavEl = slidenavRef.current;
    return () => {
      if (slidenavEl)
        slidenavEl.removeEventListener("transitionend", onTransitionEnd);
    };
  }, [onTransitionEnd]);

  return (
    <div
      ref={slidenavRef}
      className={joinClasses(props.cls ?? "right", "slidenav")}
      style={props.style as React.CSSProperties}
    >
      <div
        ref={mainRef}
        className={joinClasses(
          "main overflow-unset",
          absoluteMain ? "absolute" : "",
        )}
      >
        {props.main}
      </div>
      <div
        ref={secondaryRef}
        className={joinClasses("secondary", absoluteSecondary ? "absolute" : "")}
      >
        <div className="flex column">
          <div className="flex row padding-y-small align-center no-grow">
            <Button
              cls="icon-small padding-small"
              icon={props.backIcon ?? "#ic_chevron_left_24px"}
              iconCls="grey"
              tabIndex={-99}
              onClick={() => slideToMain()}
            />
            <div
              className={joinClasses(
                props.secondaryLabelCls,
                "grow pointer no-select",
              )}
              onClick={() => slideToMain()}
            >
              {props.secondaryTitle ?? ""}
            </div>
          </div>
          <div className="divider horizontal"></div>
          {props.secondary}
        </div>
      </div>
    </div>
  );
};

export default SlideNav;
