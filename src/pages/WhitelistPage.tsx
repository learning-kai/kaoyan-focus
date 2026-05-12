const sampleApps = [
  { name: 'Word', process: 'WINWORD.EXE', enabled: true },
  { name: 'PDF 阅读器', process: 'SumatraPDF.exe', enabled: true },
  { name: 'Anki', process: 'anki.exe', enabled: false },
];

export default function WhitelistPage() {
  return (
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">阶段 3</p>
        <h2>软件白名单</h2>
        <p>后续将支持手动添加、从运行进程选择、从拦截记录一键加入。</p>
      </div>

      <div className="form-row">
        <input placeholder="软件名称，例如 Anki" />
        <input placeholder="进程名，例如 anki.exe" />
        <button type="button">添加</button>
      </div>

      <div className="list-card">
        {sampleApps.map((app) => (
          <div className="list-row" key={app.process}>
            <div>
              <strong>{app.name}</strong>
              <p>{app.process}</p>
            </div>
            <span className={app.enabled ? 'status enabled' : 'status'}>{app.enabled ? '启用' : '停用'}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
