# Toast Window Positioning - Detailed Explanation

## The Problem

Toast and pill windows must appear correctly positioned relative to each other, and both must stay on-screen across different monitor sizes. This is trickier than it sounds because:

1. **Two independent windows** - They're created separately but positioned together
2. **Screen coordinates** - X/Y positions change based on monitor DPI, multiple monitors, etc.
3. **Centered layout** - Both windows center horizontally at bottom of screen
4. **No overlap** - Toast sits above pill with a precise gap

---

## Visual Layout

```
                    SCREEN
    ┌───────────────────────────────────┐
    │                                   │
    │                                   │
    │    ┌─────────────────────┐        │  ← Toast: 280px wide, 32px tall
    │    │  Press ESC again    │        │     "toast_x" = 20, "toast_y" = 310
    │    │    to cancel        │        │
    │    └─────────────────────┘        │
    │                ▲                  │
    │                │ 6px (gap)        │
    │                │                  │
    │            ┌───────┐              │  ← Pill: 80px wide, 40px tall
    │            │  ●●●  │              │     "pill_x" = 125, "pill_y" = 360
    │            └───────┘              │
    │                                   │
    │                                   │
    └───────────────────────────────────┘
    0                                 1920

    Screen width: 1920px
    Screen height: 1080px
```

---

## The Math

### Given Constants

```
toast_width  = 280.0 px   (width of feedback message box)
toast_height = 32.0 px    (height of feedback message box)
pill_width   = 80.0 px    (width of recording pill)
pill_height  = 40.0 px    (height of recording pill)
gap          = 6.0 px     (vertical space between windows)
```

### Step 1: Calculate Pill Position (center-bottom)

```rust
// In window_manager.rs::calculate_center_position()
// Assuming 1920x1080 screen

let screen_width = 1920.0;
let screen_height = 1080.0;

// Pill is always at center horizontally, bottom with margin
let pill_x = (screen_width - pill_width) / 2.0
           = (1920.0 - 80.0) / 2.0
           = 1840.0 / 2.0
           = 920.0 px

let pill_y = screen_height - pill_height - 80.0  // 80px margin from bottom
           = 1080.0 - 40.0 - 80.0
           = 960.0 px

Result: pill at (920, 960)
```

### Step 2: Calculate Toast Position (above pill, centered)

```rust
// In lib.rs lines 1942-1943
// Toast must be:
// 1. Centered horizontally (same x as pill center)
// 2. Directly above pill with gap

// Toast left edge calculation:
// Toast is 280px wide, pill is 80px wide
// We want toast centered on pill

let toast_x = pill_x + (pill_width - toast_width) / 2.0
            = 920.0 + (80.0 - 280.0) / 2.0
            = 920.0 + (-200.0) / 2.0
            = 920.0 + (-100.0)
            = 820.0 px

// Toast top edge calculation:
// Toast goes above pill, with gap between them

let toast_y = pill_y - toast_height - gap
            = 960.0 - 32.0 - 6.0
            = 922.0 px

Result: toast at (820, 922)
```

### Visual Verification

```
Pill:    x=920,  y=960,  width=80,  height=40
         ├─────────────────┤ 80px
         920              1000

Toast:   x=820,  y=922,  width=280, height=32
         ├───────────────────────────────────┤ 280px
         820                               1100

Alignment check:
- Pill center:  920 + (80/2)  = 960
- Toast center: 820 + (280/2) = 960  ✓ ALIGNED

Vertical gap:
- Pill top:    960
- Toast bottom: 922 + 32 = 954
- Gap:         960 - 954 = 6px  ✓ CORRECT
```

---

## The Problem: Why This Matters

### Scenario: Small 1024x768 Laptop

```
toast_x = 432 + (80 - 280) / 2 = 432 - 100 = 332
toast_y = 688 - 32 - 6 = 650

Toast occupies: x=[332, 612], y=[650, 682]
Screen width: 1024
Right edge: 612 < 1024  ✓ FITS

But on ultra-wide 3440x1440:
toast_x = 1720 - 100 = 1620
Toast occupies: x=[1620, 1900], y=[1350, 1382]
Right edge: 1900 < 3440  ✓ FITS
```

### Scenario: Pill Position Changes

If someone moves the pill to `(200, 500)`:

```
OLD toast_x = 920 + (80 - 280) / 2 = 820
NEW toast_x = 200 + (80 - 280) / 2 = 100

Toast would be misaligned! ❌
```

**That's why the positioning calculation is hardcoded based on pill position, not fixed constants.**

---

## Code Flow

### 1. App Startup (lib.rs:1895-1943)

```rust
// Calculate pill position (fixed: center-bottom)
let (pos_x, pos_y) = calculate_center_position();  // e.g., (920, 960)

// Create pill window
let pill_builder = WebviewWindowBuilder::new(app, "pill", ...)
    .inner_size(80.0, 40.0)
    .position(pos_x, pos_y)  // (920, 960)

// Calculate toast position based on pill position
let toast_width = 280.0;
let toast_height = 32.0;
let pill_width = 80.0;
let gap = 6.0;

let toast_x = pos_x + (pill_width - toast_width) / 2.0;    // 820
let toast_y = pos_y - toast_height - gap;                   // 922

// Create toast window
let toast_builder = WebviewWindowBuilder::new(app, "toast", ...)
    .inner_size(toast_width, toast_height)
    .position(toast_x, toast_y)  // (820, 922)
```

### 2. Recording Started

When user presses hotkey:
```
Backend: ESC detected → handle_escape_key()
Backend: Call pill_toast("Press ESC again to cancel", 1200)
Backend: app.get_webview_window("toast")?.show()  // Toast already positioned at (820, 922)
Frontend: Receive "toast" event → render message
Result: Toast appears above pill
```

### 3. Timeout or Second Press

```
Backend: After 1.2 seconds or on second ESC
Backend: app.get_webview_window("toast")?.hide()
Result: Toast disappears, pill stays visible
```

---

## Critical Dependencies

### If Pill Position Changes

Currently pill position is **hardcoded**:
```rust
// window_manager.rs
fn calculate_center_position() -> (f64, f64) {
    let screen_width = 1920.0;    // ASSUMES 1920px screen
    let screen_height = 1080.0;   // ASSUMES 1080px screen
    
    let pill_x = (screen_width - pill_width) / 2.0;
    let pill_y = screen_height - pill_height - 80.0;
    
    (pill_x, pill_y)
}
```

**Problem:** This doesn't account for:
- DPI scaling (retina displays)
- Multiple monitors
- Actual screen dimensions at runtime

**Current Status:** This is a **known limitation** but works for typical cases.

### If Constants Change

If you change these dimensions:
```rust
let toast_width = 280.0;   // Can't be smaller without cutting off message
let toast_height = 32.0;   // Must fit one line of text + padding
let pill_width = 80.0;     // Must fit the pill visualization
let gap = 6.0;             // Visual spacing preference
```

The centering calculation **automatically adjusts**:
```rust
let toast_x = pos_x + (pill_width - toast_width) / 2.0;
```

So if you increase `toast_width` to 320:
```
toast_x = 920 + (80 - 320) / 2.0 = 920 - 120 = 800
// Toast shifts 20px further left to maintain centering
```

---

## Edge Cases

### Edge Case 1: Toast Exceeds Screen Width

On 1024px screen with 280px toast:
```
pill_x = (1024 - 80) / 2 = 472
toast_x = 472 + (80 - 280) / 2 = 472 - 100 = 372

Toast bounds: [372, 652]
Screen width: 1024
Fits: 652 < 1024  ✓
```

**But with 1280px toast on 1024px screen:**
```
toast_x = 372
Toast bounds: [372, 1652]
Exceeds: 1652 > 1024  ❌
Solution: Clamp toast_x or reduce toast width
```

### Edge Case 2: Vertical Space on Small Screens

Minimum space needed:
```
pill_y (from bottom) = 80px
pill_height = 40px
gap = 6px
toast_height = 32px
minimum_screen_height = 80 + 40 + 6 + 32 = 158px

But with typical margins:
minimum = ~440px (to look decent)
```

---

## Documentation You Should Add

### In lib.rs (line 1935):

```rust
/// TOAST WINDOW POSITIONING ALGORITHM
///
/// Toast is positioned above the pill window, both centered horizontally.
/// This calculation runs once at app startup and again whenever pill position changes.
///
/// LAYOUT:
/// ┌─────────────────────────────────┐
/// │      TOAST (280×32)             │  y = pill_y - toast_height - gap
/// │  "Press ESC again to cancel"    │
/// └─────────────────────────────────┘
///                ▲ 6px gap
///            ┌───────┐
///            │  ●●●  │  PILL (80×40)
///            │ PILL  │  y = center-bottom of screen
///            └───────┘
///
/// CENTERING MATH:
/// The toast (280px) is wider than the pill (80px).
/// To center toast on pill:
///   toast_x = pill_x + (pill_width - toast_width) / 2.0
///
/// With pill_x = 920, this gives:
///   toast_x = 920 + (80 - 280) / 2.0 = 920 - 100 = 820
///
/// IMPORTANT: These dimensions are interdependent:
/// - If pill_width changes, toast_x calculation must update
/// - If toast_width changes, left edge shifts
/// - If pill position changes, toast must move with it
///
/// CONSTRAINTS:
/// - Toast width (280px): Must fit "Press ESC again to cancel" message
/// - Toast height (32px): Must fit one line of text with padding
/// - Gap (6px): Visual spacing between windows
/// - Pill width (80px): UI design constant
///
/// FUTURE IMPROVEMENTS:
/// - Use runtime screen dimensions instead of hardcoded values
/// - Add safe-zone detection to prevent toast clipping
/// - Calculate position when pill is repositioned (if allowed)
```

### In window_manager.rs:

```rust
/// Calculate center-bottom position for pill window
///
/// Pill is always positioned:
/// - Horizontally: Center of screen
/// - Vertically: 80px from bottom (above dock/taskbar)
///
/// Note: Toast window positioning is tightly coupled to this.
/// If you change this, update toast positioning in lib.rs:1942-1943
fn calculate_center_position() -> (f64, f64) {
    // TODO: Use actual screen dimensions at runtime
    // Currently hardcoded to 1920x1080, doesn't account for DPI scaling
    ...
}
```

---

## Testing Checklist

- [ ] 1024×768 display: Toast doesn't clip
- [ ] 4K display (3840×2160): Toast positioned correctly
- [ ] Ultra-wide (3440×1440): Toast centered
- [ ] Retina MacBook (2880×1800): Check DPI scaling
- [ ] Tablet landscape (2560×1440): Toast visible
- [ ] Change toast_width to 320px: Still centered
- [ ] Change pill_width to 100px: Toast re-centers
- [ ] Multiple monitors: Pill appears on primary screen

---

## Summary

**Toast positioning is a fixed calculation, not magic:**

```
1. Pill is always at center-bottom (calculated from screen size)
2. Toast is positioned above pill with gap
3. Toast is centered on pill using: x = pill_x + (pill_width - toast_width) / 2.0
4. These happen at app startup and don't change at runtime
```

**The risk:** If pill position or dimensions change, toast will be misaligned. That's why we need documentation and tests.

