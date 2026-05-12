export default function SettingsPage() {
  return (
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">设置</p>
        <h2>约束边界</h2>
        <p>本软件只做用户态自律约束，不隐藏进程、不绕过权限、不阻止任务管理器。</p>
      </div>

      <div className="settings-list">
        <label>
          <input type="checkbox" disabled />
          专注期间关闭窗口时最小化到托盘（阶段 6）
        </label>
        <label>
          <input type="checkbox" disabled />
          启用严格模式应急退出（阶段 8）
        </label>
        <label>
          <input type="checkbox" disabled />
          记录非白名单应用干扰事件（阶段 5）
        </label>
      </div>
    </section>
  );
}
