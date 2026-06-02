import * as React from "react"
import { X } from "lucide-react"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/data-display/badge"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/overlays/command"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/overlays/popover"

function MultiSelect({
  value = [],
  onChange,
  options = [],
  placeholder = "Select items...",
  className = "",
  searchable = true,
  clearable = true,
  ...props
}) {
  const [open, setOpen] = React.useState(false)
  const [search, setSearch] = React.useState("")

  const selectedItems = options.filter(opt => value.includes(opt.value))
  const availableOptions = options.filter(opt => 
    !value.includes(opt.value) && 
    (!search || opt.label.toLowerCase().includes(search.toLowerCase()))
  )

  const handleSelect = (optionValue) => {
    const newValue = value.includes(optionValue)
      ? value.filter(v => v !== optionValue)
      : [...value, optionValue]
    onChange(newValue)
  }

  const handleRemove = (optionValue, e) => {
    e.stopPropagation()
    onChange(value.filter(v => v !== optionValue))
  }

  const handleClear = (e) => {
    e.stopPropagation()
    onChange([])
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            "flex h-auto min-h-8 w-full items-center justify-between rounded-lg border border-input bg-transparent px-2.5 py-1 text-sm transition-colors outline-none hover:bg-accent/5 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50",
            className
          )}
          {...props}
        >
          <div className="flex flex-wrap gap-1 flex-1">
            {selectedItems.length === 0 ? (
              <span className="text-muted-foreground">{placeholder}</span>
            ) : (
              selectedItems.map(item => (
                <Badge
                  key={item.value}
                  className="gap-1 pr-1 rounded-md"
                >
                  <span>{item.label}</span>
                  <button
                    type="button"
                    onClick={(e) => handleRemove(item.value, e)}
                    className="rounded-sm hover:bg-accent/20"
                  >
                    <X size={12} />
                  </button>
                </Badge>
              ))
            )}
          </div>
          {clearable && selectedItems.length > 0 && (
            <button
              type="button"
              onClick={handleClear}
              className="ml-2 rounded-sm hover:bg-accent/20 p-0.5"
            >
              <X size={14} />
            </button>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-[var(--radix-popover-trigger-width)] p-0" align="start">
        <Command>
          {searchable && <CommandInput placeholder="Search..." value={search} onValueChange={setSearch} />}
          <CommandList>
            <CommandEmpty>No results found.</CommandEmpty>
            <CommandGroup>
              {availableOptions.map(option => (
                <CommandItem key={option.value} onSelect={() => handleSelect(option.value)}>
                  {option.label}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

export { MultiSelect }
