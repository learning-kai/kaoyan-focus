const stats = [
  { label: '今日学习', value: '0 分钟' },
  { label: '本周学习', value: '0 小时' },
  { label: '本月学习', value: '0 小时' },
  { label: '干扰次数', value: '0 次' },
];

export default function StatsPage() {
  return (
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">阶段 7</p>
        <h2>学习统计</h2>
        <p>后续将按政治、英语、数学、专业课统计专注时长。</p>
      </div>

      <div className="stats-grid">
        {stats.map((item) => (
          <article className="stat-card" key={item.label}>
            <span>{item.label}</span>
            <strong>{item.value}</strong>
          </article>
        ))}
      </div>
    </section>
  );
}
