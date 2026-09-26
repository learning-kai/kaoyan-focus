import { ChevronDown, Plus, Save, Sparkles, Trash2, Wifi } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import ConfirmDialog from '../../components/ConfirmDialog';
import {
  getAiSchedulerSettings,
  parseAiSchedulerError,
  saveAiSchedulerSettings,
  testAiSchedulerConnection,
} from '../../services/aiSchedulerApi';
import type {
  AiConnectionTestResult,
  AiSchedulerSettings,
  AiTimeWindow,
} from '../../types/aiScheduler';
import { AI_PROVIDER_PRESETS, findAiProviderPreset } from '../../types/aiScheduler';

type AiSchedulerPanelProps = {
  expanded: boolean;
  /** 专注进行中时锁定设置，与其它设置面板一致 */
  locked: boolean;
  onToggle: () => void;
};

const WEEKDAY_LABELS = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];

/** 出境字段说明。首次启用必须让用户明确知道哪些内容会离开本机。 */
const DISCLOSURE_SENT_FIELDS = [
  '任务标题',
  '任务备注（可在下方关闭）',
  '优先级与预计耗时',
  '截止日期',
  '分类名称',
  '已有日程的标题与时间段',
  '你填写的补充指令',
];

const DISCLOSURE_KEPT_LOCAL = [
  '专注记录与统计',
  '复盘内容',
  '白名单配置',
  '账号凭据与 API Key',
];

function minutesToTimeInput(minutes: number) {
  const clamped = Math.max(0, Math.min(1440, Math.round(minutes)));
  const hours = String(Math.floor(clamped / 60)).padStart(2, '0');
  const mins = String(clamped % 60).padStart(2, '0');
  return `${hours}:${mins}`;
}

function timeInputToMinutes(value: string) {
  const [hours, mins] = value.split(':').map(Number);
  if (!Number.isFinite(hours) || !Number.isFinite(mins)) {
    return null;
  }
  return hours * 60 + mins;
}

export function AiSchedulerPanel({ expanded, locked, onToggle }: AiSchedulerPanelProps) {
  const [settings, setSettings] = useState<AiSchedulerSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<AiConnectionTestResult | null>(null);
  const [confirmEnableOpen, setConfirmEnableOpen] = useState(false);

  /**
   * 读取后端配置。
   *
   * `silent` 用于连通性测试后的回读：此时表单已渲染，不该再闪一次「正在读取…」。
   */
  const loadSettings = useCallback(async (options?: { silent?: boolean }) => {
    if (!options?.silent) {
      setLoading(true);
    }
    setError(null);
    try {
      setSettings(await getAiSchedulerSettings());
    } catch (reason) {
      setError(parseAiSchedulerError(reason).message);
    } finally {
      if (!options?.silent) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    if (!expanded || settings) {
      return;
    }
    void loadSettings();
  }, [expanded, loadSettings, settings]);

  function updateSettings(patch: Partial<AiSchedulerSettings>) {
    setSettings((current) => (current ? { ...current, ...patch } : current));
    setNotice(null);
  }

  function applyProviderPreset(key: string) {
    const preset = findAiProviderPreset(key);
    if (!preset) {
      updateSettings({ provider_preset: key });
      return;
    }
    updateSettings({
      provider_preset: preset.key,
      base_url: preset.base_url || settings?.base_url || '',
      model: preset.model || settings?.model || '',
      structured_output_mode: preset.structured_output_mode,
    });
  }

  async function persist(next: AiSchedulerSettings, successMessage: string) {
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await saveAiSchedulerSettings(next);
      setSettings(saved);
      setNotice(successMessage);
    } catch (reason) {
      setError(parseAiSchedulerError(reason).message);
    } finally {
      setSaving(false);
    }
  }

  async function handleSave() {
    if (!settings) {
      return;
    }
    await persist(settings, 'AI 排期配置已保存到本机。');
  }

  async function handleTest() {
    if (!settings) {
      return;
    }
    setTesting(true);
    setError(null);
    setNotice(null);
    setTestResult(null);
    try {
      // 先落盘：测试命令是从本机凭据存储里读密钥的，刚输入但未保存的密钥必须先写进去，
      // 否则会误报 missing_api_key。空密钥在后端等价于「不修改」，所以重复保存是安全的。
      const saved = await saveAiSchedulerSettings(settings);
      setSettings(saved);

      const result = await testAiSchedulerConnection();
      setTestResult(result);
      setNotice(result.message);
      // 探测结果（档位与模型列表）由后端回写，这里静默同步一次前端状态。
      await loadSettings({ silent: true });
    } catch (reason) {
      const parsed = parseAiSchedulerError(reason);
      setError(
        parsed.retryable ? `${parsed.message}（可重试）` : parsed.message,
      );
    } finally {
      setTesting(false);
    }
  }

  function handleToggleEnabled(nextEnabled: boolean) {
    if (!settings) {
      return;
    }
    if (nextEnabled && !settings.privacy_acknowledged) {
      setConfirmEnableOpen(true);
      return;
    }
    updateSettings({ enabled: nextEnabled });
  }

  async function confirmEnable() {
    const current = settings;
    setConfirmEnableOpen(false);
    if (!current) {
      return;
    }
    await persist(
      { ...current, enabled: true, privacy_acknowledged: true },
      'AI 排期已启用。任务属性会在生成排期时发送给所选服务商。',
    );
  }

  const currentPreset = findAiProviderPreset(settings?.provider_preset ?? '');

  return (
    <section className="command-panel">
      <div className="panel-title">
        <div>
          <p className="eyebrow">AI 排期</p>
          <h3>AI 智能日程规划</h3>
        </div>
        <Sparkles size={20} />
        <button
          aria-expanded={expanded}
          className="settings-collapse-button"
          onClick={onToggle}
          type="button"
        >
          <span>{settings?.enabled ? '已启用' : '已关闭'}</span>
          <ChevronDown size={17} />
        </button>
      </div>

      {expanded && (
        <>
          <p className="panel-copy">
            读取清单任务的标题、优先级、预计耗时与截止日，结合你的可用时段生成不重叠的时间轴。
            排期先以草案形式预览，确认后才写入日历；接口不可用时可在错误提示中切换本地兜底。
          </p>

          {loading && <p className="alert neutral">正在读取 AI 排期配置…</p>}

          {!loading && settings && (
            <>
              <label className="capability-row sync-toggle-row">
                <Sparkles size={17} />
                <input
                  checked={settings.enabled}
                  disabled={locked}
                  onChange={(event) => handleToggleEnabled(event.target.checked)}
                  type="checkbox"
                />
                <span>启用 AI 智能日程规划</span>
              </label>

              <div className="ai-butler-card">
                <div className="ai-butler-card-head">
                  <div>
                    <span className="eyebrow">管家模式</span>
                    <strong>让排期自己处理生活节奏</strong>
                    <small>这些偏好会保存在本机，生成每次日程时自动生效。</small>
                  </div>
                  <Sparkles size={18} />
                </div>
                <label className="capability-row sync-toggle-row">
                  <input
                    checked={settings.planner_preferences.auto_meals}
                    disabled={locked}
                    onChange={(event) =>
                      updateSettings({
                        planner_preferences: {
                          ...settings.planner_preferences,
                          auto_meals: event.target.checked,
                        },
                      })
                    }
                    type="checkbox"
                  />
                  <span>自动留出三餐时间</span>
                </label>
                <label className="capability-row sync-toggle-row">
                  <input
                    checked={settings.planner_preferences.adaptive_durations}
                    disabled={locked}
                    onChange={(event) =>
                      updateSettings({
                        planner_preferences: {
                          ...settings.planner_preferences,
                          adaptive_durations: event.target.checked,
                        },
                      })
                    }
                    type="checkbox"
                  />
                  <span>让 AI 按科目难度和剩余时间自动决定每块时长</span>
                </label>
                <div className="inline-fields">
                  <label className="field-block">
                    <span>每日目标学习（分钟）</span>
                    <input
                      className="text-input"
                      disabled={locked}
                      max={960}
                      min={60}
                      onChange={(event) =>
                        updateSettings({
                          planner_preferences: {
                            ...settings.planner_preferences,
                            daily_target_minutes: Number(event.target.value) || 360,
                          },
                        })
                      }
                      step={30}
                      type="number"
                      value={settings.planner_preferences.daily_target_minutes}
                    />
                  </label>
                  <label className="field-block">
                    <span>休息节奏</span>
                    <select
                      className="text-input"
                      disabled={locked}
                      onChange={(event) =>
                        updateSettings({
                          planner_preferences: {
                            ...settings.planner_preferences,
                            rest_style: event.target.value,
                          },
                        })
                      }
                      value={settings.planner_preferences.rest_style}
                    >
                      <option value="gentle">温和：多留一点缓冲</option>
                      <option value="balanced">平衡：学习与休息均衡</option>
                      <option value="focused">专注：减少切换</option>
                    </select>
                  </label>
                </div>
                <div className="ai-meal-grid">
                  {settings.planner_preferences.meal_windows.map((meal, index) => (
                    <label className="field-block" key={`${meal.kind}-${index}`}>
                      <span>{meal.kind}</span>
                      <div className="ai-meal-times">
                        <input
                          aria-label={`${meal.kind}开始时间`}
                          className="text-input"
                          disabled={locked}
                          type="time"
                          value={minutesToTimeInput(meal.start_minute)}
                          onChange={(event) => {
                            const start = timeInputToMinutes(event.target.value);
                            if (start == null) return;
                            const meal_windows = settings.planner_preferences.meal_windows.map((entry, entryIndex) =>
                              entryIndex === index ? { ...entry, start_minute: start } : entry,
                            );
                            updateSettings({ planner_preferences: { ...settings.planner_preferences, meal_windows } });
                          }}
                        />
                        <span>至</span>
                        <input
                          aria-label={`${meal.kind}结束时间`}
                          className="text-input"
                          disabled={locked}
                          type="time"
                          value={minutesToTimeInput(meal.end_minute)}
                          onChange={(event) => {
                            const end = timeInputToMinutes(event.target.value);
                            if (end == null) return;
                            const meal_windows = settings.planner_preferences.meal_windows.map((entry, entryIndex) =>
                              entryIndex === index ? { ...entry, end_minute: end } : entry,
                            );
                            updateSettings({ planner_preferences: { ...settings.planner_preferences, meal_windows } });
                          }}
                        />
                      </div>
                    </label>
                  ))}
                </div>
                <label className="field-block">
                  <span>长期记忆（例如：晚上效率低，专业课放上午）</span>
                  <textarea
                    className="text-input"
                    disabled={locked}
                    maxLength={240}
                    onChange={(event) =>
                      updateSettings({
                        planner_preferences: {
                          ...settings.planner_preferences,
                          memory_note: event.target.value,
                        },
                      })
                    }
                    placeholder="可留空。只保存你主动写下的排期偏好。"
                    rows={2}
                    value={settings.planner_preferences.memory_note}
                  />
                </label>
              </div>

              <div className="form-stack">
                <label className="field-block">
                  <span>服务商预设</span>
                  <select
                    className="text-input"
                    disabled={locked}
                    onChange={(event) => applyProviderPreset(event.target.value)}
                    value={settings.provider_preset}
                  >
                    {AI_PROVIDER_PRESETS.map((preset) => (
                      <option key={preset.key} value={preset.key}>
                        {preset.label}
                      </option>
                    ))}
                  </select>
                </label>
                {currentPreset?.hint && <p className="alert neutral">{currentPreset.hint}</p>}

                <label className="field-block">
                  <span>接口地址（Base URL）</span>
                  <input
                    className="text-input"
                    disabled={locked}
                    onChange={(event) => updateSettings({ base_url: event.target.value })}
                    placeholder="https://api.deepseek.com"
                    value={settings.base_url}
                  />
                </label>

                <label className="field-block">
                  <span>API Key</span>
                  <input
                    autoComplete="off"
                    className="text-input"
                    disabled={locked}
                    onChange={(event) => updateSettings({ api_key: event.target.value })}
                    placeholder={
                      settings.api_key_configured ? '已保存，留空表示不修改' : 'sk-…（只加密保存在本机）'
                    }
                    type="password"
                    value={settings.api_key}
                  />
                </label>

                <div className="inline-fields">
                  <label className="field-block">
                    <span>模型</span>
                    <input
                      className="text-input"
                      disabled={locked}
                      list="ai-scheduler-model-options"
                      onChange={(event) => updateSettings({ model: event.target.value })}
                      placeholder="deepseek-v4-flash"
                      value={settings.model}
                    />
                    <datalist id="ai-scheduler-model-options">
                      {settings.available_models.map((model) => (
                        <option key={model} value={model} />
                      ))}
                    </datalist>
                  </label>
                  <label className="field-block">
                    <span>结构化输出档位</span>
                    <select
                      className="text-input"
                      disabled={locked}
                      onChange={(event) =>
                        updateSettings({
                          structured_output_mode: event.target
                            .value as AiSchedulerSettings['structured_output_mode'],
                        })
                      }
                      value={settings.structured_output_mode}
                    >
                      <option value="auto">自动探测</option>
                      <option value="json_object">json_object（DeepSeek 等）</option>
                      <option value="json_schema">json_schema（OpenAI 等）</option>
                    </select>
                  </label>
                </div>

                <details className="details-card stacked">
                  <summary>请求参数与容量</summary>
                  <div className="inline-fields">
                    <label className="field-block">
                      <span>超时（秒）</span>
                      <input
                        className="text-input"
                        disabled={locked}
                        max={300}
                        min={5}
                        onChange={(event) =>
                          updateSettings({ timeout_seconds: Number(event.target.value) || 60 })
                        }
                        type="number"
                        value={settings.timeout_seconds}
                      />
                    </label>
                    <label className="field-block">
                      <span>失败重试次数</span>
                      <input
                        className="text-input"
                        disabled={locked}
                        max={5}
                        min={0}
                        onChange={(event) =>
                          updateSettings({ max_retries: Number(event.target.value) || 0 })
                        }
                        type="number"
                        value={settings.max_retries}
                      />
                    </label>
                    <label className="field-block">
                      <span>max_tokens</span>
                      <input
                        className="text-input"
                        disabled={locked}
                        max={8192}
                        min={256}
                        onChange={(event) =>
                          updateSettings({ max_tokens: Number(event.target.value) || 2048 })
                        }
                        type="number"
                        value={settings.max_tokens}
                      />
                    </label>
                    <label className="field-block">
                      <span>temperature</span>
                      <input
                        className="text-input"
                        disabled={locked}
                        max={2}
                        min={0}
                        onChange={(event) =>
                          updateSettings({ temperature: Number(event.target.value) || 0 })
                        }
                        step={0.1}
                        type="number"
                        value={settings.temperature}
                      />
                    </label>
                  </div>
                  <div className="inline-fields">
                    <label className="field-block">
                      <span>未估时任务默认时长（分钟）</span>
                      <input
                        className="text-input"
                        disabled={locked}
                        max={480}
                        min={5}
                        onChange={(event) =>
                          updateSettings({
                            default_block_minutes: Number(event.target.value) || 45,
                          })
                        }
                        step={5}
                        type="number"
                        value={settings.default_block_minutes}
                      />
                    </label>
                    <label className="field-block">
                      <span>相邻块最小间隔（分钟）</span>
                      <input
                        className="text-input"
                        disabled={locked}
                        max={120}
                        min={0}
                        onChange={(event) =>
                          updateSettings({ min_break_minutes: Number(event.target.value) || 0 })
                        }
                        step={5}
                        type="number"
                        value={settings.min_break_minutes}
                      />
                    </label>
                    <label className="field-block">
                      <span>单日上限（分钟）</span>
                      <input
                        className="text-input"
                        disabled={locked}
                        max={1440}
                        min={30}
                        onChange={(event) =>
                          updateSettings({ max_daily_minutes: Number(event.target.value) || 480 })
                        }
                        step={30}
                        type="number"
                        value={settings.max_daily_minutes}
                      />
                    </label>
                  </div>
                  <label className="capability-row sync-toggle-row">
                    <Sparkles size={17} />
                    <input
                      checked={settings.disable_thinking}
                      disabled={locked}
                      onChange={(event) =>
                        updateSettings({ disable_thinking: event.target.checked })
                      }
                      type="checkbox"
                    />
                    <span>关闭模型思考模式（推荐，否则 temperature 失效且延迟更高）</span>
                  </label>
                  <label className="capability-row sync-toggle-row">
                    <Sparkles size={17} />
                    <input
                      checked={settings.send_notes}
                      disabled={locked}
                      onChange={(event) => updateSettings({ send_notes: event.target.checked })}
                      type="checkbox"
                    />
                    <span>把任务备注一并发送（关闭后 AI 无法依据备注里的细节排期）</span>
                  </label>
                </details>

                <WindowEditor
                  disabled={locked}
                  label="可用时段（每天可以安排任务的时间）"
                  onChange={(windows) => updateSettings({ available_windows: windows })}
                  windows={settings.available_windows}
                />
                <WindowEditor
                  allowEmpty
                  disabled={locked}
                  label="高效时段（会被优先占用）"
                  onChange={(windows) => updateSettings({ peak_windows: windows })}
                  windows={settings.peak_windows}
                />

                <div className="details-card stacked">
                  <p className="field-block">
                    <span>密钥状态</span>
                    <strong>{settings.api_key_configured ? '已配置（加密保存在本机）' : '未配置'}</strong>
                  </p>
                  {testResult && (
                    <>
                      <p className="field-block">
                        <span>档位探测结果</span>
                        <strong>{testResult.structured_output_mode}</strong>
                      </p>
                      <p className="field-block">
                        <span>可用模型</span>
                        <strong>
                          {testResult.available_models.length > 0
                            ? `${testResult.available_models.length} 个`
                            : '网关未提供列表'}
                        </strong>
                      </p>
                    </>
                  )}
                </div>
              </div>

              {error && (
                <p className="alert error" role="alert">
                  {error}
                </p>
              )}
              {notice && <p className="alert success">{notice}</p>}
              {!settings.enabled && (
                <p className="alert neutral">
                  AI 排期已关闭，清单页与日历页不会出现「AI 排期」入口。
                </p>
              )}

              <div className="row-actions">
                <button
                  className="secondary-action"
                  disabled={saving || locked}
                  onClick={() => void handleSave()}
                  type="button"
                >
                  <Save size={17} />
                  保存配置
                </button>
                <button
                  className="secondary-action"
                  disabled={testing || locked || !settings.base_url || !settings.model}
                  onClick={() => void handleTest()}
                  type="button"
                >
                  <Wifi size={17} />
                  测试连接
                </button>
              </div>
              {(saving || testing) && (
                <p className="alert neutral">{testing ? '正在测试连接…' : '正在保存…'}</p>
              )}
            </>
          )}
        </>
      )}

      <ConfirmDialog
        cancelLabel="暂不启用"
        confirmLabel="我已知悉，启用"
        loading={saving}
        message="启用前请确认：生成排期时，下列任务属性会发送到你选择的服务商。"
        onCancel={() => setConfirmEnableOpen(false)}
        onConfirm={() => void confirmEnable()}
        open={confirmEnableOpen}
        title="启用 AI 排期会发送这些数据"
      >
        <div className="confirm-dialog-columns">
          <div>
            <p className="eyebrow">会发送</p>
            <ul>
              {DISCLOSURE_SENT_FIELDS.map((field) => (
                <li key={field}>{field}</li>
              ))}
            </ul>
          </div>
          <div>
            <p className="eyebrow">不会发送</p>
            <ul>
              {DISCLOSURE_KEPT_LOCAL.map((field) => (
                <li key={field}>{field}</li>
              ))}
            </ul>
          </div>
        </div>
      </ConfirmDialog>
    </section>
  );
}

function WindowEditor({
  allowEmpty = false,
  disabled,
  label,
  windows,
  onChange,
}: {
  allowEmpty?: boolean;
  disabled: boolean;
  label: string;
  windows: AiTimeWindow[];
  onChange: (windows: AiTimeWindow[]) => void;
}) {
  function updateRow(index: number, patch: Partial<AiTimeWindow>) {
    onChange(windows.map((window, current) => (current === index ? { ...window, ...patch } : window)));
  }

  function removeRow(index: number) {
    onChange(windows.filter((_, current) => current !== index));
  }

  function addRow() {
    const last = windows[windows.length - 1];
    onChange([
      ...windows,
      {
        weekday: last ? Math.min(7, last.weekday + 1) : 1,
        start_minute: last?.start_minute ?? 8 * 60,
        end_minute: last?.end_minute ?? 22 * 60,
      },
    ]);
  }

  return (
    <div className="details-card stacked">
      <p className="field-block">
        <span>{label}</span>
        <strong>{windows.length === 0 ? (allowEmpty ? '未设置' : '未设置（将使用默认 08:00–22:00）') : `${windows.length} 段`}</strong>
      </p>
      {windows.map((window, index) => (
        <div className="inline-fields" key={`${window.weekday}-${window.start_minute}-${index}`}>
          <label className="field-block">
            <span>星期</span>
            <select
              className="text-input"
              disabled={disabled}
              onChange={(event) => updateRow(index, { weekday: Number(event.target.value) })}
              value={window.weekday}
            >
              {WEEKDAY_LABELS.map((dayLabel, dayIndex) => (
                <option key={dayLabel} value={dayIndex + 1}>
                  {dayLabel}
                </option>
              ))}
            </select>
          </label>
          <label className="field-block">
            <span>开始</span>
            <input
              className="text-input"
              disabled={disabled}
              onChange={(event) => {
                const minutes = timeInputToMinutes(event.target.value);
                if (minutes !== null) {
                  updateRow(index, { start_minute: minutes });
                }
              }}
              type="time"
              value={minutesToTimeInput(window.start_minute)}
            />
          </label>
          <label className="field-block">
            <span>结束</span>
            <input
              className="text-input"
              disabled={disabled}
              onChange={(event) => {
                const minutes = timeInputToMinutes(event.target.value);
                if (minutes !== null) {
                  updateRow(index, { end_minute: minutes });
                }
              }}
              type="time"
              value={minutesToTimeInput(window.end_minute)}
            />
          </label>
          <button
            aria-label="删除该时段"
            className="small-action danger"
            disabled={disabled}
            onClick={() => removeRow(index)}
            type="button"
          >
            <Trash2 size={15} />
          </button>
        </div>
      ))}
      <div className="row-actions">
        <button className="ghost-action" disabled={disabled} onClick={addRow} type="button">
          <Plus size={15} />
          添加时段
        </button>
      </div>
    </div>
  );
}
