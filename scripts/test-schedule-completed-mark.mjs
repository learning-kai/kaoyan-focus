import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(process.cwd());

const required = [
  ['src/pages/SchedulePage.tsx', 'isScheduleBlockCompleted'],
  ['src/pages/SchedulePage.tsx', 'scheduleBlockStatusClass'],
  ['src/pages/SchedulePage.tsx', 'schedule-completed-badge'],
  ['src/pages/SchedulePage.tsx', 'handleCompleteScheduleBlock'],
  ['src/services/scheduleApi.ts', 'setScheduleBlockCompleted'],
  ['src-tauri/src/lib.rs', 'commands::schedule::set_schedule_block_completed'],
  ['src-tauri/src/commands/schedule.rs', 'pub fn set_schedule_block_completed'],
  ['src/components/ScheduleDrawer.tsx', 'isScheduleBlockCompleted'],
  ['src/components/ScheduleDrawer.tsx', 'scheduleBlockStatusClass'],
  ['src/components/ScheduleDrawer.tsx', 'schedule-completed-badge'],
  ['src/styles.css', '.schedule-block.is-completed'],
  ['src/styles.css', '.week-block.is-completed'],
  ['src/styles.css', '.schedule-drawer-block.is-completed'],
  ['src-tauri/src/commands/checklist.rs', 'cascade_schedule_blocks_for_today_item_completion'],
  ['src-tauri/src/commands/schedule.rs', 'cascade_schedule_blocks_for_today_item_completion'],
  ['src-tauri/src/commands/schedule.rs', "status != 'running'"],
  ['src-tauri/src/commands/schedule.rs', 'completing_today_item_cascades_planned_schedule_block_to_completed'],
  ['src-tauri/src/commands/schedule.rs', 'uncompleting_today_item_restores_completed_block_but_keeps_running'],
];

let failed = false;
for (const [file, needle] of required) {
  const content = readFileSync(resolve(root, file), 'utf8');
  if (!content.includes(needle)) {
    console.error(`Missing ${JSON.stringify(needle)} in ${file}`);
    failed = true;
  }
}

const schedulePage = readFileSync(resolve(root, 'src/pages/SchedulePage.tsx'), 'utf8');
const drawer = readFileSync(resolve(root, 'src/components/ScheduleDrawer.tsx'), 'utf8');

if (!schedulePage.includes('calendarDragActivationDistance = 8')) {
  console.error('Schedule timeline drag must require an 8px activation distance');
  failed = true;
}

const pointerDownMatch = schedulePage.match(
  /function handleBlockPointerDown[\s\S]*?function handleResizePointerDown/,
);
if (!pointerDownMatch || !pointerDownMatch[0].includes('setPendingBlockDragState')) {
  console.error('Schedule block pointer down must create a pending gesture');
  failed = true;
} else if (pointerDownMatch[0].includes("startBlockDrag(block, 'move'")) {
  console.error('Schedule block pointer down starts dragging before the activation threshold');
  failed = true;
}

const completeToggleMatch = schedulePage.match(
  /className=\{blockCompleted[\s\S]*?handleCompleteScheduleBlock[\s\S]*?<Check size=\{14\} \/>/,
);
if (!completeToggleMatch || !completeToggleMatch[0].includes('onPointerDown')) {
  console.error('Schedule completion toggle must isolate pointer down from block dragging');
  failed = true;
}

const deleteActionMatch = schedulePage.match(
  /className="is-delete-action"[\s\S]*?<Trash2 size=\{14\} \/>/,
);
if (!deleteActionMatch || !deleteActionMatch[0].includes('onPointerDown')) {
  console.error('Schedule delete action must isolate pointer down from block dragging');
  failed = true;
}

const hig = readFileSync(resolve(root, 'src/apple-hig.css'), 'utf8');
if (!hig.includes('.schedule-block-actions .is-delete-action')) {
  console.error('Schedule delete action is missing its enlarged hit target');
  failed = true;
}

if (!schedulePage.includes('blockedTodayItemDragRef.current === itemId')) {
  console.error('Today-item completion control must block native drag activation');
  failed = true;
}

const dayClassMatch = schedulePage.match(/className=\{`schedule-block[^`]*`\}/);
if (!dayClassMatch || !dayClassMatch[0].includes('scheduleBlockStatusClass')) {
  console.error('SchedulePage day view missing completed class binding');
  failed = true;
}
const weekClassMatch = schedulePage.match(/className=\{`week-block[^`]*`\}/);
if (!weekClassMatch || !weekClassMatch[0].includes('scheduleBlockStatusClass')) {
  console.error('SchedulePage week view missing completed class binding');
  failed = true;
}
const drawerClassMatch = drawer.match(/className=\{`schedule-drawer-block[^`]*`\}/);
if (!drawerClassMatch || !drawerClassMatch[0].includes('scheduleBlockStatusClass')) {
  console.error('ScheduleDrawer missing completed class binding');
  failed = true;
}
if (!schedulePage.includes('已完成') || !drawer.includes('已完成')) {
  console.error('Missing 已完成 aria/status label text');
  failed = true;
}

const quickAddOptionsMatch = schedulePage.match(
  /<select value=\{quickAddSourceTodayItemId[\s\S]*?<\/select>/,
);
if (!quickAddOptionsMatch?.[0].includes('.filter((item) => !item.completed)')) {
  console.error('Schedule quick add must exclude completed today items');
  failed = true;
}

if (failed) {
  process.exit(1);
}

console.log('Schedule completed mark source assertions passed');
