import type { EmailReminderResult, EmailReminderSettings } from '../types/settings';
import { invokeCommand } from './tauriInvoke';

export function getEmailReminderSettings(): Promise<EmailReminderSettings> {
  return invokeCommand<EmailReminderSettings>('get_email_reminder_settings');
}

export function saveEmailReminderSettings(settings: EmailReminderSettings): Promise<EmailReminderSettings> {
  return invokeCommand<EmailReminderSettings>('save_email_reminder_settings', { settings });
}

export function testEmailReminder(settings: EmailReminderSettings): Promise<EmailReminderResult> {
  return invokeCommand<EmailReminderResult>('test_email_reminder', { settings });
}

export function checkDueTaskEmailReminders(): Promise<EmailReminderResult> {
  return invokeCommand<EmailReminderResult>('check_due_task_email_reminders');
}
