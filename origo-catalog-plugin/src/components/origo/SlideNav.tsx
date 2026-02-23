import type { JSX } from "solid-js";
import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import Button from "./Button";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoSlideNavProps {
  backIcon?: string;
  cls?: string;
  style?: StyleProp;
  main: JSX.Element;
  secondary: JSX.Element;
  secondaryTitle?: string;
  secondaryLabelCls?: string;
  initial?: "main" | "secondary";
  onSlide?: (state: "main" | "secondary") => void;
}

export const SlideNav = (props: OrigoSlideNavProps) => {
  const [active, setActive] = createSignal(props.initial ?? "main");
  let slidenavEl: HTMLDivElement | undefined;
  let mainEl: HTMLDivElement | undefined;
  let secondaryEl: HTMLDivElement | undefined;
  const [absoluteMain, setAbsoluteMain] = createSignal(false);
  const [absoluteSecondary, setAbsoluteSecondary] = createSignal(true);

  const slideToMain = () => setActive("main");

  const animateHeight = (currentSlide: HTMLElement, newSlide: HTMLElement) => {
    if (!slidenavEl) return;
    const newHeight = newSlide.scrollHeight;
    const currentHeight = currentSlide.scrollHeight;
    const elementTransition = slidenavEl.style.transition;
    slidenavEl.style.transition = "";

    requestAnimationFrame(() => {
      slidenavEl!.style.height = `${currentHeight}px`;
      slidenavEl!.style.transition = elementTransition;

      requestAnimationFrame(() => {
        slidenavEl!.style.height = `${newHeight}px`;
      });
    });
  };

  const onTransitionEnd = () => {
    if (!slidenavEl || !mainEl || !secondaryEl) return;
    slidenavEl.removeEventListener("transitionend", onTransitionEnd);
    slidenavEl.style.height = "";
    setAbsoluteMain((value) => !value);
    setAbsoluteSecondary((value) => !value);
  };

  const applySecondary = () => {
    if (!slidenavEl || !mainEl || !secondaryEl) return;
    slidenavEl.classList.add("slide-secondary");
    animateHeight(mainEl, secondaryEl);
    slidenavEl.addEventListener("transitionend", onTransitionEnd);
    props.onSlide?.("secondary");
  };

  const applyMain = () => {
    if (!slidenavEl || !mainEl || !secondaryEl) return;
    slidenavEl.classList.remove("slide-secondary");
    animateHeight(secondaryEl, mainEl);
    slidenavEl.addEventListener("transitionend", onTransitionEnd);
    props.onSlide?.("main");
  };

  createEffect(() => {
    if (!slidenavEl || !mainEl || !secondaryEl) return;
    if (active() === "secondary") {
      applySecondary();
    } else {
      applyMain();
    }
  });

  onMount(() => {
    if (active() === "secondary") {
      applySecondary();
    }
  });

  onCleanup(() => {
    if (slidenavEl)
      slidenavEl.removeEventListener("transitionend", onTransitionEnd);
  });

  return (
    <div
      ref={slidenavEl}
      class={joinClasses(props.cls ?? "right", "slidenav")}
      style={props.style as JSX.CSSProperties}
    >
      <div
        ref={mainEl}
        class={joinClasses(
          "main overflow-unset",
          absoluteMain() ? "absolute" : "",
        )}
      >
        {props.main}
      </div>
      <div
        ref={secondaryEl}
        class={joinClasses("secondary", absoluteSecondary() ? "absolute" : "")}
      >
        <div class="flex column">
          <div class="flex row padding-y-small align-center no-grow">
            <Button
              cls="icon-small padding-small"
              icon={props.backIcon ?? "#ic_chevron_left_24px"}
              iconCls="grey"
              tabIndex={-99}
              onClick={() => slideToMain()}
            />
            <div
              class={joinClasses(
                props.secondaryLabelCls,
                "grow pointer no-select",
              )}
              onClick={() => slideToMain()}
            >
              {props.secondaryTitle ?? ""}
            </div>
          </div>
          <div class="divider horizontal"></div>
          {props.secondary}
        </div>
      </div>
    </div>
  );
};

export default SlideNav;
