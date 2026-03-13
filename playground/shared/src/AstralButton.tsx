import { forwardRef, type ButtonHTMLAttributes } from "react";
import classNames from "classnames";

const AstralButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement>
>(({ className, children, ...otherProps }, ref) => {
  return (
    <button
      ref={ref}
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
});

export default AstralButton;
