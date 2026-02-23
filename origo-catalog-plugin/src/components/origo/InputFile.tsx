import type { JSX } from "solid-js";

export interface OrigoInputFileProps {
  labelCls?: string;
  inputCls?: string;
  label?: string;
  onChange?: (
    evt: Event & { currentTarget: HTMLInputElement; target: Element },
  ) => void;
}

export const InputFile = (props: OrigoInputFileProps) => {
  const handleChange: JSX.EventHandlerUnion<HTMLInputElement, Event> = (
    evt,
  ) => {
    props.onChange?.(
      evt as Event & { currentTarget: HTMLInputElement; target: Element },
    );
  };

  return (
    <>
      <label class={props.labelCls}>{props.label}</label>
      <input type="file" class={props.inputCls} onChange={handleChange} />
    </>
  );
};

export default InputFile;
