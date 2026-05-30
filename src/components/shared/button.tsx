import * as React from "react"
import { cn } from "../../lib/utils"
import { useApp } from "../../hooks/useApp"

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "success" | "danger"
  size?: "default" | "sm" | "lg"
  loading?: boolean
  icon?: React.ElementType
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(({ 
  className, 
  variant = "primary",
  size = "default",
  loading = false,
  icon: Icon,
  children,
  disabled,
  ...props 
}, ref) => {
  
  
  const variants = {
    primary: "bg-gradient-to-r from-blue-500 to-purple-600 shadow-lg shadow-blue-500/30 hover:opacity-90 hover:shadow-xl text-white",
    secondary: `bg-secondary text-secondary-foreground border hover:bg-secondary/70 hover:border-primary/40`,
    success: "bg-gradient-to-r from-emerald-500 to-teal-600 shadow-lg shadow-emerald-500/30 hover:opacity-90 hover:shadow-xl text-white",
    danger: "bg-gradient-to-r from-red-500 to-pink-600 shadow-lg shadow-red-500/30 hover:opacity-90 hover:shadow-xl text-white"}
  
  const sizes = {
    default: "px-6 py-2.5 text-sm",
    sm: "px-4 py-2 text-xs",
    lg: "px-8 py-3 text-base"}
  
  return (
    <button
      ref={ref}
      disabled={disabled || loading}
      className={cn(
        "font-medium rounded-xl",
        "disabled:opacity-50 disabled:cursor-not-allowed",
        "flex items-center gap-2 justify-center",
        "transition-all duration-200 cursor-pointer hover:-translate-y-px active:translate-y-0 active:scale-[0.97]",
        variants[variant],
        sizes[size],
        className
      )}
      {...props}
    >
      {loading && (
        <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
      )}
      {Icon && !loading && <Icon size={16} />}
      {children}
    </button>
  )
})
Button.displayName = "Button"

export { Button }
