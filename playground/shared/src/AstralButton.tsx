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
        "uppercase",
        "ease-in-out",
        "font-heading",
        "outline-radiate",
        "transition-all duration-200",
        "bg-radiate",
        "text-black",
        "hover:text-white",
        "hover:bg-galaxy",
        "cursor-pointer",
        "outline-1",
        "dark:outline",
        "dark:hover:outline-white",
        "rounded-md",
        "tracking-[.08em]",
        "text-sm",
        "font-medium",
        "enabled:hover:bg-galaxy",
        className,
      )}
      {...otherProps}
    >
      {children}
    </button>
  );
}
