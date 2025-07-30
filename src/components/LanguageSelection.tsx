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
import { Check, ChevronsUpDown } from "lucide-react"
import * as React from "react"

export const languages = [
    {
      value: "en",
      label: "English",
    },
    {
      value: "es",
      label: "Spanish",
    },
    {
      value: "zh",
      label: "Chinese",
    },
    {
      value: "hi",
      label: "Hindi",
    },
    {
      value: "ar",
      label: "Arabic",
    },
    {
      value: "fr",
      label: "French",
    },
    {
      value: "pt",
      label: "Portuguese",
    },
    {
      value: "ru",
      label: "Russian",
    },
    {
      value: "ja",
      label: "Japanese",
    },
    {
      value: "de",
      label: "German",
    },
    {
      value: "ko",
      label: "Korean",
    },
    {
      value: "it",
      label: "Italian",
    },
    {
      value: "tr",
      label: "Turkish",
    },
    {
      value: "vi",
      label: "Vietnamese",
    },
    {
      value: "pl",
      label: "Polish",
    },
    {
      value: "uk",
      label: "Ukrainian",
    },
    {
      value: "nl",
      label: "Dutch",
    },
    {
      value: "id",
      label: "Indonesian",
    },
    {
      value: "th",
      label: "Thai",
    },
    {
      value: "sv",
      label: "Swedish",
    },
    {
      value: "af",
      label: "Afrikaans",
    },
    {
      value: "sq",
      label: "Albanian",
    },
    {
      value: "am",
      label: "Amharic",
    },
    {
      value: "hy",
      label: "Armenian",
    },
    {
      value: "as",
      label: "Assamese",
    },
    {
      value: "az",
      label: "Azerbaijani",
    },
    {
      value: "ba",
      label: "Bashkir",
    },
    {
      value: "eu",
      label: "Basque",
    },
    {
      value: "be",
      label: "Belarusian",
    },
    {
      value: "bn",
      label: "Bengali",
    },
    {
      value: "bs",
      label: "Bosnian",
    },
    {
      value: "br",
      label: "Breton",
    },
    {
      value: "bg",
      label: "Bulgarian",
    },
    {
      value: "my",
      label: "Myanmar",
    },
    {
      value: "ca",
      label: "Catalan",
    },
    {
      value: "yue",
      label: "Cantonese",
    },
    {
      value: "hr",
      label: "Croatian",
    },
    {
      value: "cs",
      label: "Czech",
    },
    {
      value: "da",
      label: "Danish",
    },
    {
      value: "et",
      label: "Estonian",
    },
    {
      value: "fo",
      label: "Faroese",
    },
    {
      value: "fi",
      label: "Finnish",
    },
    {
      value: "gl",
      label: "Galician",
    },
    {
      value: "ka",
      label: "Georgian",
    },
    {
      value: "el",
      label: "Greek",
    },
    {
      value: "gu",
      label: "Gujarati",
    },
    {
      value: "ht",
      label: "Haitian Creole",
    },
    {
      value: "ha",
      label: "Hausa",
    },
    {
      value: "haw",
      label: "Hawaiian",
    },
    {
      value: "he",
      label: "Hebrew",
    },
    {
      value: "hu",
      label: "Hungarian",
    },
    {
      value: "is",
      label: "Icelandic",
    },
    {
      value: "jw",
      label: "Javanese",
    },
    {
      value: "kn",
      label: "Kannada",
    },
    {
      value: "kk",
      label: "Kazakh",
    },
    {
      value: "km",
      label: "Khmer",
    },
    {
      value: "lo",
      label: "Lao",
    },
    {
      value: "la",
      label: "Latin",
    },
    {
      value: "lv",
      label: "Latvian",
    },
    {
      value: "ln",
      label: "Lingala",
    },
    {
      value: "lt",
      label: "Lithuanian",
    },
    {
      value: "lb",
      label: "Luxembourgish",
    },
    {
      value: "mk",
      label: "Macedonian",
    },
    {
      value: "mg",
      label: "Malagasy",
    },
    {
      value: "ms",
      label: "Malay",
    },
    {
      value: "ml",
      label: "Malayalam",
    },
    {
      value: "mt",
      label: "Maltese",
    },
    {
      value: "mi",
      label: "Maori",
    },
    {
      value: "mr",
      label: "Marathi",
    },
    {
      value: "mn",
      label: "Mongolian",
    },
    {
      value: "ne",
      label: "Nepali",
    },
    {
      value: "no",
      label: "Norwegian",
    },
    {
      value: "nn",
      label: "Nynorsk",
    },
    {
      value: "oc",
      label: "Occitan",
    },
    {
      value: "ps",
      label: "Pashto",
    },
    {
      value: "fa",
      label: "Persian",
    },
    {
      value: "pa",
      label: "Punjabi",
    },
    {
      value: "ro",
      label: "Romanian",
    },
    {
      value: "sa",
      label: "Sanskrit",
    },
    {
      value: "sr",
      label: "Serbian",
    },
    {
      value: "sn",
      label: "Shona",
    },
    {
      value: "sd",
      label: "Sindhi",
    },
    {
      value: "si",
      label: "Sinhala",
    },
    {
      value: "sk",
      label: "Slovak",
    },
    {
      value: "sl",
      label: "Slovenian",
    },
    {
      value: "so",
      label: "Somali",
    },
    {
      value: "su",
      label: "Sundanese",
    },
    {
      value: "sw",
      label: "Swahili",
    },
    {
      value: "tl",
      label: "Tagalog",
    },
    {
      value: "tg",
      label: "Tajik",
    },
    {
      value: "ta",
      label: "Tamil",
    },
    {
      value: "tt",
      label: "Tatar",
    },
    {
      value: "te",
      label: "Telugu",
    },
    {
      value: "bo",
      label: "Tibetan",
    },
    {
      value: "tk",
      label: "Turkmen",
    },
    {
      value: "ur",
      label: "Urdu",
    },
    {
      value: "uz",
      label: "Uzbek",
    },
    {
      value: "cy",
      label: "Welsh",
    },
    {
      value: "yi",
      label: "Yiddish",
    },
    {
      value: "yo",
      label: "Yoruba",
    },
  ]

interface LanguageSelectionProps {
  value: string
  onValueChange: (value: string) => void
  className?: string
}

export function LanguageSelection({ value, onValueChange, className }: LanguageSelectionProps) {
  const [open, setOpen] = React.useState(false)
  
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          className={cn("w-48 justify-between", className)}
        >
          {value
            ? languages.find((language) => language.value === value)?.label
            : "Select language"}
          <ChevronsUpDown className="opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[200px] p-0">
        <Command>
          <CommandInput placeholder="Search language..." className="h-9" />
          <CommandList>
            <CommandEmpty>No language found.</CommandEmpty>
            <CommandGroup>
              {languages.map((language) => (
                <CommandItem
                  key={language.value}
                  value={language.value}
                  onSelect={(currentValue) => {
                    onValueChange(currentValue)
                    setOpen(false)
                  }}
                >
                  {language.label}
                  <Check
                    className={cn(
                      "ml-auto",
                      value === language.value ? "opacity-100" : "opacity-0"
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
