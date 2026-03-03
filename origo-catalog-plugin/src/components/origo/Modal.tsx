import type React from "react";
import Button from "./Button";
import { joinClasses, type StyleProp } from "./utils";

export interface OrigoModalProps {
  title?: string;
  cls?: string;
  contentCls?: string;
  style?: StyleProp;
  closeIcon?: string;
  isStatic?: boolean;
  newTabUrl?: string;
  onClose?: () => void;
  onShow?: () => void;
  onHide?: () => void;
  visible?: boolean;
  children?: React.ReactNode;
}

export const Modal = (props: OrigoModalProps) => {
  const isVisible = props.visible ?? true;

  return (
    <div className={joinClasses(props.cls, "flex", isVisible ? "" : "o-hidden")}>
      <div
        className="o-modal-screen"
        onClick={() => !props.isStatic && props.onClose?.()}
      />
      <div
        className={joinClasses("o-modal", props.contentCls)}
        style={props.style as React.CSSProperties}
      >
        <div className="flex row justify-end grey-lightest draggable">
          <div
            className="flex row justify-start margin-y-smaller margin-left text-weight-bold"
            style={{ width: "100%" }}
          >
            {props.title ?? ""}
          </div>
          {props.newTabUrl && (
            <Button
              cls="small round margin-top-smaller margin-bottom-auto margin-right icon-smaller grey-lightest no-shrink"
              icon="#ic_launch_24px"
              onClick={() => window.open(props.newTabUrl)}
            />
          )}
          <Button
            cls="small round margin-top-smaller margin-bottom-auto margin-right icon-smaller grey-lightest no-shrink"
            icon={props.closeIcon ?? "#ic_close_24px"}
            ariaLabel="Stäng"
            validStates={["initial", "hidden"]}
            onClick={() => props.onClose?.()}
          />
        </div>
        <div className="o-modal-content">{props.children}</div>
      </div>
    </div>
  );
};

export default Modal;
