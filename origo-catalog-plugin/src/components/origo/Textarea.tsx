import type { JSX } from "solid-js";
import { createEffect, createSignal } from "solid-js";
import { type StyleProp } from "./utils";

export interface OrigoTextareaProps {
  cls?: string;
  placeholderText?: string;
  rows?: number;
  cols?: number;
  value?: string;
  style?: StyleProp;
  onChange?: (value: string) => void;
}

export const Textarea = (props: OrigoTextareaProps) => {
  const [value, setValue] = createSignal(props.value ?? "");

  createEffect(() => {
    if (props.value !== undefined) setValue(props.value);
  });

  const handleInput: JSX.EventHandlerUnion<HTMLTextAreaElement, InputEvent> = (
    evt,
  ) => {
    const newValue = evt.currentTarget.value;
    setValue(newValue);
    props.onChange?.(newValue);
  };

  return (
    <textarea
      placeholder={props.placeholderText}
      rows={props.rows ?? 3}
      cols={props.cols ?? 30}
      class={props.cls}
      style={props.style as JSX.CSSProperties}
      value={value()}
      onInput={handleInput}
    />
  );
};

export default Textarea;
