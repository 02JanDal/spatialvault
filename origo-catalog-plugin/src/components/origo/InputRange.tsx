import type { JSX } from "solid-js";
import { createEffect, createSignal } from "solid-js";
import { type StyleProp } from "./utils";

export interface OrigoInputRangeProps {
  cls?: string;
  minValue?: number;
  maxValue?: number;
  value?: number;
  initialValue?: number;
  step?: number;
  style?: StyleProp;
  unit?: string;
  label?: string;
  onChange?: (value: number) => void;
}

export const InputRange = (props: OrigoInputRangeProps) => {
  const minValue = () => props.minValue ?? 0;
  const maxValue = () => props.maxValue ?? 100;
  const initialValue = () =>
    props.initialValue ?? (minValue() + maxValue()) / 2;

  const [value, setValue] = createSignal(props.value ?? initialValue());

  createEffect(() => {
    if (props.value !== undefined) setValue(props.value);
  });

  const handleInput: JSX.EventHandlerUnion<HTMLInputElement, InputEvent> = (
    evt,
  ) => {
    const newValue = Number(evt.currentTarget.value);
    setValue(newValue);
    props.onChange?.(newValue);
  };

  const handleChange: JSX.EventHandlerUnion<HTMLInputElement, Event> = (
    evt,
  ) => {
    const newValue = Number(evt.currentTarget.value);
    setValue(newValue);
    props.onChange?.(newValue);
  };

  return (
    <>
      <div class="flex no-wrap text-smaller align-center">
        <input
          type="range"
          min={minValue()}
          max={maxValue()}
          value={value()}
          step={props.step ?? 1}
          class={props.cls}
          style={props.style as JSX.CSSProperties}
          tabindex={-99}
          onInput={handleInput}
          onChange={handleChange}
        />
        <output class="padding-left-small text-align-center">{value()}</output>
        <div>&nbsp;{props.unit ?? ""}</div>
      </div>
      <div class="text-smaller text-align-center padding-smallpadding-top-smallest width-full">
        {props.label ?? ""}
      </div>
    </>
  );
};

export default InputRange;
