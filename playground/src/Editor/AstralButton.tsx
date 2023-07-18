import type { ButtonHTMLAttributes } from "react";
import classNames from "classnames";

export default function AstralButton({
  className,
  children,
  ...otherProps
}: ButtonHTMLAttributes<any>) {
  return (
    <button
      className={classNames(
        "bg-radiate",
        "text-black",
        "hover:text-radiate",
        "hover:bg-galaxy",
        "outline-1",
        "dark:outline",
        "dark:hover:outline-radiate",
        "rounded-md",
        "text-sm",
        "font-semibold",
        "enabled:hover:bg-galaxy",
        className,
      )}
      {...otherProps}
    >
      {children}
    </button>
  );
}
