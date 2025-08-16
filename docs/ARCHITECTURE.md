# VoiceTypr Frontend Architecture

## Overview

VoiceTypr's frontend architecture follows a **component composition pattern** with clear separation of concerns and event-driven communication between components. Components are loaded directly for instant desktop app experience.

## Component Structure

```
src/
├── App.tsx                          # Root component (22 lines) - Only providers
├── components/
│   ├── AppContainer.tsx             # Main orchestration (179 lines)
│   ├── tabs/                        # Self-contained feature tabs
│   │   ├── TabContainer.tsx         # Tab routing component
│   │   ├── RecordingsTab.tsx        # Transcription history
│   │   ├── ModelsTab.tsx            # Model management
│   │   ├── SettingsTab.tsx          # General settings
│   │   ├── EnhancementsTab.tsx      # AI features
│   │   ├── AdvancedTab.tsx          # Advanced settings
│   │   └── AccountTab.tsx           # License & account
│   └── sections/                    # Presentational components
│       ├── RecentRecordings.tsx
│       ├── ModelsSection.tsx
│       ├── GeneralSettings.tsx
│       └── ...
├── contexts/                        # Global state providers
│   ├── SettingsContext.tsx
│   ├── LicenseContext.tsx
│   └── ReadinessContext.tsx
└── hooks/                           # Shared business logic
    ├── useEventCoordinator.ts
    ├── useModelManagement.ts
    └── useRecording.ts
```

## Architecture Layers

### 1. **Provider Layer (App.tsx)**
The root component only manages context providers and error boundaries:
- `AppErrorBoundary` - Catches and handles React errors
- `LicenseProvider` - License management
- `SettingsProvider` - Global settings state
- `ReadinessProvider` - Permission checks

### 2. **Orchestration Layer (AppContainer.tsx)**
Handles app-level concerns:
- Onboarding flow management
- Global event listeners
- App initialization (API keys, update service, cleanup)
- Navigation state management

### 3. **Routing Layer (TabContainer.tsx)**
Implements direct tab routing for instant switching:
- Direct component imports (no lazy loading)
- Instant tab transitions
- Section-to-component mapping

### 4. **Feature Layer (Tab Components)**
Self-contained feature modules:
- **RecordingsTab**: Manages transcription history and recording events
- **ModelsTab**: Handles model downloads and selection
- **SettingsTab**: General app settings and hotkey configuration
- **EnhancementsTab**: AI enhancement features and API keys
- **AdvancedTab**: Advanced configuration options
- **AccountTab**: License management and about info

### 5. **Presentation Layer (Section Components)**
Pure presentational components that receive props and emit events:
- No direct state management
- No business logic
- Only UI rendering and user interactions

## State Management

### Global State (Contexts)
```typescript
// Settings - App configuration
SettingsContext → useSettings()

// License - License validation
LicenseContext → useLicense()

// Readiness - Permission states
ReadinessContext → useReadiness()
```

### Local State (Tabs)
Each tab manages its own local state:
```typescript
// RecordingsTab
const [history, setHistory] = useState<TranscriptionHistory[]>([]);

// Other tabs use hooks for state
const modelManagement = useModelManagement();
```

### Event-Driven Updates
Components communicate via Tauri events:
```typescript
// Register event listener
registerEvent("history-updated", () => {
  loadHistory();
});

// Emit event from backend
emit("history-updated", {});
```

## Event Architecture

### Event Flow
```
Backend (Rust) → Tauri Events → Frontend Event Coordinator → Tab Components
```

### Event Categories

1. **Recording Events** (RecordingsTab)
   - `history-updated` - Reload transcription history
   - `recording-error` - Handle recording failures
   - `transcription-error` - Handle transcription failures

2. **Model Events** (ModelsTab)
   - `download-retry` - Download retry notifications
   - `model-downloaded` - Model download completion

3. **Navigation Events** (AppContainer)
   - `navigate-to-settings` - Open settings from tray
   - `no-models-available` - Redirect to onboarding

4. **Error Events** (Distributed)
   - `no-speech-detected` - Audio validation failures
   - `hotkey-registration-failed` - Hotkey conflicts
   - `ai-enhancement-error` - AI processing errors

## Performance Optimizations

### Lazy Loading
All tab components are directly imported for instant desktop experience:
```typescript
import { RecordingsTab } from './tabs/RecordingsTab';
import { ModelsTab } from './tabs/ModelsTab';
// ... etc
```

### Code Splitting
Each tab is a separate chunk:
- `RecordingsTab.chunk.js`
- `ModelsTab.chunk.js`
- Only loaded when user navigates to that tab

### React Forget Compiler
The app uses React Forget compiler for automatic memoization:
- No manual `useCallback` or `useMemo` needed
- Components automatically optimized
- Prevents unnecessary re-renders

## Adding New Features

### Adding a New Tab

1. Create the tab component:
```typescript
// src/components/tabs/NewFeatureTab.tsx
export function NewFeatureTab() {
  // Tab-specific state and logic
  const [data, setData] = useState();
  
  // Register event handlers
  useEffect(() => {
    registerEvent("feature-event", handleEvent);
  }, []);
  
  return <FeatureSection {...props} />;
}
```

2. Add to TabContainer:
```typescript
// src/components/tabs/TabContainer.tsx
import { NewFeatureTab } from './NewFeatureTab';

// In switch statement
case 'newfeature':
  return <NewFeatureTab />;
```

3. Add navigation option:
```typescript
// src/components/Sidebar.tsx
<SidebarMenuItem value="newfeature">
  New Feature
</SidebarMenuItem>
```

### Adding Event Handlers

1. Register in appropriate tab:
```typescript
useEffect(() => {
  registerEvent<PayloadType>("event-name", (data) => {
    // Handle event
  });
}, [registerEvent]);
```

2. Emit from backend:
```rust
app_handle.emit("event-name", payload)?;
```

## Testing Strategy

### Component Testing
Each tab has its own test file:
- `RecordingsTab.test.tsx` - History management tests
- `ModelsTab.test.tsx` - Model operations tests
- `TabContainer.test.tsx` - Routing and tab switching tests

### Integration Testing
Test complete user flows:
- Tab navigation
- Event handling
- State updates

### Mocking Strategy
```typescript
// Mock contexts
vi.mock('@/contexts/SettingsContext');

// Mock hooks  
vi.mock('@/hooks/useModelManagement');

// Mock Tauri API
vi.mock('@tauri-apps/api/core');
```

## Best Practices

### Component Guidelines
1. **Single Responsibility**: Each component has one clear purpose
2. **Props Over State**: Prefer props for data flow
3. **Events for Communication**: Use events for cross-component updates
4. **Error Boundaries**: Wrap risky operations

### State Management
1. **Lift Appropriately**: Global state in contexts, local state in components
2. **Derive When Possible**: Calculate values instead of storing
3. **Event Consistency**: Use typed events with clear payloads

### Performance
1. **Direct Import Routes**: All tabs are directly imported for instant switching
2. **Virtualize Long Lists**: Use react-window for large datasets
3. **Batch Updates**: Group related state changes

## Troubleshooting

### Common Issues

**Tab not rendering:**
- Check TabContainer switch statement
- Verify import path
- Check for console errors

**Event not firing:**
- Verify event name matches backend
- Check event registration in useEffect
- Ensure cleanup on unmount

**State not updating:**
- Check context provider wrapping
- Verify hook usage
- Look for stale closures

## Migration from Monolithic Architecture

The refactoring from a 524-line monolithic App.tsx achieved:
- **96% reduction** in main component complexity
- **Better testability** with isolated components
- **Instant tab switching** with direct imports
- **Enhanced maintainability** with clear boundaries

Each piece of functionality was carefully extracted to maintain 100% feature parity while improving the architecture.