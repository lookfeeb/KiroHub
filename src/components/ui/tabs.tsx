"use client"
import * as React from "react"
import { Tabs as TabsPrimitive } from "@base-ui/react/tabs"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"

function Tabs({
  className,
  orientation = "horizontal",
  ...props
}: { className?: string; orientation?: string; [key: string]: any }) {
  return (
    <TabsPrimitive.Root
      data-slot="tabs"
      data-orientation={orientation}
      className={cn(
        "group/tabs flex gap-2",
        orientation === "horizontal" ? "flex-col" : "flex-row",
        className
      )}
      {...props}
    />
  )
}

const tabsListVariants = cva(
  "group/tabs-list inline-flex items-center justify-center rounded-lg p-[3px] text-muted-foreground",
  {
    variants: {
      variant: {
        default: "bg-muted",
        line: "gap-1 bg-transparent"}},
    defaultVariants: {
      variant: "default"}}
)

function TabsList({
  className,
  variant = "default",
  ...props
}: { className?: string; variant?: "default" | "line"; [key: string]: any }) {
  return (
    <TabsPrimitive.List
      data-slot="tabs-list"
      data-variant={variant}
      className={cn(tabsListVariants({ variant }), className)}
      {...props}
    />
  )
}

function TabsTrigger({ className, value, ...props }: { className?: string; value: any; [key: string]: any }) {
  return (
    <TabsPrimitive.Tab
      data-slot="tabs-trigger"
      value={value}
      className={cn(
        "relative inline-flex h-9 appearance-none outline-none items-center justify-center gap-1.5 rounded-md border border-transparent px-3 py-1 text-sm font-medium whitespace-nowrap text-foreground/60 transition-all cursor-pointer hover:text-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-50 disabled:cursor-not-allowed aria-disabled:pointer-events-none aria-disabled:opacity-50 data-active:bg-primary data-active:text-primary-foreground data-active:shadow-sm [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    />
  )
}

function TabsContent({ className, value, ...props }: { className?: string; value: any; [key: string]: any }) {
  return (
    <TabsPrimitive.Panel
      data-slot="tabs-content"
      value={value}
      className={cn("flex-1 text-sm outline-none w-full mt-2", className)}
      {...props}
    />
  )
}

export { Tabs, TabsList, TabsTrigger, TabsContent, tabsListVariants }
