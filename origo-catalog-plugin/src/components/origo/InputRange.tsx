import type React from "react";
import { useEffect, useState } from "react";
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
  const minValue = props.minValue ?? 0;
  const maxValue = props.maxValue ?? 100;
  const initialValue = props.initialValue ?? (minValue + maxValue) / 2;

  const [value, setValue] = useState(props.value ?? initialValue);

  useEffect(() => {
    if (props.value !== undefined) setValue(props.value);
  }, [props.value]);

  const handleChange = (evt: React.ChangeEvent<HTMLInputElement>) => {
    const newValue = Number(evt.currentTarget.value);
    setValue(newValue);
    props.onChange?.(newValue);
  };

  return (
    <>
      <div className="flex no-wrap text-smaller align-center">
        <input
          type="range"
          min={minValue}
          max={maxValue}
          value={value}
          step={props.step ?? 1}
          className={props.cls}
          style={props.style as React.CSSProperties}
          tabIndex={-99}
          onChange={handleChange}
        />
        <output className="padding-left-small text-align-center">{value}</output>
        <div>&nbsp;{props.unit ?? ""}</div>
      </div>
      <div className="text-smaller text-align-center padding-smallpadding-top-smallest width-full">
        {props.label ?? ""}
      </div>
    </>
  );
};

export default InputRange;
