$ErrorActionPreference = "Stop"

$dbPath = "C:\Users\Lenovo\AppData\Roaming\com.kaoyan.focus\kaoyan-focus.sqlite3"

if (-not (Test-Path -LiteralPath $dbPath)) {
  throw "Database not found: $dbPath"
}

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
  throw "Python not found in PATH."
}

$script = @'
import sqlite3
import shutil
from datetime import datetime, timezone
from pathlib import Path

path = Path(r"C:\Users\Lenovo\AppData\Roaming\com.kaoyan.focus\kaoyan-focus.sqlite3")
stamp = datetime.now().strftime("%Y%m%d%H%M%S")
backup = path.with_name(f"kaoyan-focus.before-manual-unlock-{stamp}.sqlite3")

source = sqlite3.connect(str(path))
dest = sqlite3.connect(str(backup))
try:
    source.backup(dest)
finally:
    dest.close()
    source.close()

now = datetime.now(timezone.utc).isoformat(timespec="microseconds")
now_ms = int(datetime.now(timezone.utc).timestamp() * 1000)

conn = sqlite3.connect(str(path))
conn.row_factory = sqlite3.Row
try:
    conn.execute("BEGIN IMMEDIATE")
    active_rows = conn.execute(
        "SELECT id, current_session_id FROM study_modes WHERE status = 'active'"
    ).fetchall()
    session_ids = [row["current_session_id"] for row in active_rows if row["current_session_id"] is not None]

    conn.execute(
        """
        UPDATE study_modes
        SET phase = 'emergency_exited',
            status = 'emergency_exited',
            finish_reason = 'emergency_exit',
            ended_at = ?,
            current_session_id = NULL,
            paused_at = NULL,
            updated_at = ?,
            state_revision = COALESCE(state_revision, 0) + 1,
            last_control_action = 'emergency_exit',
            last_control_at = ?
        WHERE status = 'active'
        """,
        (now, now, now_ms),
    )

    changed_sessions = 0
    if session_ids:
        placeholders = ",".join("?" for _ in session_ids)
        before = conn.total_changes
        conn.execute(
            f"""
            UPDATE focus_sessions
            SET status = 'emergency_exited',
                end_reason = 'emergency_exit',
                ended_at = ?,
                updated_at = ?,
                emergency_exit_count = emergency_exit_count + 1
            WHERE id IN ({placeholders})
              AND status = 'running'
            """,
            [now, now, *session_ids],
        )
        changed_sessions = conn.total_changes - before

    before = conn.total_changes
    conn.execute(
        """
        UPDATE settings
        SET value = 'normal',
            updated_at = ?
        WHERE key = 'default_focus_mode'
        """,
        (now,),
    )
    if conn.total_changes == before:
        conn.execute(
            """
            INSERT INTO settings (key, value, updated_at)
            VALUES ('default_focus_mode', 'normal', ?)
            """,
            (now,),
        )

    conn.commit()

    print(f"Unlock complete.")
    print(f"Database: {path}")
    print(f"Backup:   {backup}")
    print(f"Study modes updated: {len(active_rows)}")
    print(f"Focus sessions updated: {changed_sessions}")
finally:
    conn.close()
'@

$tempScript = Join-Path ([System.IO.Path]::GetTempPath()) ("unlock-kaoyan-focus-{0}.py" -f [System.Guid]::NewGuid().ToString("N"))
try {
  $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
  [System.IO.File]::WriteAllText($tempScript, $script, $utf8NoBom)
  & $python.Source $tempScript
}
finally {
  if (Test-Path -LiteralPath $tempScript) {
    Remove-Item -LiteralPath $tempScript -Force
  }
}
