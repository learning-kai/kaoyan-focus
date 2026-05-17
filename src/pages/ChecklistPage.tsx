import { Fragment, useEffect, useMemo, useState, type KeyboardEvent } from 'react';
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from '@dnd-kit/core';
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import {
  CalendarCheck2,
  Check,
  ChevronDown,
  GripVertical,
  ListPlus,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import TodayPlanDrawer from '../components/TodayPlanDrawer';
import {
  addTaskToTodayPlan,
  completeChecklistTask,
  completeTodayPlanItem,
  createChecklistTask,
  createTodayPlanItem,
  deleteChecklistTask,
  deleteTodayPlanItem,
  getChecklistPageData,
  reorderChecklistTasks,
  reorderTodayPlanItems,
  updateChecklistTask,
  updateTodayPlanItem,
} from '../services/checklistApi';
import { getStudyModeState } from '../services/focusApi';
import type {
  ChecklistCategory,
  ChecklistPageData,
  ChecklistTask,
  ChecklistTaskDraft,
  TodayPlanItem,
  TodayPlanItemDraft,
} from '../types/checklist';
import type { StudyModeState } from '../types/focus';

type DragState =
  | { kind: 'today'; itemId: number }
  | { kind: 'task'; taskId: number }
  | null;

const categoryOrder = ['politics', 'english', 'math', 'major', 'general'] as const;
const TODAY_CONTAINER_ID = 'today-container';
const CATEGORY_CONTAINER_ID = 'category-container';

function isSingleLineSubmitKey(event: KeyboardEvent<HTMLInputElement>) {
  return event.key === 'Enter'
    && !event.shiftKey
    && !event.altKey
    && !event.ctrlKey
    && !event.metaKey
    && !event.nativeEvent.isComposing;
}

function handleSubmitOnEnter(
  event: KeyboardEvent<HTMLInputElement>,
  canSubmit: boolean,
  submit: () => void,
) {
  if (!canSubmit || !isSingleLineSubmitKey(event)) {
    return;
  }

  event.preventDefault();
  submit();
}

const subjectNameMap: Record<number, string> = {
  1: '政治',
  2: '英语',
  3: '数学',
  4: '专业课',
};

const emptyTodayDraft: TodayPlanItemDraft = {
  title: '',
  note: '',
  dueDate: '',
  subjectId: null,
};

const emptyTaskDraft = (categoryKey: string): ChecklistTaskDraft => ({
  categoryKey,
  title: '',
  note: '',
  dueDate: '',
});

function summarizeText(value: string | null | undefined, fallback: string, maxLength = 18) {
  const text = value?.trim();
  if (!text) {
    return fallback;
  }

  return text.length > maxLength ? `${text.slice(0, maxLength)}…` : text;
}

function buildTodayMeta(item: TodayPlanItem) {
  const parts: string[] = [];
  if (item.note?.trim()) {
    parts.push(summarizeText(item.note, '', 12));
  }
  if (item.due_date) {
    parts.push(`截止 ${item.due_date}`);
  }

  return parts.length > 0 ? parts.join(' · ') : '无备注';
}

function buildTaskMeta(task: ChecklistTask) {
  const parts: string[] = [];
  if (task.note?.trim()) {
    parts.push(summarizeText(task.note, '', 18));
  }
  if (task.due_date) {
    parts.push(`截止 ${task.due_date}`);
  }

  return parts.length > 0 ? parts.join(' · ') : '未设置备注或截止日期';
}

function getCategoryKeyForSubject(subjectId: number | null | undefined) {
  switch (subjectId) {
    case 1:
      return 'politics';
    case 2:
      return 'english';
    case 3:
      return 'math';
    case 4:
      return 'major';
    default:
      return null;
  }
}

function getTodaySortableId(itemId: number) {
  return `today:${itemId}`;
}

function getTaskSortableId(taskId: number) {
  return `task:${taskId}`;
}

function parseDragIdentifier(identifier: string | number | null | undefined): DragState {
  if (typeof identifier !== 'string') {
    return null;
  }

  if (identifier.startsWith('today:')) {
    const itemId = Number(identifier.slice(6));
    return Number.isFinite(itemId) ? { kind: 'today', itemId } : null;
  }

  if (identifier.startsWith('task:')) {
    const taskId = Number(identifier.slice(5));
    return Number.isFinite(taskId) ? { kind: 'task', taskId } : null;
  }

  return null;
}

function resolveActiveCategoryKey(pageData: ChecklistPageData, preferredCategoryKey: string | null) {
  if (preferredCategoryKey && pageData.categories.some((category) => category.key === preferredCategoryKey)) {
    return preferredCategoryKey;
  }

  if (pageData.categories.some((category) => category.key === pageData.active_category_key)) {
    return pageData.active_category_key;
  }

  return pageData.categories[0]?.key ?? 'politics';
}

export default function ChecklistPage() {
  const [data, setData] = useState<ChecklistPageData | null>(null);
  const [studyState, setStudyState] = useState<StudyModeState | null>(null);
  const [activeCategoryKey, setActiveCategoryKey] = useState<string>('politics');
  const [composerCategoryKey, setComposerCategoryKey] = useState<string | null>(null);
  const [dragState, setDragState] = useState<DragState>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [taskDrafts, setTaskDrafts] = useState<Record<string, ChecklistTaskDraft>>({});
  const [editingTaskId, setEditingTaskId] = useState<number | null>(null);
  const [editingTaskDraft, setEditingTaskDraft] = useState<ChecklistTaskDraft | null>(null);
  const [showTodayComposer, setShowTodayComposer] = useState(false);
  const [todayDraft, setTodayDraft] = useState<TodayPlanItemDraft>(emptyTodayDraft);
  const [editingTodayId, setEditingTodayId] = useState<number | null>(null);
  const [editingTodayDraft, setEditingTodayDraft] = useState<TodayPlanItemDraft>(emptyTodayDraft);
  const [showCompleted, setShowCompleted] = useState<Record<string, boolean>>({});
  const [dragOverId, setDragOverId] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 6,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const categories = useMemo(() => {
    if (!data) {
      return [];
    }

    const byKey = new Map(data.categories.map((category) => [category.key, category]));
    return categoryOrder
      .map((key) => byKey.get(key))
      .filter((category): category is ChecklistCategory => Boolean(category));
  }, [data]);

  const activeCategory =
    categories.find((category) => category.key === activeCategoryKey) ?? categories[0] ?? null;
  const currentSubjectId = studyState?.status === 'active' ? studyState.subject_id : null;
  const currentSubjectLabel = currentSubjectId ? subjectNameMap[currentSubjectId] ?? '当前科目' : null;
  const highlightedCategoryKey = getCategoryKeyForSubject(currentSubjectId);
  const todaySourceTaskIds = useMemo(() => {
    const ids = new Set<number>();
    for (const item of data?.today_items ?? []) {
      if (typeof item.source_task_id === 'number') {
        ids.add(item.source_task_id);
      }
    }
    return ids;
  }, [data]);

  useEffect(() => {
    void initializePage();
  }, []);

  async function initializePage(preferredCategoryKey: string | null = null) {
    try {
      setLoading(true);
      setError(null);
      const [pageData, currentStudyState] = await Promise.all([
        getChecklistPageData(),
        getStudyModeState(),
      ]);

      setData(pageData);
      setStudyState(currentStudyState);
      setActiveCategoryKey(resolveActiveCategoryKey(pageData, preferredCategoryKey));
      seedDrafts(pageData);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  function seedDrafts(pageData: ChecklistPageData) {
    const nextDrafts: Record<string, ChecklistTaskDraft> = {};
    for (const category of pageData.categories) {
      nextDrafts[category.key] = emptyTaskDraft(category.key);
    }
    setTaskDrafts(nextDrafts);
  }

  async function withRefresh(
    work: () => Promise<void>,
    successMessage?: string,
    preferredCategoryKey: string | null = activeCategoryKey,
  ) {
    try {
      setSaving(true);
      setError(null);
      setMessage(null);
      await work();
      await initializePage(preferredCategoryKey);
      if (successMessage) {
        setMessage(successMessage);
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
      setDragState(null);
    }
  }

  function updateTaskDraft(categoryKey: string, patch: Partial<ChecklistTaskDraft>) {
    setTaskDrafts((current) => ({
      ...current,
      [categoryKey]: {
        ...current[categoryKey],
        ...patch,
      },
    }));
  }

  function beginEditTask(task: ChecklistTask) {
    setEditingTaskId(task.id);
    setEditingTaskDraft({
      categoryKey: task.category_key,
      title: task.title,
      note: task.note ?? '',
      dueDate: task.due_date ?? '',
    });
    setComposerCategoryKey(null);
  }

  function beginEditTodayItem(item: TodayPlanItem) {
    setEditingTodayId(item.id);
    setEditingTodayDraft({
      title: item.title,
      note: item.note ?? '',
      dueDate: item.due_date ?? '',
      subjectId: item.subject_id,
    });
  }

  function handleSelectCategory(categoryKey: string) {
    setActiveCategoryKey(categoryKey);
    setComposerCategoryKey(null);
    setEditingTaskId(null);
    setEditingTaskDraft(null);
  }

  function toggleActiveCategoryComposer() {
    if (!activeCategory) {
      return;
    }

    setEditingTaskId(null);
    setEditingTaskDraft(null);
    setComposerCategoryKey((current) => (current === activeCategory.key ? null : activeCategory.key));
  }

  function findTaskByDragState() {
    if (!data || dragState?.kind !== 'task') {
      return null;
    }

    for (const category of data.categories) {
      const task = category.pending_tasks.find((item) => item.id === dragState.taskId);
      if (task) {
        return task;
      }
    }

    return null;
  }

  async function handleCreateTask(categoryKey: string) {
    const draft = taskDrafts[categoryKey];
    if (!draft?.title.trim()) {
      return;
    }

    await withRefresh(async () => {
      await createChecklistTask(draft);
      setTaskDrafts((current) => ({
        ...current,
        [categoryKey]: emptyTaskDraft(categoryKey),
      }));
      setComposerCategoryKey(null);
    }, '待办已加入当前分类。', categoryKey);
  }

  async function handleSaveTaskEdit() {
    if (editingTaskId === null || editingTaskDraft === null || !editingTaskDraft.title.trim()) {
      return;
    }

    await withRefresh(async () => {
      await updateChecklistTask(editingTaskId, editingTaskDraft);
      setEditingTaskId(null);
      setEditingTaskDraft(null);
    }, '待办已更新。', activeCategoryKey);
  }

  async function handleDeleteTask(taskId: number) {
    if (!window.confirm('删除这条待办后，关联的今日任务实例也会一起删除。继续吗？')) {
      return;
    }

    await withRefresh(async () => {
      await deleteChecklistTask(taskId);
    }, '待办已删除。', activeCategoryKey);
  }

  async function handleToggleTask(task: ChecklistTask) {
    await withRefresh(async () => {
      await completeChecklistTask(task.id, !task.completed);
    }, task.completed ? '待办已恢复为未完成。' : '待办已移入已完成。', activeCategoryKey);
  }

  async function handleAddToToday(task: ChecklistTask, insertIndex?: number) {
    if (!data) {
      return;
    }

    const exists = data.today_items.some((item) => item.source_task_id === task.id);
    if (exists) {
      setMessage('这条待办今天已经进入任务区了。');
      setDragState(null);
      return;
    }

    await withRefresh(async () => {
      const created = await addTaskToTodayPlan(task.id);
      const items = [...data.today_items];
      const targetIndex = Math.max(0, Math.min(insertIndex ?? items.length, items.length));

      if (!items.some((item) => item.id === created.id)) {
        items.splice(targetIndex, 0, created);
      }

      await reorderTodayPlanItems(items.map((item) => item.id));
    }, '已加入顶部进入任务区。', activeCategoryKey);
  }

  async function handleCreateTodayItem() {
    if (!todayDraft.title.trim()) {
      return;
    }

    await withRefresh(async () => {
      await createTodayPlanItem(todayDraft);
      setTodayDraft(emptyTodayDraft);
      setShowTodayComposer(false);
    }, '进入任务已添加。', activeCategoryKey);
  }

  async function handleSaveTodayEdit() {
    if (editingTodayId === null || !editingTodayDraft.title.trim()) {
      return;
    }

    await withRefresh(async () => {
      await updateTodayPlanItem(editingTodayId, editingTodayDraft);
      setEditingTodayId(null);
      setEditingTodayDraft(emptyTodayDraft);
    }, '进入任务已更新。', activeCategoryKey);
  }

  async function handleDeleteTodayItem(itemId: number) {
    await withRefresh(async () => {
      await deleteTodayPlanItem(itemId);
    }, '进入任务已删除。', activeCategoryKey);
  }

  async function handleCompleteTodayItem(item: TodayPlanItem) {
    const nextCompleted = !item.completed;
    let syncSourceCompletion = false;

    if (nextCompleted && item.source_task_id !== null) {
      syncSourceCompletion = window.confirm('完成今日任务时，也同步完成源待办吗？');
    }

    await withRefresh(async () => {
      await completeTodayPlanItem(item.id, nextCompleted, syncSourceCompletion);
    }, nextCompleted ? '进入任务已完成。' : '进入任务已恢复为未完成。', activeCategoryKey);
  }

  async function handleTodayListDrop(insertIndex?: number) {
    if (!data || dragState === null) {
      return;
    }

    if (dragState.kind === 'today') {
      const items = [...data.today_items];
      const fromIndex = items.findIndex((item) => item.id === dragState.itemId);
      if (fromIndex === -1) {
        return;
      }

      const [moved] = items.splice(fromIndex, 1);
      let targetIndex = insertIndex ?? items.length;
      if (fromIndex < targetIndex) {
        targetIndex -= 1;
      }
      targetIndex = Math.max(0, Math.min(targetIndex, items.length));

      if (targetIndex === fromIndex) {
        setDragState(null);
        return;
      }

      items.splice(targetIndex, 0, moved);

      await withRefresh(async () => {
        await reorderTodayPlanItems(items.map((item) => item.id));
      }, undefined, activeCategoryKey);
      return;
    }

    const task = findTaskByDragState();
    if (!task) {
      return;
    }

    await handleAddToToday(task, insertIndex);
  }

  async function handleCategoryTaskDrop(insertIndex?: number) {
    if (!activeCategory || dragState?.kind !== 'task') {
      return;
    }

    const tasks = [...activeCategory.pending_tasks];
    const fromIndex = tasks.findIndex((task) => task.id === dragState.taskId);
    if (fromIndex === -1) {
      return;
    }

    const [moved] = tasks.splice(fromIndex, 1);
    let targetIndex = insertIndex ?? tasks.length;
    if (fromIndex < targetIndex) {
      targetIndex -= 1;
    }
    targetIndex = Math.max(0, Math.min(targetIndex, tasks.length));

    if (targetIndex === fromIndex) {
      setDragState(null);
      return;
    }

    tasks.splice(targetIndex, 0, moved);

    await withRefresh(async () => {
      await reorderChecklistTasks(activeCategory.key, tasks.map((task) => task.id));
    }, undefined, activeCategory.key);
  }

  function handleDragStart(event: DragStartEvent) {
    setMessage(null);
    const nextDragState = parseDragIdentifier(event.active.id);
    setDragState(nextDragState);
    setDragOverId(null);
  }

  function handleDragOver(event: DragOverEvent) {
    if (event.over) {
      setDragOverId(String(event.over.id));
    } else {
      setDragOverId(null);
    }
  }

  async function handleDragEnd(event: DragEndEvent) {
    const nextDragState = parseDragIdentifier(event.active.id);
    const overId = event.over ? String(event.over.id) : null;

    setDragState(null);
    setDragOverId(null);

    if (!nextDragState || !overId || !data) {
      return;
    }

    if (nextDragState.kind === 'today') {
      const items = [...data.today_items];
      const fromIndex = items.findIndex((item) => item.id === nextDragState.itemId);
      if (fromIndex === -1) {
        return;
      }

      let targetIndex = items.length - 1;
      if (overId === TODAY_CONTAINER_ID) {
        targetIndex = items.length - 1;
      } else if (overId.startsWith('today:')) {
        const overItemId = Number(overId.slice(6));
        targetIndex = items.findIndex((item) => item.id === overItemId);
      } else {
        return;
      }

      if (targetIndex === -1 || targetIndex === fromIndex) {
        return;
      }

      const reordered = arrayMove(items, fromIndex, targetIndex);
      await withRefresh(async () => {
        await reorderTodayPlanItems(reordered.map((item) => item.id));
      }, undefined, activeCategoryKey);
      return;
    }

    const task = findTaskByDragState();
    if (!task) {
      return;
    }

    if (overId === TODAY_CONTAINER_ID || overId.startsWith('today:')) {
      if (todaySourceTaskIds.has(task.id)) {
        setMessage('这条待办今天已经进入任务区了。');
        return;
      }

      let targetIndex = data.today_items.length;
      if (overId.startsWith('today:')) {
        const overItemId = Number(overId.slice(6));
        const foundIndex = data.today_items.findIndex((item) => item.id === overItemId);
        if (foundIndex !== -1) {
          targetIndex = foundIndex;
        }
      }

      await handleAddToToday(task, targetIndex);
      return;
    }

    if (!activeCategory) {
      return;
    }

    const tasks = [...activeCategory.pending_tasks];
    const fromIndex = tasks.findIndex((item) => item.id === task.id);
    if (fromIndex === -1) {
      return;
    }

    let targetIndex = tasks.length - 1;
    if (overId === CATEGORY_CONTAINER_ID) {
      targetIndex = tasks.length - 1;
    } else if (overId.startsWith('task:')) {
      const overTaskId = Number(overId.slice(5));
      targetIndex = tasks.findIndex((item) => item.id === overTaskId);
    } else {
      return;
    }

    if (targetIndex === -1 || targetIndex === fromIndex) {
      return;
    }

    const reordered = arrayMove(tasks, fromIndex, targetIndex);
    await withRefresh(async () => {
      await reorderChecklistTasks(activeCategory.key, reordered.map((item) => item.id));
    }, undefined, activeCategory.key);
  }

  if (loading && data === null) {
    return (
      <section className="page-shell checklist-shell">
        <div className="empty-state">
          <strong>正在载入清单</strong>
          <p>顶部进入任务区和五大分类正在准备中。</p>
        </div>
      </section>
    );
  }

  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={(event) => void handleDragEnd(event)} onDragOver={handleDragOver} onDragStart={handleDragStart}>
      <section className="page-shell checklist-shell checklist-clean-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">Task Console</p>
          <h2>进入任务与分类待办</h2>
          <p>先在政治、英语、数学、专业课、通用里整理待办，再把今天真正要做的事项拖入顶部进入任务区。</p>
        </div>
        <button className="secondary-action" disabled={saving} onClick={() => void initializePage(activeCategoryKey)} type="button">
          <RefreshCw size={17} />
          刷新
        </button>
      </header>

      {error && <p className="alert error">{error}</p>}
      {message && <p className="alert success">{message}</p>}

      <TodayPlanDrawer
        dndContainerId={TODAY_CONTAINER_ID}
        dndIsOver={dragOverId === TODAY_CONTAINER_ID}
        currentSubjectLabel={currentSubjectLabel}
        editingTodayDraft={editingTodayDraft}
        editingTodayId={editingTodayId}
        emptyDescription="从下方分类清单拖入，或者直接手动新建今天的临时任务。"
        emptyTitle="今天还没有进入任务"
        items={data?.today_items ?? []}
        onBeginEdit={beginEditTodayItem}
        onCancelEdit={() => setEditingTodayId(null)}
        onChangeEdit={(patch) => setEditingTodayDraft((current) => ({ ...(current ?? emptyTodayDraft), ...patch }))}
        onClose={undefined}
        onComplete={(target) => void handleCompleteTodayItem(target)}
        onCreate={() => void handleCreateTodayItem()}
        onDelete={(itemId) => void handleDeleteTodayItem(itemId)}
        onDraftChange={(patch) => setTodayDraft((current) => ({ ...current, ...patch }))}
        onRefresh={() => void initializePage(activeCategoryKey)}
        onSaveEdit={() => void handleSaveTodayEdit()}
        onToggleComposer={() => setShowTodayComposer((current) => !current)}
        saving={saving}
        showComposer={showTodayComposer}
        sortable
        subtitle="Today Queue"
        title="进入任务"
        getItemDragId={(item) => getTodaySortableId(item.id)}
        todayDate={data?.today_date ?? ''}
        todayDraft={todayDraft}
      />

      <section className="command-panel checklist-categories-panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Categories</p>
            <h3>五大分类</h3>
          </div>
          <span className="board-title-meta">
            <span>固定五类</span>
          </span>
        </div>

        <div className="checklist-category-tabs" role="tablist" aria-label="清单分类">
          {categories.map((category) => (
            <button
              aria-selected={activeCategoryKey === category.key}
              className={
                [
                  'category-tab',
                  activeCategoryKey === category.key ? 'active' : '',
                  highlightedCategoryKey === category.key ? 'is-highlighted' : '',
                ]
                  .filter(Boolean)
                  .join(' ')
              }
              key={category.key}
              onClick={() => handleSelectCategory(category.key)}
              role="tab"
              type="button"
            >
              <strong>{category.title}</strong>
              <small>{category.pending_tasks.length} 项待办</small>
            </button>
          ))}
        </div>

        {activeCategory && (
          <div className="checklist-category-body">
            <div className="category-surface-head">
              <div className="category-surface-title">
                <p className="eyebrow">Current Category</p>
                <h4>{activeCategory.title} 待办</h4>
              </div>
              <button
                aria-label={`新增 ${activeCategory.title} 待办`}
                className={composerCategoryKey === activeCategory.key ? 'small-action icon-action enabled' : 'small-action icon-action'}
                onClick={toggleActiveCategoryComposer}
                title={`新增 ${activeCategory.title} 待办`}
                type="button"
              >
                <Plus size={16} />
              </button>
            </div>

            {composerCategoryKey === activeCategory.key && (
              <div className="task-composer">
                <TaskEditor
                  draft={taskDrafts[activeCategory.key] ?? emptyTaskDraft(activeCategory.key)}
                  saving={saving}
                  titleLabel={`新增 ${activeCategory.title} 待办`}
                  onChange={(patch) => updateTaskDraft(activeCategory.key, patch)}
                  onSubmit={() => void handleCreateTask(activeCategory.key)}
                  submitOnEnter
                  submitLabel="加入分类清单"
                />
              </div>
            )}

            <CategoryDropArea isOver={dragOverId === CATEGORY_CONTAINER_ID}>
              {activeCategory.pending_tasks.length === 0 ? (
                <div className="empty-state">
                  <strong>{activeCategory.title} 还没有待办</strong>
                  <p>先在这里一条一条写好，再把今天要做的拖到上方进入任务区。</p>
                </div>
              ) : (
                <SortableContext items={activeCategory.pending_tasks.map((task) => getTaskSortableId(task.id))} strategy={verticalListSortingStrategy}>
                  <div className="category-task-list">
                    {activeCategory.pending_tasks.map((task) => (
                      <CategoryTaskRow
                        alreadyInToday={todaySourceTaskIds.has(task.id)}
                        editingTaskDraft={editingTaskDraft}
                        editingTaskId={editingTaskId}
                        saving={saving}
                        task={task}
                        onAddToToday={(target) => void handleAddToToday(target)}
                        onBeginEdit={beginEditTask}
                        onCancelEdit={() => setEditingTaskId(null)}
                        onChangeEdit={(patch) => setEditingTaskDraft((current) => ({ ...(current as ChecklistTaskDraft), ...patch }))}
                        onDelete={(taskId) => void handleDeleteTask(taskId)}
                        onSaveEdit={() => void handleSaveTaskEdit()}
                        onToggleComplete={(target) => void handleToggleTask(target)}
                      />
                    ))}
                  </div>
                </SortableContext>
              )}
            </CategoryDropArea>

            <div className="completed-section">
              <button
                className={`ghost-action completed-toggle${showCompleted[activeCategory.key] ? ' is-open' : ''}`}
                onClick={() => setShowCompleted((current) => ({ ...current, [activeCategory.key]: !current[activeCategory.key] }))}
                type="button"
              >
                <ChevronDown size={15} />
                已完成 {activeCategory.completed_tasks.length} 项
              </button>

              {showCompleted[activeCategory.key] && (
                <div className="completed-task-list">
                  {activeCategory.completed_tasks.length === 0 ? (
                    <div className="empty-state compact">这个分类暂时还没有已完成事项。</div>
                  ) : (
                    activeCategory.completed_tasks.map((task) => (
                      <article className="category-task-row is-completed" key={task.id}>
                        <div className="row-main">
                          <span className="row-icon enabled">
                            <Check size={18} />
                          </span>
                          <div>
                            <strong>{task.title}</strong>
                            <p className="category-task-meta">{buildTaskMeta(task)}</p>
                          </div>
                        </div>
                        <div className="row-actions">
                          <button className="small-action" disabled={saving} onClick={() => void handleToggleTask(task)} type="button">
                            恢复
                          </button>
                        </div>
                      </article>
                    ))
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </section>
      </section>
    </DndContext>
  );
}

function TaskEditor({
  draft,
  saving,
  titleLabel,
  onChange,
  onSubmit,
  submitOnEnter = false,
  submitLabel,
}: {
  draft: ChecklistTaskDraft | TodayPlanItemDraft;
  saving: boolean;
  titleLabel: string;
  onChange: (patch: Partial<ChecklistTaskDraft & TodayPlanItemDraft>) => void;
  onSubmit: () => void;
  submitOnEnter?: boolean;
  submitLabel: string;
}) {
  const canSubmit = !saving && Boolean(draft.title.trim());

  return (
    <div className="task-editor-grid compact">
      <label className="field-block">
        <span>{titleLabel}</span>
        <input
          className="text-input"
          onChange={(event) => onChange({ title: event.target.value })}
          onKeyDown={(event) => handleSubmitOnEnter(event, submitOnEnter && canSubmit, onSubmit)}
          placeholder="写下一条清晰的待办"
          value={draft.title}
        />
      </label>

      <label className="field-block">
        <span>截止日期</span>
        <input
          className="text-input compact-input"
          onChange={(event) => onChange({ dueDate: event.target.value })}
          type="date"
          value={draft.dueDate ?? ''}
        />
      </label>

      <label className="field-block">
        <span>备注</span>
        <textarea
          className="text-input textarea-input"
          onChange={(event) => onChange({ note: event.target.value })}
          placeholder="可以写步骤、资料来源或提醒"
          value={draft.note ?? ''}
        />
      </label>

      <button className="primary-action" disabled={!canSubmit} onClick={onSubmit} type="button">
        <ListPlus size={16} />
        {submitLabel}
      </button>
    </div>
  );
}

function CategoryDropArea({
  children,
  isOver = false,
}: {
  children: React.ReactNode;
  isOver?: boolean;
}) {
  const { setNodeRef } = useDroppable({
    id: CATEGORY_CONTAINER_ID,
  });

  return (
    <div className={['category-drop-area', isOver ? 'is-over' : ''].filter(Boolean).join(' ')} ref={setNodeRef}>
      {children}
    </div>
  );
}

function SortableHandle({
  id,
  className,
  children,
}: {
  id: string;
  className: string;
  children: React.ReactNode;
}) {
  const { attributes, listeners, setNodeRef } = useSortable({ id });

  return (
    <span className={className} ref={setNodeRef} {...attributes} {...listeners}>
      {children}
    </span>
  );
}

function CategoryTaskRow({
  task,
  alreadyInToday,
  editingTaskId,
  editingTaskDraft,
  saving,
  onAddToToday,
  onBeginEdit,
  onCancelEdit,
  onChangeEdit,
  onDelete,
  onSaveEdit,
  onToggleComplete,
}: {
  task: ChecklistTask;
  alreadyInToday: boolean;
  editingTaskId: number | null;
  editingTaskDraft: ChecklistTaskDraft | null;
  saving: boolean;
  onAddToToday: (task: ChecklistTask) => void;
  onBeginEdit: (task: ChecklistTask) => void;
  onCancelEdit: () => void;
  onChangeEdit: (patch: Partial<ChecklistTaskDraft>) => void;
  onDelete: (taskId: number) => void;
  onSaveEdit: () => void;
  onToggleComplete: (task: ChecklistTask) => void;
}) {
  const sortableId = getTaskSortableId(task.id);
  const { attributes, isDragging, listeners, setNodeRef, transform, transition } = useSortable({ id: sortableId });
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <article className={`category-task-row${isDragging ? ' is-dragging' : ''}`} ref={setNodeRef} style={style}>
      <div className="row-main">
        <span className="row-icon category-task-handle" {...attributes} {...listeners}>
          <GripVertical size={18} />
        </span>
        <div>
          <strong>{task.title}</strong>
          <p className="category-task-meta">{buildTaskMeta(task)}</p>
        </div>
      </div>

      <div className="row-actions">
        <button
          className={alreadyInToday ? 'small-action is-muted' : 'small-action enabled'}
          disabled={saving || alreadyInToday}
          onClick={() => onAddToToday(task)}
          title={alreadyInToday ? '今天已在任务区' : '加入今日任务'}
          type="button"
        >
          <CalendarCheck2 size={15} />
          {alreadyInToday ? '已加入今日' : '加入今日'}
        </button>
        <button className="small-action" disabled={saving} onClick={() => onToggleComplete(task)} type="button">
          <Check size={15} />
          完成
        </button>
        <button className="small-action" disabled={saving} onClick={() => onBeginEdit(task)} type="button">
          <Pencil size={15} />
          编辑
        </button>
        <button className="small-action danger" disabled={saving} onClick={() => onDelete(task.id)} type="button">
          <Trash2 size={15} />
          删除
        </button>
      </div>

      {editingTaskId === task.id && editingTaskDraft && (
        <div className="task-inline-editor">
          <TaskEditor
            draft={editingTaskDraft}
            saving={saving}
            titleLabel="编辑待办"
            onChange={onChangeEdit}
            onSubmit={onSaveEdit}
            submitLabel="保存待办"
          />
          <button className="ghost-action" onClick={onCancelEdit} type="button">
            取消
          </button>
        </div>
      )}
    </article>
  );
}
