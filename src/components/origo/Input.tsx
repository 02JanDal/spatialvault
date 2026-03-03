import type React from "react";
import { useEffect, useState } from "react";
import { type StyleProp } from "./utils";

export interface OrigoInputProps {
  id?: string;
  cls?: string;
  placeholder?: string;
  value?: string;
  style?: StyleProp;
  onChange?: (value: string) => void;
  onFocusOut?: (value: string) => void;
}

export const Input = (props: OrigoInputProps) => {
  const [value, setValue] = useState(props.value ?? "");

  useEffect(() => {
    if (props.value !== undefined) setValue(props.value);
  }, [props.value]);

  const handleChange = (evt: React.ChangeEvent<HTMLInputElement>) => {
    const newValue = evt.currentTarget.value;
    setValue(newValue);
    props.onChange?.(newValue);
  };

  const handleBlur = (evt: React.FocusEvent<HTMLInputElement>) => {
    const newValue = evt.currentTarget.value;
    setValue(newValue);
    props.onFocusOut?.(newValue);
  };

  return (
    <input
      id={props.id}
      type="text"
      placeholder={props.placeholder}
      className={props.cls}
      value={value}
      style={props.style as React.CSSProperties}
      onChange={handleChange}
      onBlur={handleBlur}
    />
  );
};

export default Input;
