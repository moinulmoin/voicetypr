# State Management & Event Architecture

## Overview

VoiceTypr uses a hybrid approach combining React Context for global state, local component state for feature-specific data, and Tauri events for cross-component communication.

## State Management Architecture

### State Hierarchy

```
┌─────────────────────────────────────┐
│         Global State                 │
│    (React Context Providers)         │
│  • SettingsContext                   │
│  • LicenseContext                    │
│  • ReadinessContext                  │
└────────────┬────────────────────────┘
             │
┌────────────▼────────────────────────┐
│      Component State                 │
│    (useState in Tabs)                │
│  • RecordingsTab: history[]          │
│  • AppContainer: activeSection       │
│  • AppContainer: showOnboarding      │
└────────────┬────────────────────────┘
             │
┌────────────▼────────────────────────┐
│       Hook State                     │
│   (Custom Hook Management)           │
│  • useModelManagement                │
│  • useRecording                      │
│  • useEventCoordinator               │
└─────────────────────────────────────┘
```

### Global State (Context Providers)

#### SettingsContext
**Purpose:** Manages application settings  
**Location:** `/src/contexts/SettingsContext.tsx`  
**Data:**
```typescript
interface AppSettings {
  hotkey: string;
  current_model: string;
  onboarding_completed: boolean;
  transcription_cleanup_days: number;
  // ... other settings
}
```
**Usage:**
```typescript
const { settings, updateSettings, refreshSettings } = useSettings();
```

#### LicenseContext
**Purpose:** Manages license validation and status  
**Location:** `/src/contexts/LicenseContext.tsx`  
**Data:**
```typescript
interface LicenseInfo {
  valid: boolean;
  key?: string;
  expiresAt?: Date;
}
```
**Usage:**
```typescript
const { license, validateLicense } = useLicense();
```

#### ReadinessContext
**Purpose:** Tracks app permissions and readiness state  
**Location:** `/src/contexts/ReadinessContext.tsx`  
**Data:**
```typescript
interface ReadinessState {
  hasMicrophonePermission: boolean;
  hasAccessibilityPermission: boolean;
  isReady: boolean;
}
```
**Usage:**
```typescript
const { checkMicrophonePermission, checkAccessibilityPermission } = useReadiness();
```

### Local Component State

#### RecordingsTab
```typescript
// Manages transcription history locally
const [history, setHistory] = useState<TranscriptionHistory[]>([]);

// Loads from backend
const loadHistory = async () => {
  const data = await invoke("get_transcription_history");
  setHistory(data);
};
```

#### AppContainer
```typescript
// Navigation state
const [activeSection, setActiveSection] = useState("recordings");

// Onboarding state
const [showOnboarding, setShowOnboarding] = useState(false);
```

### Hook-Managed State

#### useModelManagement
Centralized model operations:
```typescript
const {
  models,              // Available models
  downloadProgress,    // Download percentages
  verifyingModels,     // Models being verified
  downloadModel,       // Action: start download
  deleteModel,         // Action: delete model
  cancelDownload       // Action: cancel download
} = useModelManagement();
```

#### useRecording
Recording state machine:
```typescript
const {
  state,              // 'idle' | 'recording' | 'processing'
  audioLevel,         // Current audio level
  startRecording,     // Action: start
  stopRecording,      // Action: stop
  cancelRecording     // Action: cancel
} = useRecording();
```

## Event Architecture

### Event Flow Diagram

```
┌──────────────┐     Tauri IPC      ┌──────────────┐
│              │◄───────────────────►│              │
│   Backend    │                     │   Frontend   │
│   (Rust)     │     Events          │   (React)    │
│              │────────────────────►│              │
└──────────────┘                     └──────┬───────┘
                                            │
                                   ┌────────▼────────┐
                                   │ EventCoordinator│
                                   │   (Hook)        │
                                   └────────┬────────┘
                                            │
                           ┌────────────────┼────────────────┐
                           │                │                │
                    ┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼──────┐
                    │RecordingsTab│ │  ModelsTab  │ │SettingsTab │
                    └─────────────┘ └─────────────┘ └────────────┘
```

### Event Categories

#### 1. Recording Events
**Source:** Backend audio/transcription system  
**Handled by:** RecordingsTab, RecordingPill

| Event | Payload | Purpose |
|-------|---------|---------|
| `recording-started` | `{}` | Recording begun |
| `recording-stopped` | `{}` | Recording ended |
| `audio-level` | `{level: number}` | Audio level updates |
| `transcription-complete` | `{text: string}` | Transcription ready |
| `history-updated` | `{}` | History changed |
| `recording-error` | `string` | Recording failed |
| `transcription-error` | `string` | Transcription failed |

#### 2. Model Events
**Source:** Model download system  
**Handled by:** ModelsTab, useModelManagement

| Event | Payload | Purpose |
|-------|---------|---------|
| `download-progress` | `{model, progress}` | Download updates |
| `model-verifying` | `{model}` | Verification started |
| `model-downloaded` | `{model}` | Download complete |
| `download-cancelled` | `string` | Download cancelled |
| `download-retry` | `{model, attempt}` | Retry attempt |

#### 3. Navigation Events
**Source:** System tray, shortcuts  
**Handled by:** AppContainer

| Event | Payload | Purpose |
|-------|---------|---------|
| `navigate-to-settings` | `{}` | Open settings |
| `window-focus` | `{}` | Focus window |
| `no-models-available` | `{}` | Redirect to onboarding |

#### 4. Error Events
**Source:** Various backend systems  
**Handled by:** Distributed across tabs

| Event | Payload | Purpose |
|-------|---------|---------|
| `no-speech-detected` | `ErrorEventPayload` | No speech in recording |
| `transcription-empty` | `string` | Empty transcription |
| `hotkey-registration-failed` | `ErrorEventPayload` | Hotkey conflict |
| `no-models-error` | `ErrorEventPayload` | No models available |
| `ai-enhancement-error` | `string` | AI processing failed |
| `license-required` | `{message}` | License validation failed |

### Event Registration Pattern

Components register for events in `useEffect`:

```typescript
function RecordingsTab() {
  const { registerEvent } = useEventCoordinator("main");
  
  useEffect(() => {
    // Register event with cleanup
    const unsubscribe = registerEvent("history-updated", async () => {
      await loadHistory();
    });
    
    // Cleanup on unmount
    return () => {
      unsubscribe();
    };
  }, [registerEvent]);
}
```

### Event Coordination

The `useEventCoordinator` hook manages event lifecycle:

```typescript
export function useEventCoordinator(windowId: string) {
  // Prevents duplicate registrations
  const registeredEvents = useRef(new Set<string>());
  
  // Manages cleanup
  const cleanup = useRef<(() => void)[]>([]);
  
  // Registration with deduplication
  const registerEvent = (event: string, handler: Function) => {
    if (registeredEvents.current.has(event)) {
      return; // Already registered
    }
    
    const unsubscribe = listen(event, handler);
    cleanup.current.push(unsubscribe);
    registeredEvents.current.add(event);
    
    return unsubscribe;
  };
  
  // Cleanup on unmount
  useEffect(() => {
    return () => {
      cleanup.current.forEach(fn => fn());
    };
  }, []);
  
  return { registerEvent };
}
```

## State Update Patterns

### Direct State Updates
For simple, synchronous updates:
```typescript
const [count, setCount] = useState(0);
setCount(prev => prev + 1);
```

### Async State Updates
For backend data fetching:
```typescript
const loadData = async () => {
  try {
    const data = await invoke("get_data");
    setData(data);
  } catch (error) {
    console.error("Failed to load:", error);
  }
};
```

### Event-Driven Updates
For cross-component updates:
```typescript
// Component A emits
await invoke("save_setting", { setting: value });
// Backend emits event
emit("settings-changed", {});

// Component B listens
registerEvent("settings-changed", () => {
  refreshSettings();
});
```

### Context Updates
For global state changes:
```typescript
const { updateSettings } = useSettings();
await updateSettings({ hotkey: "Cmd+Shift+X" });
// All components using useSettings() re-render
```

## State Synchronization

### Backend → Frontend
1. Backend state changes
2. Backend emits Tauri event
3. Frontend components listening update their state

### Frontend → Backend
1. User interaction in component
2. Component calls Tauri command
3. Backend updates its state
4. Backend emits confirmation event
5. Other components update if listening

### Cross-Component
1. Component A updates shared context
2. Context provider re-renders
3. All consuming components re-render with new state

## Best Practices

### State Management
1. **Keep state local when possible** - Only lift when needed
2. **Use contexts for truly global state** - Settings, user, license
3. **Derive state when possible** - Calculate instead of store
4. **Batch related updates** - Reduce re-renders

### Event Handling
1. **Register once** - Use useEventCoordinator deduplication
2. **Clean up listeners** - Prevent memory leaks
3. **Type event payloads** - Use TypeScript interfaces
4. **Handle errors gracefully** - Try-catch in handlers

### Performance
1. **Lazy load heavy components** - Use React.lazy()
2. **Debounce frequent events** - Audio level, progress updates
3. **Use React Forget** - Automatic memoization
4. **Virtualize long lists** - Use react-window

## Debugging

### State Debugging
```typescript
// Log state changes
useEffect(() => {
  console.log("State changed:", state);
}, [state]);

// React DevTools
// Install React Developer Tools extension
```

### Event Debugging
```typescript
// Log all events
registerEvent("*", (event) => {
  console.log("Event:", event);
});

// Tauri DevTools
// Use Tauri's built-in logging
```

### Common Issues

**State not updating:**
- Check if using stale closure
- Verify event is being emitted
- Ensure component is mounted

**Events not firing:**
- Verify event name matches
- Check registration timing
- Look for error in handler

**Memory leaks:**
- Missing cleanup in useEffect
- Event listeners not removed
- Circular references in closures

## Testing

### Testing State
```typescript
it('updates state on event', async () => {
  render(<Component />);
  
  // Trigger event
  const callback = window.__testEventCallbacks['event-name'];
  await callback({ data: 'test' });
  
  // Verify state updated
  expect(screen.getByText('test')).toBeInTheDocument();
});
```

### Mocking Events
```typescript
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: (event, handler) => {
      window.__testEventCallbacks[event] = handler;
      return vi.fn(); // cleanup
    }
  })
}));
```

### Testing Async State
```typescript
it('loads data async', async () => {
  vi.mocked(invoke).mockResolvedValueOnce(mockData);
  
  render(<Component />);
  
  await waitFor(() => {
    expect(screen.getByText(mockData.text)).toBeInTheDocument();
  });
});
```