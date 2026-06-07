export type AlarmStatus = 'scheduled' | 'ringing' | 'dismissed' | string;

export type Alarm = {
  id: number;
  title: string;
  note: string | null;
  alarm_date: string;
  alarm_time: string;
  alarm_at: string;
  enabled: boolean;
  status: AlarmStatus;
  fired_at: string | null;
  dismissed_at: string | null;
  created_at: string;
  updated_at: string;
};

export type AlarmDraft = {
  title: string;
  note?: string | null;
  alarmDate: string;
  alarmTime: string;
  enabled: boolean;
};
