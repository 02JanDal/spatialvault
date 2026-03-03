import type React from "react";

export interface OrigoInputFileProps {
  labelCls?: string;
  inputCls?: string;
  label?: string;
  onChange?: (evt: React.ChangeEvent<HTMLInputElement>) => void;
}

export const InputFile = (props: OrigoInputFileProps) => {
  return (
    <>
      <label className={props.labelCls}>{props.label}</label>
      <input
        type="file"
        className={props.inputCls}
        onChange={props.onChange}
      />
    </>
  );
};

export default InputFile;
