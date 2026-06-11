# 白名单页面选项卡切换布局实施计划

> [!NOTE]
> This document may not reflect the current implementation.
> See the final report for up-to-date state:
> [Final Report](../reports/whitelist-tab-layout.md)

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将白名单页面从垂直堆叠布局改为选项卡切换布局，减少页面滚动长度。

**Architecture:** 使用简单的选项卡切换，将页面分为"查看规则"和"添加规则"两个选项卡。默认显示规则列表，点击"添加规则"选项卡才显示表单和快速来源面板。

**Tech Stack:** React, TypeScript, CSS

---

### Task 1: 添加选项卡状态管理

**Covers:** 页面布局优化

**Files:**
- Modify: `src/pages/WhitelistPage.tsx:63-90`

- [ ] **Step 1: 添加选项卡状态变量**

在组件状态中添加选项卡状态：

```typescript
type WhitelistTab = 'rules' | 'add';

// 在状态声明部分添加
const [activeTab, setActiveTab] = useState<WhitelistTab>('rules');
```

- [ ] **Step 2: 验证状态添加**

运行类型检查确保没有错误：

```bash
npm run typecheck
```

Expected: 无错误

- [ ] **Step 3: 提交更改**

```bash
git add src/pages/WhitelistPage.tsx
git commit -m "feat: add tab state management for whitelist page"
```

### Task 2: 创建选项卡切换组件

**Covers:** 页面布局优化

**Files:**
- Modify: `src/pages/WhitelistPage.tsx:395-436`

- [ ] **Step 1: 在页面头部添加选项卡切换**

在现有的页面头部（`<header className="page-header">`）之后，添加选项卡切换组件：

```tsx
{/* 选项卡切换 */}
<div className="whitelist-tabs">
  <button
    className={`whitelist-tab ${activeTab === 'rules' ? 'active' : ''}`}
    onClick={() => setActiveTab('rules')}
    type="button"
  >
    查看规则
  </button>
  <button
    className={`whitelist-tab ${activeTab === 'add' ? 'active' : ''}`}
    onClick={() => setActiveTab('add')}
    type="button"
  >
    添加规则
  </button>
</div>
```

- [ ] **Step 2: 验证组件添加**

运行开发服务器查看效果：

```bash
npm run dev
```

Expected: 页面显示选项卡切换，但内容尚未分离

- [ ] **Step 3: 提交更改**

```bash
git add src/pages/WhitelistPage.tsx
git commit -m "feat: add tab switcher component to whitelist page"
```

### Task 3: 分离页面内容到两个选项卡

**Covers:** 页面布局优化

**Files:**
- Modify: `src/pages/WhitelistPage.tsx:438-831`

- [ ] **Step 1: 将添加规则面板和快速来源面板包裹在条件渲染中**

将现有的`<div className="whitelist-workbench">`及其内容包裹在条件渲染中：

```tsx
{activeTab === 'add' && (
  <div className="whitelist-workbench">
    <section className="command-panel add-rule-panel">
      {/* 现有的添加规则表单内容 */}
    </section>
    
    <section className="command-panel source-panel">
      {/* 现有的快速来源面板内容 */}
    </section>
  </div>
)}
```

- [ ] **Step 2: 将规则列表包裹在条件渲染中**

将现有的规则列表部分包裹在条件渲染中：

```tsx
{activeTab === 'rules' && (
  <section className="command-panel">
    <div className="panel-title">
      {/* 现有的标题内容 */}
    </div>
    
    {apps.length === 0 ? (
      <div className="empty-state">
        {/* 现有的空状态内容 */}
      </div>
    ) : (
      <div className="rule-list">
        {/* 现有的规则列表内容 */}
      </div>
    )}
  </section>
)}
```

- [ ] **Step 3: 保持进程选择器和拦截记录选择器在添加规则选项卡中**

确保这两个选择器只在添加规则选项卡中显示：

```tsx
{activeTab === 'add' && processPickerOpen && (
  <section className="command-panel picker-panel">
    {/* 现有的进程选择器内容 */}
  </section>
)}

{activeTab === 'add' && blockedPickerOpen && (
  <section className="command-panel picker-panel">
    {/* 现有的拦截记录选择器内容 */}
  </section>
)}
```

- [ ] **Step 4: 验证内容分离**

运行开发服务器测试选项卡切换：

```bash
npm run dev
```

Expected: 点击选项卡可以切换显示不同内容

- [ ] **Step 5: 提交更改**

```bash
git add src/pages/WhitelistPage.tsx
git commit -m "feat: separate whitelist page content into tabs"
```

### Task 4: 添加选项卡样式

**Covers:** 页面布局优化

**Files:**
- Modify: `src/styles.css`

- [ ] **Step 1: 添加选项卡基础样式**

在styles.css中添加选项卡样式：

```css
.whitelist-tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--line);
}

.whitelist-tab {
  padding: 8px 16px;
  border: none;
  background: transparent;
  color: var(--muted);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: all 0.2s ease;
}

.whitelist-tab:hover {
  color: var(--text);
}

.whitelist-tab.active {
  color: var(--text);
  border-bottom-color: var(--text);
}
```

- [ ] **Step 2: 验证样式应用**

运行开发服务器查看样式效果：

```bash
npm run dev
```

Expected: 选项卡显示正确的样式和交互效果

- [ ] **Step 3: 提交更改**

```bash
git add src/styles.css
git commit -m "style: add whitelist tab switcher styles"
```

### Task 5: 优化响应式设计

**Covers:** 页面布局优化

**Files:**
- Modify: `src/styles.css:4630-4659`

- [ ] **Step 1: 在媒体查询中添加选项卡样式**

在现有的移动端媒体查询中添加选项卡样式：

```css
@media (max-width: 768px) {
  .whitelist-tabs {
    flex-direction: column;
    gap: 0;
  }
  
  .whitelist-tab {
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
    text-align: left;
  }
  
  .whitelist-tab.active {
    border-bottom-color: var(--text);
    background: var(--surface);
  }
}
```

- [ ] **Step 2: 验证响应式效果**

在浏览器中调整窗口大小，测试移动端布局：

```bash
npm run dev
```

Expected: 在小屏幕设备上，选项卡垂直堆叠，内容适应屏幕宽度

- [ ] **Step 3: 提交更改**

```bash
git add src/styles.css
git commit -m "style: add responsive styles for whitelist tabs"
```

### Task 6: 测试和验证

**Covers:** 页面布局优化

**Files:**
- Test: 手动测试

- [ ] **Step 1: 运行完整测试**

```bash
npm run dev
```

测试以下场景：
1. 默认显示规则列表选项卡
2. 点击"添加规则"选项卡显示表单
3. 添加白名单规则后自动切换到规则列表
4. 学习模式锁定状态下选项卡切换正常
5. 响应式布局在不同屏幕尺寸下正常显示

- [ ] **Step 2: 运行类型检查和lint**

```bash
npm run typecheck
npm run lint
```

Expected: 无错误

- [ ] **Step 3: 最终提交**

```bash
git add .
git commit -m "feat: implement whitelist page tab layout optimization

- Add tab switcher to reduce page scroll length
- Separate 'view rules' and 'add rule' into tabs
- Default to rules list tab for better UX
- Add responsive styles for mobile devices
- Maintain all existing functionality"
```

### Task 7: 文档更新

**Covers:** 页面布局优化

**Files:**
- Modify: `FEATURES.md`

- [ ] **Step 1: 更新功能文档**

在FEATURES.md中添加白名单页面优化说明：

```markdown
## 白名单页面优化

白名单页面已从垂直堆叠布局改为选项卡切换布局：
- **查看规则选项卡**：默认显示，展示当前所有白名单规则
- **添加规则选项卡**：点击后显示添加表单和快速来源面板
- **减少滚动**：页面长度减少约50%，提升用户体验
- **响应式设计**：在移动设备上自动适配布局
```

- [ ] **Step 2: 提交文档更新**

```bash
git add FEATURES.md
git commit -m "docs: update whitelist page optimization in FEATURES.md"
```