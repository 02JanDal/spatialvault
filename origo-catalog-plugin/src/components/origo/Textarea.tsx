import type React from "react";
import { useEffect, useState } from "react";
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
  const [value, setValue] = useState(props.value ?? "");

  useEffect(() => {
    if (props.value !== undefined) setValue(props.value);
  }, [props.value]);

  const handleChange = (evt: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = evt.currentTarget.value;
    setValue(newValue);
    props.onChange?.(newValue);
  };

  return (
    <textarea
      placeholder={props.placeholderText}
      rows={props.rows ?? 3}
      cols={props.cols ?? 30}
      className={props.cls}
      style={props.style as React.CSSProperties}
      value={value}
      onChange={handleChange}
    />
  );
};

export default Textarea;
