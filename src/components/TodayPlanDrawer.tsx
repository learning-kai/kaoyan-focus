import type { CSSProperties, KeyboardEvent, ReactNode } from 'react';
import { useDroppable } from '@dnd-kit/core';
import { SortableContext, useSortable, verticalListSortingStrategy } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Check, GripVertical, Pencil, Plus, RefreshCw, Trash2, X } from 'lucide-react';
import type { TodayPlanItem, TodayPlanItemDraft } from '../types/checklist';

type TodayPlanDrawerProps = {
  title?: string;
  subtitle?: string;
  todayDate: string;
  items: TodayPlanItem[];
  currentSubjectLabel?: string | null;
  saving: boolean;
  editingTodayId: number | null;
  editingTodayDraft: TodayPlanItemDraft;
  showComposer: boolean;
  todayDraft: TodayPlanItemDraft;
  emptyTitle: string;
  emptyDescription: string;
  submitLabel?: string;
  inlineTitleLabel?: string;
  compact?: boolean;
  isOpen?: boolean;
  variant?: 'panel' | 'drawer';
  sortable?: boolean;
  dndContainerId?: string;
  dndIsOver?: boolean;
  getItemDragId?: (item: TodayPlanItem) => string;
  onToggleComposer: () => void;
  onDraftChange: (patch: Partial<TodayPlanItemDraft>) => void;
  onCreate: () => void;
  onRefresh?: () => void;
  onClose?: () => void;
  onBeginEdit: (item: TodayPlanItem) => void;
  onCancelEdit: () => void;
  onChangeEdit: (patch: Partial<TodayPlanItemDraft>) => void;
  onComplete: (item: TodayPlanItem) => void;
  onDelete: (itemId: number) => void;
  onSaveEdit: () => void;
};

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

function TaskEditor({
  draft,
  saving,
  titleLabel,
  onChange,
  onSubmit,
  submitOnEnter = false,
  submitLabel,
}: {
  draft: TodayPlanItemDraft;
  saving: boolean;
  titleLabel: string;
  onChange: (patch: Partial<TodayPlanItemDraft>) => void;
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
          placeholder="写下一条清晰的任务"
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
        <Plus size={16} />
        {submitLabel}
      </button>
    </div>
  );
}

function TodayItemCardBase({
  item,
  editingTodayId,
  editingTodayDraft,
  saving,
  compact = false,
  dragHandle,
  isDragging = false,
  style,
  onBeginEdit,
  onCancelEdit,
  onChangeEdit,
  onComplete,
  onDelete,
  onSaveEdit,
}: {
  item: TodayPlanItem;
  editingTodayId: number | null;
  editingTodayDraft: TodayPlanItemDraft;
  saving: boolean;
  compact?: boolean;
  dragHandle?: ReactNode;
  isDragging?: boolean;
  style?: CSSProperties;
  onBeginEdit: (item: TodayPlanItem) => void;
  onCancelEdit: () => void;
  onChangeEdit: (patch: Partial<TodayPlanItemDraft>) => void;
  onComplete: (item: TodayPlanItem) => void;
  onDelete: (itemId: number) => void;
  onSaveEdit: () => void;
}) {
  return (
    <article
      className={`today-item-row${item.completed ? ' is-completed' : ''}${compact ? ' is-drawer' : ''}${isDragging ? ' is-dragging' : ''}`}
      style={style}
    >
      <div className="today-item-compact">
        {dragHandle}
        <button
          aria-label={item.completed ? '恢复为未完成' : '标记完成'}
          className={item.completed ? 'small-action icon-action enabled' : 'small-action icon-action'}
          disabled={saving}
          onClick={() => onComplete(item)}
          title={item.completed ? '恢复为未完成' : '标记完成'}
          type="button"
        >
          <Check size={13} />
        </button>

        <div className="today-item-copy" title={item.title}>
          <strong>{item.title}</strong>
        </div>

        <div className="row-actions today-item-actions">
          <button
            aria-label="编辑今日任务"
            className="small-action icon-action"
            disabled={saving}
            onClick={() => onBeginEdit(item)}
            title="编辑今日任务"
            type="button"
          >
            <Pencil size={13} />
          </button>
          <button
            aria-label="删除今日任务"
            className="small-action icon-action danger"
            disabled={saving}
            onClick={() => onDelete(item.id)}
            title="删除今日任务"
            type="button"
          >
            <Trash2 size={13} />
          </button>
        </div>
      </div>

      {editingTodayId === item.id && (
        <div className="task-inline-editor today-inline-editor">
          <TaskEditor
            draft={editingTodayDraft}
            saving={saving}
            titleLabel="编辑今日任务"
            onChange={onChangeEdit}
            onSubmit={onSaveEdit}
            submitLabel="保存今日任务"
          />
          <button className="ghost-action" onClick={onCancelEdit} type="button">
            取消
          </button>
        </div>
      )}
    </article>
  );
}

function SortableTodayItemCard({
  dragId,
  ...props
}: {
  dragId: string;
} & Omit<Parameters<typeof TodayItemCardBase>[0], 'dragHandle' | 'isDragging' | 'style'>) {
  const { attributes, isDragging, listeners, setActivatorNodeRef, setNodeRef, transform, transition } = useSortable({
    id: dragId,
  });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div ref={setNodeRef}>
      <TodayItemCardBase
        {...props}
        dragHandle={(
          <button
            aria-label="拖动排序"
            className="row-icon today-item-handle"
            ref={setActivatorNodeRef}
            title="拖动排序"
            type="button"
            {...attributes}
            {...listeners}
          >
            <GripVertical size={13} />
          </button>
        )}
        isDragging={isDragging}
        style={style}
      />
    </div>
  );
}

function TodayDropArea({
  id,
  isOver,
  children,
}: {
  id: string;
  isOver?: boolean;
  children: ReactNode;
}) {
  const { setNodeRef } = useDroppable({ id });

  return (
    <div className={`today-drop-area${isOver ? ' is-over' : ''}`} ref={setNodeRef}>
      {children}
    </div>
  );
}

export default function TodayPlanDrawer({
  title = '今日任务',
  subtitle = '今日队列',
  todayDate,
  items,
  currentSubjectLabel,
  saving,
  editingTodayId,
  editingTodayDraft,
  showComposer,
  todayDraft,
  emptyTitle,
  emptyDescription,
  submitLabel = '加入今日任务',
  inlineTitleLabel = '新增今日任务',
  compact = false,
  isOpen = true,
  variant = 'panel',
  sortable = false,
  dndContainerId,
  dndIsOver = false,
  getItemDragId,
  onToggleComposer,
  onDraftChange,
  onCreate,
  onRefresh,
  onClose,
  onBeginEdit,
  onCancelEdit,
  onChangeEdit,
  onComplete,
  onDelete,
  onSaveEdit,
}: TodayPlanDrawerProps) {
  const rootClassName = [
    variant === 'drawer' ? 'today-plan-drawer' : 'command-panel today-plan-panel',
    compact ? 'is-compact' : '',
    isOpen ? 'is-open' : '',
    sortable ? 'is-sortable' : '',
  ]
    .filter(Boolean)
    .join(' ');

  const canSort = sortable && Boolean(dndContainerId) && Boolean(getItemDragId);
  const sortableIds = canSort && getItemDragId ? items.map((item) => getItemDragId(item)) : [];
  const isDrawerVariant = variant === 'drawer';
  const blockGlobalShortcuts = isDrawerVariant && isOpen;
  const drawerHidden = isDrawerVariant && !isOpen;
  const content = items.length === 0 ? (
    <div className={`empty-state empty-drop-zone${compact ? ' compact' : ''}`}>
      <strong>{emptyTitle}</strong>
      <p>{emptyDescription}</p>
    </div>
  ) : (
    <div className={`today-plan-list${compact ? ' is-drawer' : ''}`}>
      {items.map((item) => {
        const sharedProps = {
          compact,
          editingTodayDraft,
          editingTodayId,
          item,
          onBeginEdit,
          onCancelEdit,
          onChangeEdit,
          onComplete,
          onDelete,
          onSaveEdit,
          saving,
        };

        if (canSort && getItemDragId) {
          return <SortableTodayItemCard {...sharedProps} dragId={getItemDragId(item)} key={item.id} />;
        }

        return <TodayItemCardBase {...sharedProps} key={item.id} />;
      })}
    </div>
  );

  return (
    <section
      aria-hidden={drawerHidden ? true : undefined}
      className={rootClassName}
      data-block-global-shortcuts={blockGlobalShortcuts ? 'true' : undefined}
      inert={drawerHidden ? true : undefined}
    >
      <div className="panel-title today-drawer-head">
        <div>
          <p className="eyebrow">{subtitle}</p>
          <h3>{title}</h3>
        </div>
        <div className="today-drawer-tools">
          {onRefresh && (
            <button className={isDrawerVariant ? 'focus-hud-card today-drawer-tool-card today-drawer-tool-icon' : 'small-action icon-action'} disabled={saving} onClick={onRefresh} title="刷新今日任务" type="button">
              <RefreshCw size={15} />
            </button>
          )}
          <button className={isDrawerVariant ? 'focus-hud-card today-drawer-tool-card today-drawer-add' : 'primary-action today-drawer-add'} disabled={saving} onClick={onToggleComposer} type="button">
            {isDrawerVariant ? (
              <>
                <span className="focus-hud-icon"><Plus size={16} /></span>
                <span className="focus-hud-copy">
                  <span>{showComposer ? '收起' : '新增'}</span>
                  <strong>今日任务</strong>
                </span>
              </>
            ) : (
              <>
                <Plus size={16} />
                {showComposer ? '收起' : '新增'}
              </>
            )}
          </button>
          {onClose && (
            <button aria-label="关闭今日任务" className={isDrawerVariant ? 'focus-hud-card today-drawer-tool-card today-drawer-tool-icon' : 'small-action icon-action'} onClick={onClose} title="关闭今日任务" type="button">
              <X size={15} />
            </button>
          )}
        </div>
      </div>

      <div className="today-plan-meta">
        <span>日期 {todayDate}</span>
        <span>{items.length} 项</span>
        {currentSubjectLabel && <span>当前科目 {currentSubjectLabel}</span>}
      </div>

      {showComposer && (
        <div className="today-composer">
          <TaskEditor
            draft={todayDraft}
            saving={saving}
            titleLabel={inlineTitleLabel}
            onChange={onDraftChange}
            onSubmit={onCreate}
            submitOnEnter
            submitLabel={submitLabel}
          />
        </div>
      )}

      {canSort && dndContainerId ? (
        <TodayDropArea id={dndContainerId} isOver={dndIsOver}>
          <SortableContext items={sortableIds} strategy={verticalListSortingStrategy}>
            {content}
          </SortableContext>
        </TodayDropArea>
      ) : (
        content
      )}
    </section>
  );
}
