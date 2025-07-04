import { useEffect, useRef, useState } from 'react';
import { Input } from './ui/input';
import { Badge } from './ui/badge';
import { Keyboard } from 'lucide-react';

interface HotkeyInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

export function HotkeyInput({ value, onChange, placeholder }: HotkeyInputProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [keys, setKeys] = useState<Set<string>>(new Set());
  const inputRef = useRef<HTMLInputElement>(null);
  const isEscapePressed = useRef(false);

  useEffect(() => {
    if (!isRecording) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      
      const key = e.key;
      
      // Handle Escape to cancel
      if (key === 'Escape') {
        isEscapePressed.current = true;
        setIsRecording(false);
        setKeys(new Set());
        inputRef.current?.blur();
        return;
      }
      
      const newKeys = new Set(keys);
      
      // Add modifier keys
      if (e.metaKey || e.ctrlKey) newKeys.add('CommandOrControl');
      if (e.shiftKey) newKeys.add('Shift');
      if (e.altKey) newKeys.add('Alt');
      
      // Add the actual key (capitalize it)
      if (!['Control', 'Shift', 'Alt', 'Meta'].includes(key)) {
        newKeys.add(key.length === 1 ? key.toUpperCase() : key);
      }
      
      setKeys(newKeys);
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      
      if (keys.size > 0) {
        // Format the shortcut
        const modifiers: string[] = [];
        const regularKeys: string[] = [];
        
        keys.forEach(key => {
          if (['CommandOrControl', 'Shift', 'Alt'].includes(key)) {
            modifiers.push(key);
          } else {
            regularKeys.push(key);
          }
        });
        
        // Standard order: CommandOrControl+Alt+Shift+Key
        const orderedModifiers = ['CommandOrControl', 'Alt', 'Shift'].filter(mod => modifiers.includes(mod));
        const shortcut = [...orderedModifiers, ...regularKeys].join('+');
        
        onChange(shortcut);
        setIsRecording(false);
        setKeys(new Set());
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);

    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [isRecording, keys, onChange]);

  const formatShortcutDisplay = (shortcut: string) => {
    const isMac = navigator.userAgent.toUpperCase().indexOf('MAC') >= 0;
    return shortcut
      .replace('CommandOrControl', isMac ? '⌘' : 'Ctrl')
      .replace('Shift', '⇧')
      .replace('Alt', isMac ? '⌥' : 'Alt')
      .replace('Plus', '+');
  };

  return (
    <div className="space-y-2">
      <div className="relative">
        <Input
          ref={inputRef}
          value={isRecording ? 'Press your hotkey combination...' : formatShortcutDisplay(value)}
          onClick={() => {
            setIsRecording(true);
            // Keep focus on the input
            inputRef.current?.focus();
          }}
          onBlur={() => {
            // Reset the recording state when focus is lost
            // but don't interfere if Escape was pressed
            if (!isEscapePressed.current) {
              setIsRecording(false);
              setKeys(new Set());
            }
            isEscapePressed.current = false;
          }}
          readOnly
          className="cursor-pointer pr-10"
          placeholder={placeholder || "Click to set hotkey"}
        />
        <Keyboard className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
      </div>
      {isRecording && (
        <div className="flex items-center gap-2">
          <Badge variant="secondary" className="animate-pulse">Recording...</Badge>
          <span className="text-xs text-muted-foreground">Press Esc to cancel</span>
        </div>
      )}
    </div>
  );
}