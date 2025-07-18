import { cn } from '@/lib/utils';

/**
 * Formats a hotkey string into styled keyboard keys
 * e.g., "cmd+shift+space" -> styled keyboard keys
 */
export function formatHotkey(hotkey: string): React.ReactNode {
  if (!hotkey) return null;
  
  // Normalize the hotkey string
  const normalized = hotkey.toLowerCase()
    .replace('commandorcontrol', 'cmd')
    .replace('command', 'cmd')
    .replace('control', 'ctrl')
    .replace('option', 'alt')
    .replace('meta', 'cmd');
  
  // Split by + and filter out empty strings
  const keys = normalized.split('+').filter(Boolean);
  
  // Map common key names to display names
  const keyDisplayMap: Record<string, string> = {
    'cmd': 'cmd',
    'ctrl': 'ctrl',
    'shift': 'shift',
    'alt': 'alt',
    'space': 'space',
    'enter': 'enter',
    'tab': 'tab',
    'escape': 'esc',
    'esc': 'esc',
    'delete': 'del',
    'backspace': '⌫',
    'up': '↑',
    'down': '↓',
    'left': '←',
    'right': '→',
  };
  
  return (
    <span className="inline-flex items-center gap-1">
      {keys.map((key, index) => (
        <span key={index} className="inline-flex items-center gap-1">
          <kbd className={cn(
            "inline-flex items-center justify-center",
            "min-w-[28px] h-7 px-2 rounded",
            "bg-muted text-muted-foreground",
            "text-sm font-medium",
            "border border-b-2 border-muted-foreground/20",
            "shadow-sm"
          )}>
            {keyDisplayMap[key] || key}
          </kbd>
          {index < keys.length - 1 && (
            <span className="text-muted-foreground text-sm">+</span>
          )}
        </span>
      ))}
    </span>
  );
}