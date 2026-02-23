import type { JSX } from "solid-js";
import { createEffect, createSignal } from "solid-js";
import { type StyleProp } from "./utils";

export interface OrigoInputProps {
  cls?: string;
  placeholderText?: string;
  value?: string;
  style?: StyleProp;
  onChange?: (value: string) => void;
  onFocusOut?: (value: string) => void;
}

export const Input = (props: OrigoInputProps) => {
  const [value, setValue] = createSignal(props.value ?? "");

  createEffect(() => {
    if (props.value !== undefined) setValue(props.value);
  });

  const handleInput: JSX.EventHandlerUnion<HTMLInputElement, InputEvent> = (
    evt,
  ) => {
    const newValue = evt.currentTarget.value;
    setValue(newValue);
    props.onChange?.(newValue);
  };

  const handleBlur: JSX.EventHandlerUnion<HTMLInputElement, FocusEvent> = (
    evt,
  ) => {
    const newValue = evt.currentTarget.value;
    setValue(newValue);
    props.onFocusOut?.(newValue);
  };

  return (
    <input
      type="text"
      placeholder={props.placeholderText}
      class={props.cls}
      value={value()}
      style={props.style as JSX.CSSProperties}
      onInput={handleInput}
      onBlur={handleBlur}
    />
  );
};

export default Input;
