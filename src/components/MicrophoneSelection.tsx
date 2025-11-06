"use client"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"
import { Check, ChevronsUpDown, Mic } from "lucide-react"
import * as React from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { toast } from "sonner"

interface MicrophoneSelectionProps {
  value?: string
  onValueChange: (value: string | undefined) => void
  className?: string
}

export function MicrophoneSelection({ value, onValueChange, className }: MicrophoneSelectionProps) {
  const [open, setOpen] = React.useState(false)
  const [devices, setDevices] = React.useState<string[]>([])
  const [loading, setLoading] = React.useState(false)

  // Fetch audio devices on mount
  React.useEffect(() => {
    let unlisten: (() => void) | undefined

    const fetchDevices = async () => {
      try {
        setLoading(true)
        const audioDevices = await invoke<string[]>("get_audio_devices")
        console.log("Fetched audio devices:", audioDevices)
        setDevices(audioDevices)
      } catch (error) {
        console.error("Failed to fetch audio devices:", error)
        toast.error("Failed to load audio devices")
      } finally {
        setLoading(false)
      }
    }

    fetchDevices()

    listen<string[]>("audio-devices-updated", ({ payload }) => {
      console.log("Audio devices updated:", payload)
      setDevices(Array.isArray(payload) ? payload : [])
    })
      .then((dispose) => {
        unlisten = dispose
      })
      .catch((error) => {
        console.warn("Failed to listen for audio device updates:", error)
      })

    return () => {
      unlisten?.()
    }
  }, [])

  // Check if selected device is available when devices list changes
  React.useEffect(() => {
    if (value && devices.length > 0 && !devices.includes(value)) {
      console.log(`Selected device "${value}" is no longer available, resetting to default`)
      toast.info(`${value} is no longer available, switching to default microphone`)
      onValueChange(undefined) // Reset to default
    }
  }, [devices, value, onValueChange])

  const handleDeviceSelect = async (deviceName: string | undefined) => {
    console.log(`Selecting microphone: ${deviceName || 'Default'}`)
    onValueChange(deviceName)
    setOpen(false)
  }

  // Show device name, or indicate if it's unavailable
  const displayValue = React.useMemo(() => {
    if (!value) return "Default"
    if (devices.includes(value)) return value
    return `${value} (Not Available)`
  }, [value, devices])
  
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          className={cn("w-64 justify-between", className)}
          disabled={loading}
        >
          <div className="flex items-center gap-2">
            <Mic className="h-4 w-4" />
            <span className="truncate">{displayValue}</span>
          </div>
          <ChevronsUpDown className="opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-64 p-0">
        <Command>
          <CommandInput placeholder="Search microphone..." className="h-9" />
          <CommandList>
            <CommandEmpty>No microphone found.</CommandEmpty>
            <CommandGroup>
              {/* Default option */}
              <CommandItem
                key="default"
                value="default"
                onSelect={() => handleDeviceSelect(undefined)}
              >
                <Mic className="mr-2 h-4 w-4" />
                Default
                <Check
                  className={cn(
                    "ml-auto",
                    !value ? "opacity-100" : "opacity-0"
                  )}
                />
              </CommandItem>
              {/* Available devices */}
              {devices.map((device) => (
                <CommandItem
                  key={device}
                  value={device}
                  onSelect={() => handleDeviceSelect(device)}
                >
                  <Mic className="mr-2 h-4 w-4" />
                  <span className="truncate">{device}</span>
                  <Check
                    className={cn(
                      "ml-auto",
                      value === device ? "opacity-100" : "opacity-0"
                    )}
                  />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}