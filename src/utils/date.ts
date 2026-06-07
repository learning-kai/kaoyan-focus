export function formatDateKey(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

export function currentMinuteOfDay(date = new Date()) {
  return date.getHours() * 60 + date.getMinutes();
}
