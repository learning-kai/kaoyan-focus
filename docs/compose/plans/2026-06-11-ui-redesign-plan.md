# UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the kaoyan-focus UI with Arc-style warm/friendly aesthetics and Apple-style smooth/natural animations

**Architecture:** Rewrite CSS files to implement Arc-style warm/friendly design system with Apple-style animations. Focus on three areas: layout restructuring, art style overhaul, and animation enhancement.

**Tech Stack:** CSS3, CSS Custom Properties, CSS Animations, CSS Transitions

---

## File Structure

- **Layout:** `src/styles.css` - Main layout and component styles
- **Art Style:** `src/professional-ui.css` - Visual design system
- **Art Style:** `src/theme-light.css` - Light theme overrides
- **Art Style:** `src/theme-variants.css` - Theme variant definitions
- **Animation:** `src/motion.css` - Animation definitions and transitions
- **Components:** `src/components.css` - Component-specific styles

## Task 1: Layout Restructuring - Sidebar and Navigation

**Covers:** [S3]

**Files:**
- Modify: `src/styles.css:115-280`
- Modify: `src/professional-ui.css:48-117`

- [ ] **Step 1: Update sidebar layout with Arc-style warm design**

```css
.sidebar {
  z-index: var(--z-nav);
  background:
    linear-gradient(180deg, rgba(248, 250, 252, 0.96), rgba(239, 244, 250, 0.92)),
    #f8fafc;
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.86), 12px 0 34px rgba(82, 101, 129, 0.08);
  border-right: none;
  padding: 20px 16px;
}
```

- [ ] **Step 2: Update navigation items with warm interactions**

```css
.nav-item {
  position: relative;
  overflow: hidden;
  min-height: 50px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.74);
  transition:
    background var(--motion-base) var(--ease-standard),
    border-color var(--motion-base) var(--ease-standard),
    color var(--motion-base) var(--ease-standard),
    box-shadow var(--motion-base) var(--ease-standard),
    transform var(--motion-base) var(--ease-standard);
}

.nav-item:hover {
  background: rgba(255, 255, 255, 0.9);
  box-shadow: 0 8px 20px rgba(82, 101, 129, 0.07);
  transform: translateY(-1px);
}

.nav-item[aria-current='page'] {
  background: rgba(255, 255, 255, 0.95);
  box-shadow: 0 8px 20px rgba(82, 101, 129, 0.07);
}
```

- [ ] **Step 3: Run visual verification**

Run: `npm run dev` and check sidebar appearance
Expected: Warm, friendly sidebar with smooth hover effects

- [ ] **Step 4: Commit changes**

```bash
git add src/styles.css src/professional-ui.css
git commit -m "feat: restructure sidebar with Arc-style warm design"
```

## Task 2: Layout Restructuring - Main Content Area

**Covers:** [S3]

**Files:**
- Modify: `src/styles.css:294-320`
- Modify: `src/professional-ui.css:123-140`

- [ ] **Step 1: Update main panel and page shell**

```css
.main-panel {
  display: grid;
  align-content: start;
  padding: 24px;
  background: linear-gradient(180deg, rgba(248, 250, 252, 0.96), rgba(239, 244, 250, 0.92));
}

.page-shell {
  width: min(100%, 1280px);
  margin-inline: auto;
  gap: 24px;
  border-radius: 20px;
  background: #ffffff;
  box-shadow: 0 18px 42px rgba(82, 101, 129, 0.12);
  padding: 32px;
}
```

- [ ] **Step 2: Update page header with warm typography**

```css
.page-header h2 {
  font-size: 32px;
  font-weight: 760;
  text-wrap: balance;
  letter-spacing: -0.02em;
  color: #1a1a2e;
}

.page-header p:not(.eyebrow) {
  max-width: 68ch;
  line-height: 1.6;
  color: #4a5568;
}
```

- [ ] **Step 3: Run visual verification**

Run: `npm run dev` and check main content area
Expected: Clean, spacious layout with warm colors

- [ ] **Step 4: Commit changes**

```bash
git add src/styles.css src/professional-ui.css
git commit -m "feat: restructure main content with Arc-style layout"
```

## Task 3: Art Style - Color System

**Covers:** [S5]

**Files:**
- Modify: `src/styles.css:1-35`
- Modify: `src/professional-ui.css:1-15`

- [ ] **Step 1: Define warm color system**

```css
:root {
  /* Warm neutral colors */
  --bg-warm: #f8f9fa;
  --bg-warm-elevated: #ffffff;
  --surface-warm: rgba(255, 255, 255, 0.95);
  --surface-warm-soft: rgba(255, 255, 255, 0.88);
  
  /* Warm text colors */
  --ink-warm: #1a1a2e;
  --muted-warm: #4a5568;
  --muted-warm-light: #718096;
  
  /* Warm accent colors */
  --blue-warm: #4a90e2;
  --blue-warm-soft: rgba(74, 144, 226, 0.12);
  --green-warm: #48bb78;
  --green-warm-soft: rgba(72, 187, 120, 0.12);
  --amber-warm: #ed8936;
  --amber-warm-soft: rgba(237, 137, 54, 0.12);
  --red-warm: #f56565;
  --red-warm-soft: rgba(245, 101, 101, 0.12);
  
  /* Warm shadows */
  --shadow-warm: 0 4px 12px rgba(0, 0, 0, 0.05);
  --shadow-warm-hover: 0 8px 24px rgba(0, 0, 0, 0.08);
  --shadow-warm-strong: 0 12px 32px rgba(0, 0, 0, 0.1);
  
  /* Border radius */
  --radius-warm: 8px;
  --radius-warm-lg: 12px;
  --radius-warm-xl: 16px;
}
```

- [ ] **Step 2: Update component colors to use warm system**

```css
.primary-action,
.start-ritual-button {
  color: #ffffff;
  background: var(--blue-warm);
  border: 1px solid rgba(74, 144, 226, 0.26);
  box-shadow: 0 4px 12px rgba(74, 144, 226, 0.26);
}

.secondary-action,
.ghost-action,
.small-action {
  color: var(--ink-warm);
  border-color: rgba(82, 101, 129, 0.2);
  background: rgba(255, 255, 255, 0.72);
}
```

- [ ] **Step 3: Run visual verification**

Run: `npm run dev` and check color system
Expected: Warm, friendly color palette throughout the UI

- [ ] **Step 4: Commit changes**

```bash
git add src/styles.css src/professional-ui.css
git commit -m "feat: implement Arc-style warm color system"
```

## Task 4: Art Style - Shadows and Depth

**Covers:** [S5]

**Files:**
- Modify: `src/professional-ui.css:1-15`
- Modify: `src/styles.css:24-28`

- [ ] **Step 1: Update shadow system with warm effects**

```css
:root {
  --shadow-panel: 0 18px 42px rgba(82, 101, 129, 0.12);
  --shadow-card: 0 8px 20px rgba(82, 101, 129, 0.07);
  --shadow-hover: 0 18px 38px rgba(56, 105, 212, 0.14);
  --shadow-control: 0 10px 22px rgba(82, 101, 129, 0.09);
}
```

- [ ] **Step 2: Apply shadows to key components**

```css
.metric-card,
.core-fact,
.details-card {
  background: #ffffff;
  box-shadow: var(--shadow-card);
  border: 1px solid rgba(82, 101, 129, 0.08);
  border-radius: var(--radius-warm-lg);
}

.metric-card:hover,
.core-fact:hover,
.details-card:hover {
  box-shadow: var(--shadow-hover);
  transform: translateY(-2px);
}
```

- [ ] **Step 3: Run visual verification**

Run: `npm run dev` and check shadow effects
Expected: Warm, subtle shadows with smooth hover transitions

- [ ] **Step 4: Commit changes**

```bash
git add src/professional-ui.css src/styles.css
git commit -m "feat: implement warm shadow system with depth"
```

## Task 5: Animation - Apple-style Smooth Transitions

**Covers:** [S4]

**Files:**
- Modify: `src/motion.css:1-12`
- Modify: `src/motion.css:186-280`

- [ ] **Step 1: Update animation variables with Apple-style timing**

```css
:root {
  --motion-enter: 280ms;
  --motion-panel: 240ms;
  --motion-drawer: 280ms;
  --motion-list: 160ms;
  --motion-fast: 120ms;
  --motion-base: 180ms;
  --ease-standard: cubic-bezier(0.25, 0.1, 0.25, 1);
  --ease-emphasized: cubic-bezier(0.34, 1.56, 0.64, 1);
  --ease-apple: cubic-bezier(0.4, 0, 0.2, 1);
}
```

- [ ] **Step 2: Update page transition animations**

```css
.page-transition {
  display: grid;
  min-width: 0;
  animation: page-settle var(--motion-enter) var(--ease-apple) backwards;
}

@keyframes page-settle {
  from {
    opacity: 0;
    transform: translateY(8px) scale(0.998);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}
```

- [ ] **Step 3: Update element hover animations**

```css
.metric-card,
.core-fact,
.details-card {
  transition:
    transform var(--motion-base) var(--ease-apple),
    border-color var(--motion-base) var(--ease-apple),
    background var(--motion-base) var(--ease-apple),
    box-shadow var(--motion-base) var(--ease-apple),
    opacity var(--motion-base) var(--ease-apple);
}

.metric-card:hover,
.core-fact:hover,
.details-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-hover);
}
```

- [ ] **Step 4: Run visual verification**

Run: `npm run dev` and check animation smoothness
Expected: Smooth, natural animations with Apple-style timing

- [ ] **Step 5: Commit changes**

```bash
git add src/motion.css
git commit -m "feat: implement Apple-style smooth animations"
```

## Task 6: Animation - Micro-interactions

**Covers:** [S4]

**Files:**
- Modify: `src/motion.css:624-648`
- Modify: `src/styles.css:508-523`

- [ ] **Step 1: Update button interactions**

```css
.primary-action,
.secondary-action,
.ghost-action,
.small-action,
.start-ritual-button {
  transition:
    background var(--motion-base) var(--ease-apple),
    border-color var(--motion-base) var(--ease-apple),
    color var(--motion-base) var(--ease-apple),
    box-shadow var(--motion-base) var(--ease-apple),
    transform var(--motion-base) var(--ease-apple),
    opacity var(--motion-base) var(--ease-apple);
}

.primary-action:hover,
.start-ritual-button:hover {
  transform: translateY(-1px);
  box-shadow: 0 8px 24px rgba(74, 144, 226, 0.32);
}

.primary-action:active,
.start-ritual-button:active {
  transform: scale(0.98);
}
```

- [ ] **Step 2: Update focus effects**

```css
button:focus-visible,
input:focus-visible,
select:focus-visible,
textarea:focus-visible,
[tabindex]:focus-visible {
  outline: none;
  box-shadow: 0 0 0 3px rgba(74, 144, 226, 0.5);
  transition: box-shadow var(--motion-fast) var(--ease-apple);
}
```

- [ ] **Step 3: Run visual verification**

Run: `npm run dev` and check micro-interactions
Expected: Smooth, responsive interactions with natural feedback

- [ ] **Step 4: Commit changes**

```bash
git add src/motion.css src/styles.css
git commit -m "feat: implement Apple-style micro-interactions"
```

## Task 7: Art Style - Typography and Spacing

**Covers:** [S5]

**Files:**
- Modify: `src/styles.css:1-35`
- Modify: `src/professional-ui.css:1-15`

- [ ] **Step 1: Update typography system**

```css
:root {
  --font-ui: "SF Pro Display", "SF Pro Text", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif;
  --font-size-xs: 12px;
  --font-size-sm: 13px;
  --font-size-base: 14px;
  --font-size-lg: 16px;
  --font-size-xl: 18px;
  --font-size-2xl: 24px;
  --font-size-3xl: 32px;
  --line-height-tight: 1.2;
  --line-height-normal: 1.5;
  --line-height-relaxed: 1.6;
  --letter-spacing-tight: -0.02em;
  --letter-spacing-normal: -0.01em;
}
```

- [ ] **Step 2: Apply typography to components**

```css
.page-header h2 {
  font-size: var(--font-size-3xl);
  font-weight: 760;
  line-height: var(--line-height-tight);
  letter-spacing: var(--letter-spacing-tight);
  color: var(--ink-warm);
}

.page-header p {
  font-size: var(--font-size-base);
  line-height: var(--line-height-relaxed);
  color: var(--muted-warm);
}
```

- [ ] **Step 3: Run visual verification**

Run: `npm run dev` and check typography
Expected: Clean, readable typography with warm colors

- [ ] **Step 4: Commit changes**

```bash
git add src/styles.css src/professional-ui.css
git commit -m "feat: implement Arc-style typography system"
```

## Task 8: Final Integration and Testing

**Covers:** [S6, S7]

**Files:**
- Modify: All CSS files

- [ ] **Step 1: Run full build verification**

Run: `npm run build`
Expected: Successful build with no CSS errors

- [ ] **Step 2: Run type check**

Run: `npm run typecheck`
Expected: No TypeScript errors

- [ ] **Step 3: Run lint check**

Run: `npm run lint`
Expected: No linting errors

- [ ] **Step 4: Manual visual testing**

Run: `npm run dev` and test:
- Sidebar navigation
- Main content layout
- Card interactions
- Button hover effects
- Page transitions
- Responsive design

- [ ] **Step 5: Final commit**

```bash
git add .
git commit -m "feat: complete UI redesign with Arc and Apple styles"
```