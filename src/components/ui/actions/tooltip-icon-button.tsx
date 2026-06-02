import * as React from 'react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/overlays/tooltip'
import { cn } from '@/lib/utils'

interface TooltipIconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  tooltip: React.ReactNode;
  tooltipClassName?: string;
  tooltipSide?: React.ComponentProps<typeof TooltipContent>['side'];
  tooltipSideOffset?: React.ComponentProps<typeof TooltipContent>['sideOffset'];
  wrapperClassName?: string;
}

const TooltipIconButton = React.forwardRef<HTMLButtonElement, TooltipIconButtonProps>(
  ({
    tooltip,
    tooltipClassName,
    tooltipSide,
    tooltipSideOffset,
    wrapperClassName,
    className,
    children,
    type = 'button',
    'aria-label': ariaLabel,
    ...props
  }, ref) => (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className={cn('inline-flex', wrapperClassName)}>
          <button
            ref={ref}
            type={type}
            aria-label={ariaLabel || (typeof tooltip === 'string' ? tooltip : undefined)}
            className={className}
            {...props}
          >
            {children}
          </button>
        </span>
      </TooltipTrigger>
      <TooltipContent
        side={tooltipSide}
        sideOffset={tooltipSideOffset}
        className={tooltipClassName}
      >
        {tooltip}
      </TooltipContent>
    </Tooltip>
  )
)

TooltipIconButton.displayName = 'TooltipIconButton'

export { TooltipIconButton }
