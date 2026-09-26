/**
 * AI 排期抽屉的跨页事件总线。
 *
 * 抽屉是 **App 级单例**：清单页 / 日历页只负责放「AI 排期」按钮并发事件，
 * App 持有抽屉并渲染。这样切换页面（页面组件随之卸载）不会把抽屉一起关掉，
 * 草案在 App 层继续挂着，回来时也还是原来那份。
 *
 * 沿用仓库既有的 CustomEvent 约定（见 App.tsx 的 `APP_NAVIGATE_EVENT`），
 * 不引入额外的状态库。
 */

/** 分类显示名，来自清单页数据；日历页不传时用 AiPlanTimeline 的兜底名。 */
export type AiPlanCategoryLabels = Record<string, string>;

export const AI_PLAN_OPEN_EVENT = 'kaoyan-focus:open-ai-plan';
export const AI_PLAN_APPLIED_EVENT = 'kaoyan-focus:ai-plan-applied';

/** 页面请求打开 AI 排期抽屉。 */
export function openAiPlanDrawer(
  targetDate: string,
  categoryLabels?: AiPlanCategoryLabels,
): void {
  window.dispatchEvent(
    new CustomEvent(AI_PLAN_OPEN_EVENT, { detail: { targetDate, categoryLabels } }),
  );
}

/** 抽屉写入成功后通知宿主页面刷新清单 / 日历。 */
export function notifyAiPlanApplied(): void {
  window.dispatchEvent(new Event(AI_PLAN_APPLIED_EVENT));
}

/** 订阅「打开抽屉」。返回取消订阅函数，卸载时调用。 */
export function onAiPlanOpen(
  handler: (targetDate: string, categoryLabels?: AiPlanCategoryLabels) => void,
): () => void {
  const listener = (event: Event) => {
    const detail = (event as CustomEvent<{
      targetDate?: string;
      categoryLabels?: AiPlanCategoryLabels;
    }>).detail;
    if (typeof detail?.targetDate === 'string' && detail.targetDate) {
      handler(detail.targetDate, detail.categoryLabels);
    }
  };
  window.addEventListener(AI_PLAN_OPEN_EVENT, listener);
  return () => window.removeEventListener(AI_PLAN_OPEN_EVENT, listener);
}

/** 订阅「写入成功」。返回取消订阅函数，卸载时调用。 */
export function onAiPlanApplied(handler: () => void): () => void {
  window.addEventListener(AI_PLAN_APPLIED_EVENT, handler);
  return () => window.removeEventListener(AI_PLAN_APPLIED_EVENT, handler);
}
